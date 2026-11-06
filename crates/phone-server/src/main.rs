use anyhow::Context;
use clap::{Parser, Subcommand};
use phone_server::{
    app::{ApiDoc, router},
    config::Config,
    controller::Control,
    gateway::Ari,
    secrets::Secrets,
    store::Store,
};
use std::path::PathBuf;
use utoipa::OpenApi as _;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Operation>,
}
#[derive(Subcommand)]
enum Operation {
    Serve,
    Schema {
        #[arg(long)]
        output: PathBuf,
    },
    CheckConfig,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Some(Operation::Schema { output }) = cli.command {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output, ApiDoc::openapi().to_pretty_json()?)?;
        return Ok(());
    }
    let path = cli
        .config
        .or_else(|| std::env::var_os("HOMEPHONE_CONFIG").map(PathBuf::from));
    let secrets_file = path
        .as_ref()
        .and_then(|path| path.parent())
        .map(|parent| parent.join("secrets.env"));
    let config = match path {
        Some(path) => {
            toml::from_str::<Config>(&std::fs::read_to_string(path).context("read configuration")?)?
        }
        None => Config::default(),
    };
    config.validate()?;
    if matches!(cli.command, Some(Operation::CheckConfig)) {
        println!("Configuration is valid.");
        return Ok(());
    }
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "phone_server=info".into()),
        )
        .init();
    let secrets = Secrets::load_with_file(&config, secrets_file.as_deref())?;
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let store = Store::open(&config.data_dir.join("phone.sqlite")).await?;
    let ari = Ari::new(config.ari.clone(), secrets.ari_password.clone())?;
    let control = Control::start_quiesced(
        config.clone(),
        ari.clone(),
        store.clone(),
        secrets.openai_key.clone(),
        std::env::var("HOMEPHONE_START_QUIESCED").as_deref() == Ok("1")
            || std::env::var_os("HOMEPHONE_QUIESCE_FILE")
                .is_some_and(|path| std::path::Path::new(&path).exists()),
    );
    let stop = tokio_util::sync::CancellationToken::new();
    let processor = phone_server::postcall::Processor::new(
        config.clone(),
        ari.clone(),
        store.clone(),
        secrets.openai_key.clone(),
    )?;
    let notifier =
        phone_server::notifications::Notifier::new(config.clone(), store, secrets.clone())?;
    let processing = tokio::spawn(processor.run(stop.clone()));
    let notifying = tokio::spawn(notifier.run(stop.clone()));
    let app = router(config.clone(), control.clone(), ari, secrets);
    tracing::info!(operation="startup", mode=?config.mode, "HTTP service started");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        control.shutdown().await;
    })
    .await?;
    stop.cancel();
    let (processing, notifying) = tokio::join!(processing, notifying);
    processing.context("recording worker shutdown failed")?;
    notifying.context("notification worker shutdown failed")?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! { _ = terminate.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
            }
            Err(_) => {
                if tokio::signal::ctrl_c().await.is_err() {
                    tracing::warn!(operation = "shutdown_signal", code = "ctrl_c_failed");
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        if tokio::signal::ctrl_c().await.is_err() {
            tracing::warn!(operation = "shutdown_signal", code = "ctrl_c_failed");
        }
    }
}
