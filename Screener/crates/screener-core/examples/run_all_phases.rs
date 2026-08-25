use std::fs;
use std::sync::Arc;

use screener_core::data::ibkr::IbkrClient;
use screener_core::data::yahoo::{YahooClient, YahooClientConfig};
use screener_core::{run_phase1, run_phase2, run_phase3, Phase1Config, Phase2Config, Phase3Config};

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt::try_init();

    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "crates/screener-core/examples/universe.sample.txt".to_string()
    });
    let ibkr_address = std::env::args().nth(2).unwrap_or_else(|| "127.0.0.1:7497".to_string());

    let universe: Vec<String> = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read universe file {path}: {e}"))
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    println!("Loaded {} symbols from {}", universe.len(), path);

    let yahoo = Arc::new(YahooClient::new(YahooClientConfig::default()));
    let phase1_results = run_phase1(&universe, &Phase1Config::default(), yahoo, None).await;
    println!(
        "Phase 1: {} survivors, {} technical failures, {} errors",
        phase1_results.survivors.len(),
        phase1_results.technical_failures.len(),
        phase1_results.errors.len()
    );
    if phase1_results.survivors.is_empty() {
        println!("No Phase 1 survivors — stopping.");
        return;
    }

    println!("Connecting to IBKR at {ibkr_address}...");
    let ibkr: Arc<IbkrClient> = match IbkrClient::connect(&ibkr_address, 100).await {
        Ok(client) => Arc::new(client),
        Err(err) => {
            eprintln!("Failed to connect to IBKR: {err}");
            return;
        }
    };

    let phase2_results = run_phase2(&phase1_results.survivors, &Phase2Config::default(), Arc::clone(&ibkr) as _, None).await;
    println!(
        "Phase 2: {} survivors, {} technical failures, {} errors",
        phase2_results.survivors.len(),
        phase2_results.technical_failures.len(),
        phase2_results.errors.len()
    );
    if phase2_results.survivors.is_empty() {
        println!("No Phase 2 survivors — stopping.");
        return;
    }

    let phase3_results = run_phase3(&phase2_results.survivors, &Phase3Config::default(), ibkr, None).await;
    println!(
        "Phase 3: {} survivors, {} technical failures, {} errors",
        phase3_results.survivors.len(),
        phase3_results.technical_failures.len(),
        phase3_results.errors.len()
    );

    println!("\nFinal watchlist:");
    for symbol in &phase3_results.survivors {
        println!("  {symbol}");
    }
}
