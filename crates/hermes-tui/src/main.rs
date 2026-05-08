//! `hermes-tui` binary.

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    hermes_tui::main_loop().await
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    // Drop logs into a file so they don't corrupt the TUI.
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return,
    };
    let log_dir = home.join(".small-rust-hermes");
    if std::fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let log_path = log_dir.join("tui.log");
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    else {
        return;
    };
    let filter = EnvFilter::try_from_env("HERMES_LOG")
        .unwrap_or_else(|_| EnvFilter::new("warn,hermes_tui=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(file)
        .with_ansi(false)
        .with_target(false)
        .try_init();
}
