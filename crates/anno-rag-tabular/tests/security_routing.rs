//! Security regression: the local-first routing client must NOT call the
//! remote fallback unless one was explicitly attached (allow_remote=true).

use anno_rag_tabular::llm::local::client::{LocalEntity, LocalEntityExtractor, LocalTabularClient};
use anno_rag_tabular::llm::routing::RoutingLlmClient;
use anno_rag_tabular::llm::{LlmClient, StructuredOutput, Usage};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct CountingRemote(Arc<AtomicUsize>);

#[async_trait]
impl LlmClient for CountingRemote {
    async fn generate_structured(
        &self,
        _s: &str,
        _u: &str,
        _schema: &Value,
    ) -> anno_rag_tabular::error::Result<StructuredOutput> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(StructuredOutput {
            value: json!({}),
            usage: Usage::default(),
        })
    }
    fn model_id(&self) -> &str {
        "counting-remote"
    }
}

struct NoopExtractor;
impl LocalEntityExtractor for NoopExtractor {
    fn extract(
        &self,
        _t: &str,
        _l: &[(&str, &str)],
        _th: f32,
    ) -> anno_rag_tabular::error::Result<Vec<LocalEntity>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn no_remote_call_when_fallback_absent() {
    let chunk_id = uuid::Uuid::now_v7();
    let local: Box<dyn LlmClient> = Box::new(LocalTabularClient::new(Box::new(NoopExtractor)));
    let router = RoutingLlmClient::new(local, None); // allow_remote == false equivalent
    let prompt = format!("[CHUNK::{chunk_id}]hello[/CHUNK]");
    let _ = router
        .generate_structured("sys", &prompt, &json!({}))
        .await
        .expect("local-only call succeeds");
    // No remote attached → nothing to count, but the call must not panic
    // or attempt network IO. Reaching here is the assertion.
}

#[tokio::test]
async fn remote_called_only_when_attached_and_safe() {
    let chunk_id = uuid::Uuid::now_v7();
    let counter = Arc::new(AtomicUsize::new(0));
    let local: Box<dyn LlmClient> = Box::new(LocalTabularClient::new(Box::new(NoopExtractor)));
    let remote: Box<dyn LlmClient> = Box::new(CountingRemote(Arc::clone(&counter)));
    let router = RoutingLlmClient::new(local, Some(remote));
    // A PII-free prompt passes the safety gate → remote IS consulted.
    let prompt = format!("[CHUNK::{chunk_id}]the term is 24 months[/CHUNK]");
    let _ = router
        .generate_structured("sys", &prompt, &json!({}))
        .await
        .expect("call succeeds");
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "remote must be consulted once when attached + safe"
    );
}
