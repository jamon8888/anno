fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    eprintln!("anno-rag v0.1 — wiring in Task 8");
    Ok(())
}
