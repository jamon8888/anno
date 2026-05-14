//! `anno-privacy-gateway` binary.

use anno_privacy_gateway::{server, GatewayConfig};

#[tokio::main]
async fn main() -> anno_privacy_gateway::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = GatewayConfig::from_env();
    tracing::info!(
        listen = %config.listen,
        upstream = %config.upstream_anthropic_base,
        "anno-privacy-gateway starting"
    );
    server::serve(config).await
}
