use std::process::ExitCode;

use openclaw_gateway::{build_state, config::Config, router};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("openclaw_gateway=info,tower_http=warn")),
        )
        .init();

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let bind = format!("{}:{}", config.bind, config.port);
    let auth_mode = config.auth.mode.as_str().to_string();
    let state_dir = config.state_dir.clone();

    let state = build_state(config)?;
    let app = router(state.clone());

    let listener = TcpListener::bind(&bind).await?;
    let local = listener.local_addr()?;
    tracing::info!(
        address = %local,
        auth = %auth_mode,
        state_dir = %state_dir.display(),
        model = %state.llm.model(),
        model_configured = state.llm.is_configured(),
        "openclaw gateway listening"
    );
    if !state.llm.is_configured() {
        tracing::warn!(
            "no model API key configured; chat replies use the local echo transport \
             (set OPENCLAW_RUST_MODEL_API_KEY to use a real provider)"
        );
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
