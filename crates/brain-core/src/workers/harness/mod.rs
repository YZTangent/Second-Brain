pub mod jetski;

pub use jetski::JetskiHeadlessHarness;

use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

#[async_trait]
pub trait HeadlessHarness: Send + Sync {
    fn harness_id(&self) -> &str;
    async fn run_prompt(&self, prompt: &str, timeout: Duration) -> Result<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_jetski_headless_harness_executes_without_shell() {
        let harness = JetskiHeadlessHarness::new("echo", vec!["-n".to_string()]);
        assert_eq!(harness.harness_id(), "jetski");
        let out = harness
            .run_prompt("synthesized [[knowledge:k1]]", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(out, "synthesized [[knowledge:k1]]");

        assert!(harness
            .run_prompt("", Duration::from_secs(5))
            .await
            .is_err());
    }
}
