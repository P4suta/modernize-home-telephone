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
    CheckRecordingStorage,
    ExportNasCredentials {
        #[arg(long)]
        output: PathBuf,
    },
    SyncPhonebook {
        #[arg(long, default_value = "https://www.dial.page")]
        origin: String,
    },
    LookupPhonebook {
        number: String,
    },
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
    if let Some(Operation::ExportNasCredentials { output }) = &cli.command {
        Secrets::export_nas_credentials(
            &config,
            secrets_file
                .as_deref()
                .context("a configuration file is required")?,
            output,
            |name| std::env::var(name).ok(),
        )?;
        println!("NASの認証情報を秘密ファイルへ保存しました。");
        return Ok(());
    }
    if matches!(cli.command, Some(Operation::CheckConfig)) {
        println!("Configuration is valid.");
        return Ok(());
    }
    if let Some(Operation::SyncPhonebook { origin }) = &cli.command {
        #[cfg(feature = "plugin-phonebook")]
        {
            let sync = phone_plugin_phonebook::Sync::new(
                origin,
                config.mode == phone_server::config::Mode::Lab,
            )?;
            match sync.run(&config.data_dir).await? {
                phone_plugin_phonebook::Report::Unchanged { snapshot } => {
                    println!("公共電話帳は最新です。スナップショット: {snapshot}");
                }
                phone_plugin_phonebook::Report::Installed {
                    snapshot,
                    contacts,
                    numbers,
                    labels,
                    skipped,
                } => {
                    println!(
                        "公共電話帳を取り込みました。スナップショット: {snapshot}、連絡先: {contacts}件、番号: {numbers}件、表示名: {labels}件、除外: {skipped}件"
                    );
                }
            }
            return Ok(());
        }
        #[cfg(not(feature = "plugin-phonebook"))]
        {
            let _ = origin;
            anyhow::bail!("Public Phonebook is absent from this build");
        }
    }
    if let Some(Operation::LookupPhonebook { number }) = &cli.command {
        #[cfg(feature = "plugin-phonebook")]
        {
            let label = phone_plugin_phonebook::label(&config.data_dir, number)?;
            println!("{}", label.as_deref().unwrap_or("未登録"));
            return Ok(());
        }
        #[cfg(not(feature = "plugin-phonebook"))]
        {
            let _ = number;
            anyhow::bail!("Public Phonebook is absent from this build");
        }
    }
    if matches!(cli.command, Some(Operation::CheckRecordingStorage)) {
        #[cfg(feature = "plugin-nas")]
        {
            let secrets = Secrets::load_with_file(&config, secrets_file.as_deref())?;
            phone_server::recordings::probe(&config, secrets.nas_credentials.as_ref()).await?;
            println!("NASへの書き込みと読み取り照合を確認しました。");
            return Ok(());
        }
        #[cfg(not(feature = "plugin-nas"))]
        anyhow::bail!("NAS recording storage is absent from this build");
    }
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "phone_server=info".into()),
        )
        .init();
    let secrets = Secrets::load_for_calls(&config, secrets_file.as_deref())?;
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let store = Store::open(&config.data_dir.join("phone.sqlite")).await?;
    if phone_server::recordings::cleanup_temporary(&config.data_dir)
        .await
        .is_err()
    {
        tracing::warn!(
            operation = "recording_temporary_cleanup",
            code = "startup_cleanup_failed"
        );
    }
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
    )?
    .with_archive_credentials(secrets.nas_credentials.clone());
    let mut notification_config = config.clone();
    if notification_config.notifications.ntfy.is_some() && secrets.ntfy_token.is_none() {
        notification_config.notifications.ntfy = None;
    }
    let notifier =
        phone_server::notifications::Notifier::new(notification_config, store, secrets.clone())?;
    let processing = tokio::spawn(processor.run(stop.clone()));
    let notifying = tokio::spawn(notifier.run(stop.clone()));
    let app = router(config.clone(), control.clone(), ari, secrets);
    tracing::info!(operation="startup", mode=?config.mode, "HTTP service started");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        phone_server::shutdown::wait().await;
        control.shutdown().await;
    })
    .await?;
    stop.cancel();
    let (processing, notifying) = tokio::join!(processing, notifying);
    processing.context("recording worker shutdown failed")?;
    notifying.context("notification worker shutdown failed")?;
    Ok(())
}
