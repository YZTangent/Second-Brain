use brain_core::index::BrainIndex;
use brain_core::model::NodeDocument;
use brain_core::tx::session::{validate_branch, BRANCH_MAIN};
use brain_core::tx::WorktreeCoordinator;
use brain_core::workers::sync::load_external_cache;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, Write};

pub const TOOL_BEGIN_TX: &str = "brain_begin_tx";
pub const TOOL_TX_UPSERT: &str = "brain_tx_upsert";
pub const TOOL_TX_DELETE: &str = "brain_tx_delete";
pub const TOOL_COMMIT_TX: &str = "brain_commit_tx";
pub const TOOL_ABORT_TX: &str = "brain_abort_tx";
pub const TOOL_SEARCH: &str = "brain_search";
pub const TOOL_DEEP_SEARCH: &str = "brain_deep_search";
pub const TOOL_GET_NODE: &str = "brain_get_node";
pub const TOOL_TIMELINE: &str = "brain_timeline";

pub const AGENT_MCP_TOOLS: &[&str] = &[
    TOOL_BEGIN_TX,
    TOOL_TX_UPSERT,
    TOOL_TX_DELETE,
    TOOL_COMMIT_TX,
    TOOL_ABORT_TX,
    TOOL_SEARCH,
    TOOL_DEEP_SEARCH,
    TOOL_GET_NODE,
    TOOL_TIMELINE,
];

const JSONRPC_VERSION: &str = "2.0";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const ERR_PARSE_ERROR: i64 = -32700;
const ERR_METHOD_NOT_FOUND: i64 = -32601;
const ERR_INVALID_PARAMS: i64 = -32602;
const ERR_INTERNAL: i64 = -32000;
const DEFAULT_ACTOR: &str = "agent:mcp";
const DEFAULT_SEARCH_LIMIT: usize = 10;
const MAX_SEARCH_LIMIT: usize = 100;
const MAX_SUMMARY_LEN: usize = 512;
const BRAIN_RUNTIME_DIR: &str = ".brain";

pub struct McpServer {
    pub coord: WorktreeCoordinator,
    pub session_id: String,
}

impl McpServer {
    pub fn new(coord: WorktreeCoordinator, session_id: impl Into<String>) -> Self {
        Self {
            coord,
            session_id: session_id.into(),
        }
    }

