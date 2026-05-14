//! Standalone CLI: `cargo run --bin predict_test -- "I went to the"`
//!
//! Assumes llama-server is running at NEXTWORD_BASE_URL (default
//! http://127.0.0.1:8080). Used to verify the M2 prediction pipeline.

use std::time::Instant;
use nextword_core::{Predictor, PredictorConfig};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let context: String = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if context.is_empty() {
        eprintln!("usage: predict_test \"<context>\"");
        std::process::exit(2);
    }
    let base_url = std::env::var("NEXTWORD_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".into());

    let predictor = Predictor::new(PredictorConfig {
        base_url,
        ..Default::default()
    });

    // Warmup
    let _ = predictor.predict(&context, CancellationToken::new()).await;

    let t0 = Instant::now();
    let words = predictor.predict(&context, CancellationToken::new()).await?;
    let elapsed = t0.elapsed();

    println!("context : {context}");
    println!("elapsed : {:?}", elapsed);
    println!("suggest : {:?}", words);
    Ok(())
}
