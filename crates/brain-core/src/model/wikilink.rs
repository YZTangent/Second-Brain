use regex::Regex;
use std::sync::OnceLock;

const WIKILINK_PATTERN: &str = r"\[\[([^\[\]#|]+)(?:#[^\[\]|]+)?(?:\|[^\[\]]+)?\]\]";

pub fn extract_wikilinks(markdown_body: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(WIKILINK_PATTERN).expect("valid wikilink regex"));
    let mut out = Vec::new();
    for cap in re.captures_iter(markdown_body) {
        let link = cap[1].trim().to_string();
        if !out.contains(&link) {
            out.push(link);
        }
    }
    out
}
