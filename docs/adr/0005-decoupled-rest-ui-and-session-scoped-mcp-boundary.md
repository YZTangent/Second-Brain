# 0005: Decoupled REST/WebSocket UI Contract and Restricted Session-Scoped MCP Surface

`braind` exposes an OpenAPI 3.1 REST + WebSocket/SSE API (`/api/v1/*`) completely decoupled from any frontend implementation. Human/UI-only capabilities—direct node `PUT / DELETE` on `manual`, full graph visualization (`/api/v1/graph`), commit audit and revert (`/api/v1/audit/*`), worker controls (`/api/v1/workers`), and live event streams (`/api/v1/events`)—are excluded from the agent MCP surface (`/mcp`), which only exposes session-owned worktree transactions (`/api/v1/tx/*`) and read/search queries.
