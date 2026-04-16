use anyhow::Result;

use crate::cmd::config::ensure_config;
use crate::config::AppConfig;
use crate::pipeline::voice::VoicePipeline;

pub async fn run(debug: bool) -> Result<()> {
    // Load .env files for compatibility
    dotenvy::dotenv().ok();
    dotenvy::from_filename("scripts/remote_rust_chat/.env").ok();
    if let Ok(dir) = std::env::var("SCRIPT_DIR") {
        let env_path = std::path::Path::new(&dir).join(".env");
        if env_path.exists() {
            dotenvy::from_path(&env_path).ok();
        }
    }

    let cfg = AppConfig::load()?;

    // If config is incomplete, prompt user interactively instead of erroring
    let cfg = ensure_config(cfg)?;

    let mut pipeline = VoicePipeline::new(cfg, debug)?;
    pipeline.run_loop().await
}