    pub fn run_stdio(&self) -> anyhow::Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        for line in stdin.lock().lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let resp = match serde_json::from_str::<Value>(trimmed) {
                Ok(req) => self.handle_request(&req),
                Err(err) => Self::err_response(
                    Value::Null,
                    ERR_PARSE_ERROR,
                    format!("parse error: {}", err),
                ),
            };
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
            stdout.flush()?;
        }
        Ok(())
    }

    pub fn handle_request(&self, req: &Value) -> Value {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or_else(|| json!({}));

        match method {
            "initialize" => Self::ok_response(
                id,
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "serverInfo": {
                        "name": "brainctl-mcp",
                        "version": "0.1.0"
                    },
                    "capabilities": {
                        "tools": {}
                    }
                }),
            ),
            "tools/list" => Self::ok_response(
                id,
                json!({
                    "tools": Self::tool_definitions(),
                }),
            ),
            "tools/call" => {
                let Some(tool_name) = params.get("name").and_then(Value::as_str) else {
                    return Self::err_response(id, ERR_INVALID_PARAMS, "missing tool name");
                };
                if !AGENT_MCP_TOOLS.contains(&tool_name) {
                    return Self::err_response(
                        id,
                        ERR_METHOD_NOT_FOUND,
                        format!(
                            "tool '{}' is not permitted on the restricted agent MCP surface",
                            tool_name
                        ),
                    );
                }
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match self.dispatch_tool(tool_name, &args) {
                    Ok(result) => Self::ok_response(id, result),
                    Err((code, msg)) => Self::err_response(id, code, msg),
                }
            }
            _ => Self::err_response(
                id,
                ERR_METHOD_NOT_FOUND,
                format!("unsupported JSON-RPC method: {}", method),
            ),
        }
    }

    fn dispatch_tool(&self, name: &str, args: &Value) -> Result<Value, (i64, String)> {
        if self.session_id.trim().is_empty() || self.session_id.chars().any(|c| c.is_control()) {
            return Err((ERR_INVALID_PARAMS, "invalid MCP session_id".to_string()));
        }

        match name {
            TOOL_BEGIN_TX => {
                let handle = self
                    .coord
                    .begin_tx(&self.session_id)
                    .map_err(|e| (ERR_INTERNAL, e.to_string()))?;
                Ok(json!({
                    "tx_id": handle.tx_id,
                    "owner_session_id": handle.owner_session_id,
                    "worktree_path": handle.worktree_path,
                }))
            }
            TOOL_TX_UPSERT => {
                let tx_id = Self::req_str(args, "tx_id")?;
                let path = Self::req_str(args, "path")?;
                let markdown = Self::req_str(args, "markdown")?;
                let doc = NodeDocument::parse(path, markdown);
                self.coord
                    .stage_upsert(tx_id, &self.session_id, path, &doc)
                    .map_err(|e| (ERR_INTERNAL, e.to_string()))?;
                Ok(json!({
                    "status": "staged",
                    "tx_id": tx_id,
                    "path": path,
                    "node_id": doc.meta.id,
                }))
            }
            TOOL_TX_DELETE => {
                let tx_id = Self::req_str(args, "tx_id")?;
                let path = Self::req_str(args, "path")?;
                self.coord
                    .stage_delete(tx_id, &self.session_id, path)
                    .map_err(|e| (ERR_INTERNAL, e.to_string()))?;
                Ok(json!({
                    "status": "deleted",
                    "tx_id": tx_id,
                    "path": path,
                }))
            }
            TOOL_COMMIT_TX => {
                let tx_id = Self::req_str(args, "tx_id")?;
                let summary = Self::req_str(args, "summary")?.trim();
                let actor = args
                    .get("actor")
                    .and_then(Value::as_str)
                    .unwrap_or(DEFAULT_ACTOR)
                    .trim();
                if summary.is_empty()
                    || summary.len() > MAX_SUMMARY_LEN
                    || summary.chars().any(|c| c.is_control())
                    || actor.is_empty()
                    || actor.chars().any(|c| c.is_control())
                {
                    return Err((
                        ERR_INVALID_PARAMS,
                        "invalid commit summary or actor".to_string(),
                    ));
                }
                let sha = self
                    .coord
                    .commit_tx(tx_id, &self.session_id, actor, summary)
                    .map_err(|e| (ERR_INTERNAL, e.to_string()))?;
                Ok(json!({
                    "status": "committed",
                    "tx_id": tx_id,
                    "sha": sha,
                }))
            }
            TOOL_ABORT_TX => {
                let tx_id = Self::req_str(args, "tx_id")?;
                self.coord
                    .abort_tx(tx_id, &self.session_id)
                    .map_err(|e| (ERR_INTERNAL, e.to_string()))?;
                Ok(json!({
                    "status": "aborted",
                    "tx_id": tx_id,
                }))
            }
            TOOL_SEARCH | TOOL_DEEP_SEARCH => {
                let query = Self::req_str(args, "query")?;
                let branch = args
                    .get("branch")
                    .and_then(Value::as_str)
                    .unwrap_or(BRANCH_MAIN);
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|n| n as usize)
                    .unwrap_or(DEFAULT_SEARCH_LIMIT)
                    .clamp(1, MAX_SEARCH_LIMIT);
                let idx = self.load_branch_index(branch)?;
                let hits = idx.search_rrf(query, limit);
                let cited_nodes: Vec<String> = hits.iter().map(|h| h.node_id.clone()).collect();
                Ok(json!({
                    "branch": branch,
                    "query": query,
                    "hits": hits,
                    "cited_nodes": cited_nodes,
                }))
            }
            TOOL_GET_NODE => {
                let node_id = Self::req_str(args, "node_id")?;
                let branch = args
                    .get("branch")
                    .and_then(Value::as_str)
                    .unwrap_or(BRANCH_MAIN);
                let idx = self.load_branch_index(branch)?;
                let doc = idx
                    .get_node(node_id)
                    .ok_or_else(|| (ERR_INTERNAL, format!("node not found: {}", node_id)))?;
                let neighbors = idx.neighbors_1hop(node_id);
                let ext = load_external_cache(&self.coord.vault_root().join(BRAIN_RUNTIME_DIR))
                    .unwrap_or_default();
                let external_bug = doc.meta.bug.as_ref().and_then(|b| ext.bugs.get(b).cloned());
                let external_change = doc
                    .meta
                    .change
                    .as_ref()
                    .and_then(|c| ext.changes.get(c).cloned());
                Ok(json!({
                    "branch": branch,
                    "document": doc,
                    "neighbors_1hop": neighbors,
                    "external_bug": external_bug,
                    "external_change": external_change,
                }))
            }
            TOOL_TIMELINE => {
                let branch = args
                    .get("branch")
                    .and_then(Value::as_str)
                    .unwrap_or(BRANCH_MAIN);
                let idx = self.load_branch_index(branch)?;
                let ext = load_external_cache(&self.coord.vault_root().join(BRAIN_RUNTIME_DIR))
                    .unwrap_or_default();
                let tl = idx.build_timeline(&ext);
                Ok(json!({
                    "branch": branch,
                    "tasks": tl.tasks,
                    "unlinked_bugs": tl.unlinked_bugs,
                    "unlinked_changes": tl.unlinked_changes,
                    "journals": tl.journals,
                }))
            }
            _ => Err((ERR_METHOD_NOT_FOUND, format!("unknown MCP tool: {}", name))),
        }
    }

    fn load_branch_index(&self, branch: &str) -> Result<BrainIndex, (i64, String)> {
        validate_branch(branch).map_err(|e| (ERR_INVALID_PARAMS, e.to_string()))?;
        let docs = self
            .coord
            .load_branch_documents(branch)
            .map_err(|e| (ERR_INTERNAL, e.to_string()))?;
        let mut idx = BrainIndex::default();
        idx.rebuild_from_docs(&docs, &HashMap::new());
        Ok(idx)
    }

    fn req_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, (i64, String)> {
        args.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                (
                    ERR_INVALID_PARAMS,
                    format!("missing or empty required string argument '{}'", key),
                )
            })
    }

    fn tool_definitions() -> Vec<Value> {
        vec![
            json!({
                "name": TOOL_BEGIN_TX,
                "description": "Begin an isolated worktree transaction off main bound to the active session",
                "inputSchema": { "type": "object", "properties": {} }
            }),
            json!({
                "name": TOOL_TX_UPSERT,
                "description": "Stage a Markdown node upsert inside an active transaction worktree",
                "inputSchema": {
                    "type": "object",
                    "required": ["tx_id", "path", "markdown"],
                    "properties": {
                        "tx_id": { "type": "string" },
                        "path": { "type": "string" },
                        "markdown": { "type": "string" }
                    }
                }
            }),
            json!({
                "name": TOOL_TX_DELETE,
                "description": "Stage a Markdown node deletion inside an active transaction worktree",
                "inputSchema": {
                    "type": "object",
                    "required": ["tx_id", "path"],
                    "properties": {
                        "tx_id": { "type": "string" },
                        "path": { "type": "string" }
                    }
                }
            }),
            json!({
                "name": TOOL_COMMIT_TX,
                "description": "Atomically commit the transaction worktree onto main and sync manual",
                "inputSchema": {
                    "type": "object",
                    "required": ["tx_id", "summary"],
                    "properties": {
                        "tx_id": { "type": "string" },
                        "summary": { "type": "string" },
                        "actor": { "type": "string" }
                    }
                }
            }),
            json!({
                "name": TOOL_ABORT_TX,
                "description": "Abort and discard an active transaction worktree",
                "inputSchema": {
                    "type": "object",
                    "required": ["tx_id"],
                    "properties": {
                        "tx_id": { "type": "string" }
                    }
                }
            }),
            json!({
                "name": TOOL_SEARCH,
                "description": "Tier-1 Heuristic RRF Search across the Second Brain vault",
                "inputSchema": {
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": { "type": "string" },
                        "branch": { "type": "string" },
                        "limit": { "type": "integer" }
                    }
                }
            }),
            json!({
                "name": TOOL_DEEP_SEARCH,
                "description": "Tier-2 Agentic Deep Search across multi-hop graph links",
                "inputSchema": {
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": { "type": "string" },
                        "branch": { "type": "string" }
                    }
                }
            }),
            json!({
                "name": TOOL_GET_NODE,
                "description": "Read a single node and its 1-hop graph neighborhood",
                "inputSchema": {
                    "type": "object",
                    "required": ["node_id"],
                    "properties": {
                        "node_id": { "type": "string" },
                        "branch": { "type": "string" }
                    }
                }
            }),
            json!({
                "name": TOOL_TIMELINE,
                "description": "Read the live Daily/Weekly timeline projection of open tasks, bugs, changes, and journals",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "branch": { "type": "string" }
                    }
                }
            }),
        ]
    }

    fn ok_response(id: Value, result: Value) -> Value {
        json!({
            "jsonrpc": JSONRPC_VERSION,
            "id": id,
            "result": result,
        })
    }

    fn err_response(id: Value, code: i64, message: impl Into<String>) -> Value {
        json!({
            "jsonrpc": JSONRPC_VERSION,
            "id": id,
            "error": {
                "code": code,
                "message": message.into(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_agent_mcp_tools_exclude_ui_only_endpoints() {
        let expected = [
            "brain_begin_tx",
            "brain_tx_upsert",
            "brain_tx_delete",
            "brain_commit_tx",
            "brain_abort_tx",
            "brain_search",
            "brain_deep_search",
            "brain_get_node",
            "brain_timeline",
        ];
        assert_eq!(AGENT_MCP_TOOLS, &expected);

        for forbidden in [
            "brain_direct_upsert",
            "brain_direct_delete",
            "brain_graph",
            "brain_audit",
            "brain_revert",
            "brain_workers",
            "brain_events",
        ] {
            assert!(
                !AGENT_MCP_TOOLS.contains(&forbidden),
                "UI-only tool '{}' must not be exposed on agent MCP surface",
                forbidden
            );
        }

        let tmp = TempDir::new().unwrap();
        let coord = WorktreeCoordinator::new(tmp.path().join("vault")).unwrap();
        let server = McpServer::new(coord, "mcp-sess-1");

        // Verify tools/list returns the exact 9 agent tools
        let list_resp = server.handle_request(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        }));
        let tools = list_resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 9);

        // Verify calling a forbidden UI-only tool is rejected
        let forbidden_resp = server.handle_request(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "brain_revert",
                "arguments": { "sha": "abcd1234" }
            }
        }));
        assert!(forbidden_resp.get("error").is_some());

        // Verify end-to-end atomic tx flow through MCP tools/call
        let begin_resp = server.handle_request(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "brain_begin_tx",
                "arguments": {}
            }
        }));
        let tx_id = begin_resp["result"]["tx_id"].as_str().unwrap().to_string();

        let upsert_resp = server.handle_request(&json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "brain_tx_upsert",
                "arguments": {
                    "tx_id": tx_id,
                    "path": "knowledge/mcp-note.md",
                    "markdown": "---\nid: knowledge:mcp-note\ntype: knowledge\ntitle: MCP Note\ntags: [mcp]\nupdated_at: \"2026-11-01T00:00:00Z\"\n---\nSaved via MCP transaction.\n"
                }
            }
        }));
        assert_eq!(upsert_resp["result"]["status"], "staged");

        let commit_resp = server.handle_request(&json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "brain_commit_tx",
                "arguments": {
                    "tx_id": tx_id,
                    "summary": "remember: mcp note"
                }
            }
        }));
        assert_eq!(commit_resp["result"]["status"], "committed");

        let search_resp = server.handle_request(&json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "brain_search",
                "arguments": {
                    "query": "MCP Note",
                    "limit": 5
                }
            }
        }));
        assert_eq!(
            search_resp["result"]["hits"][0]["node_id"],
            "knowledge:mcp-note"
        );
    }
}
