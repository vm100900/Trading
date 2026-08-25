use std::fs;
use std::sync::Arc;

use screener_core::data::yahoo::{YahooClient, YahooClientConfig};
use screener_core::{run_phase1, Phase1Config, Phase1Progress};

#[tokio::main]
async fn main() {
    tracing_subscriber_init();

    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "crates/screener-core/examples/universe.sample.txt".to_string()
    });

    let universe: Vec<String> = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read universe file {path}: {e}"))
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    println!("Loaded {} symbols from {}", universe.len(), path);

    let client = Arc::new(YahooClient::new(YahooClientConfig::default()));
    let config = Phase1Config::default();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Phase1Progress>();
    let progress_task = tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            println!(
                "Phase 1: {}/{} started, {} passed, {} technical failures, {} errors",
                p.started, p.total, p.passed, p.technical_failures, p.errors
            );
        }
    });

    let results = run_phase1(&universe, &config, client, Some(tx)).await;
    progress_task.await.expect("progress printer task panicked");

    println!("\nSurvivors:");
    for symbol in &results.survivors {
        println!("  {symbol}");
    }

    println!("\nTechnical failures: {}", results.technical_failures.len());
    println!("Errors: {}", results.errors.len());
    for (symbol, err) in &results.errors {
        println!("  {symbol}: {err}");
    }
}

fn tracing_subscriber_init() {
    let _ = tracing_subscriber::fmt::try_init();
}
