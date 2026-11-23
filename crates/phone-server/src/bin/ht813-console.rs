use clap::{Parser, Subcommand};
use phone_server::console::http::{ApiDoc, Config, router};
use std::{path::PathBuf, process::ExitCode};
use utoipa::OpenApi as _;

#[derive(Parser)]
#[command(about = "HT813の状態・設定・項目の説明を管理します。", styles = terminal_styles())]
struct Cli {
    #[arg(
        long,
        global = true,
        conflicts_with = "human",
        help = "クライアント操作の結果をJSONで出力します。"
    )]
    json: bool,
    #[arg(
        long,
        global = true,
        help = "クライアント操作の結果を人向けに表示します。"
    )]
    human: bool,
    #[arg(long, global = true, help = "処理中のアニメーションを省略します。")]
    no_progress: bool,
    #[command(subcommand)]
    command: Operation,
}

fn terminal_styles() -> clap::builder::Styles {
    let bold = clap::builder::styling::Style::new().bold();
    clap::builder::Styles::plain()
        .header(bold)
        .usage(bold)
        .literal(bold)
        .error(bold)
        .invalid(bold)
}

#[derive(Subcommand)]
enum Operation {
    #[command(flatten)]
    Client(phone_server::console::cli::Command),
    #[cfg(feature = "console-fixture")]
    Fixture {
        #[arg(long, default_value = "127.0.0.1:8791")]
        listen: std::net::SocketAddr,
        #[arg(long, default_value = "console/dist")]
        assets: PathBuf,
    },
    Serve {
        #[arg(long)]
        config: PathBuf,
    },
    CheckConfig {
        #[arg(long)]
        config: PathBuf,
    },
    InspectForm {
        #[arg(long)]
        html: PathBuf,
        #[arg(long)]
        document: url::Url,
        #[arg(long)]
        resources: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Schema {
        #[arg(long)]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    let command = cli.command;
    if let Operation::Client(command) = command {
        let login = matches!(&command, phone_server::console::cli::Command::Login { .. });
        let output = phone_server::console::cli::Output::new(cli.json, cli.human, cli.no_progress);
        return match phone_server::console::cli::run(command, &output).await {
            Err(error) if output.is_human() => {
                output.report(&error, login)?;
                Ok(ExitCode::FAILURE)
            }
            result => result,
        };
    }
    anyhow::ensure!(
        !cli.json && !cli.human && !cli.no_progress,
        "presentation options apply to client operations"
    );
    #[cfg(feature = "console-fixture")]
    if let Operation::Fixture { listen, assets } = command {
        phone_server::console::fixture::serve(listen, assets).await?;
        return Ok(ExitCode::SUCCESS);
    }
    if let Operation::InspectForm {
        html,
        document,
        resources,
        output,
    } = command
    {
        inspect_form(&html, &document, &resources, &output)?;
        return Ok(ExitCode::SUCCESS);
    }
    if let Operation::Schema { output } = command {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output, ApiDoc::openapi().to_pretty_json()?)?;
        return Ok(ExitCode::SUCCESS);
    }
    let path = match &command {
        Operation::Serve { config } | Operation::CheckConfig { config } => config,
        Operation::Schema { .. } | Operation::InspectForm { .. } | Operation::Client(_) => {
            unreachable!()
        }
        #[cfg(feature = "console-fixture")]
        Operation::Fixture { .. } => unreachable!(),
    };
    let config: Config = toml::from_str(&std::fs::read_to_string(path)?)?;
    config.validate()?;
    if matches!(command, Operation::CheckConfig { .. }) {
        println!("Console configuration is valid.");
        return Ok(ExitCode::SUCCESS);
    }
    anyhow::ensure!(
        config.assets.join("index.html").is_file(),
        "build the console static assets before serving"
    );
    tracing_subscriber::fmt()
        .with_env_filter("phone_server=info")
        .init();
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    axum::serve(listener, router(config)?)
        .with_graceful_shutdown(phone_server::shutdown::wait())
        .await?;
    Ok(ExitCode::SUCCESS)
}

fn inspect_form(
    html: &std::path::Path,
    document: &url::Url,
    resources: &std::path::Path,
    output: &std::path::Path,
) -> anyhow::Result<()> {
    use anyhow::Context as _;
    use phone_core::console::Mode;
    use phone_server::console::native::{NativeForm, ScriptResources, script_resource_urls};
    use std::{collections::BTreeMap, io::Write as _};
    let html = std::fs::read_to_string(html)?;
    let mut verified = ScriptResources::default();
    for resource in script_resource_urls(&html, document) {
        let file = resource
            .path()
            .rsplit('/')
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing resource name"))?;
        verified.record(resource.path(), &std::fs::read(resources.join(file))?);
    }
    let form =
        NativeForm::parse_verified(&html, document, &verified).context("parse the native form")?;
    let catalogue = phone_server::console::documentation::catalogue();
    let missing: Vec<_> = form
        .fields()
        .iter()
        .filter(|field| {
            phone_server::console::documentation::find(&catalogue, &form, &field.id, &field.label)
                .is_none()
        })
        .map(|field| field.parameter.as_str())
        .collect();
    anyhow::ensure!(
        missing.is_empty(),
        "missing field documentation: {}",
        missing.join(", ")
    );
    let entries = serde_json::json!({
        "parameters": form.fields().iter().map(|field| field.parameter.as_str()).collect::<Vec<_>>(),
        "save": form.settings_values(&BTreeMap::new(), Mode::Save).context("qualify native save")?,
        "apply": form.settings_values(&BTreeMap::new(), Mode::SaveAndApply).context("qualify native apply")?,
    });
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(output)?;
    file.write_all(&serde_json::to_vec(&entries)?)?;
    file.sync_all()?;
    println!(
        "Qualified {} fields; private submission data written to the requested file.",
        form.fields().len()
    );
    Ok(())
}
