use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
    Check,
    Sources,
    Api {
        #[arg(long)]
        update: bool,
    },
    Security,
    Browser {
        #[arg(long, default_value = ".local/telephone")]
        directory: PathBuf,
    },
    Verify {
        #[arg(value_enum)]
        check: xtask::assurance::Check,
        #[arg(long)]
        inside: bool,
    },
    Evidence,
    Lab {
        #[arg(long, default_value = ".local/telephone")]
        directory: PathBuf,
    },
    Rehearse {
        #[arg(long, default_value = ".local/telephone")]
        directory: PathBuf,
    },
    Boot {
        #[arg(long)]
        directory: PathBuf,
    },
    Stop {
        #[arg(long)]
        directory: PathBuf,
    },
    Doctor {
        #[arg(long)]
        directory: PathBuf,
    },
    Backup {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Restore {
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        directory: PathBuf,
    },
    Update {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        package: PathBuf,
        #[arg(long)]
        backup: PathBuf,
    },
    Rollback {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        snapshot: PathBuf,
        #[arg(long)]
        backup: PathBuf,
    },
    Install {
        #[arg(long)]
        package: PathBuf,
        #[arg(long)]
        directory: PathBuf,
    },
    Package {
        #[arg(long)]
        output: PathBuf,
    },
    Unit {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        user: String,
        #[arg(long)]
        output: PathBuf,
    },
    Provision {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        firmware: String,
        #[arg(long)]
        mac: String,
        #[arg(long)]
        server: std::net::Ipv4Addr,
        #[arg(long)]
        output: PathBuf,
    },
    Init {
        #[arg(long, default_value = ".local/lab")]
        directory: PathBuf,
        #[arg(long, default_value = "127.0.0.1")]
        address: std::net::IpAddr,
        #[arg(long, default_value = "http://localhost:8787")]
        origin: String,
        #[arg(long)]
        owner: Option<String>,
        #[arg(long)]
        production: bool,
    },
    GatewayStart {
        #[arg(long, default_value = ".local/lab")]
        directory: PathBuf,
        #[arg(long, default_value = "homephone-asterisk:local")]
        image: String,
        #[arg(long, default_value = "homephone-lab-asterisk")]
        name: String,
    },
    Assets {
        #[arg(long)]
        output: PathBuf,
    },
    Corpus {
        #[arg(long, default_value = "fuzz/corpus")]
        output: PathBuf,
    },
}
fn main() -> anyhow::Result<()> {
    match Cli::parse().operation {
        Operation::Check => xtask::check::fast(),
        Operation::Sources => xtask::check::sources_at(
            std::path::Path::new(".agents/skills/grandstream-ht813/references"),
            true,
        ),
        Operation::Api { update } => xtask::check::schema(update),
        Operation::Security => xtask::check::security(),
        Operation::Browser { directory } => xtask::check::browser(&directory),
        Operation::Verify { check, inside } => {
            if inside {
                xtask::assurance::inside(check)
            } else {
                xtask::assurance::verify(check)
            }
        }
        Operation::Evidence => xtask::assurance::receipts(),
        Operation::Lab { directory } => xtask::manager::lab(&directory),
        Operation::Rehearse { directory } => xtask::manager::rehearse(&directory),
        Operation::Boot { directory } => xtask::manager::boot(&directory),
        Operation::Stop { directory } => xtask::manager::stop(&directory),
        Operation::Doctor { directory } => xtask::manager::doctor(&directory),
        Operation::Backup { directory, output } => xtask::manager::backup(&directory, &output),
        Operation::Restore { backup, directory } => xtask::manager::restore(&backup, &directory),
        Operation::Update {
            directory,
            package,
            backup,
        } => xtask::manager::update(&directory, &package, &backup),
        Operation::Rollback {
            directory,
            snapshot,
            backup,
        } => xtask::manager::rollback(&directory, &snapshot, &backup),
        Operation::Install { package, directory } => xtask::manager::install(&package, &directory),
        Operation::Package { output } => xtask::package::build(&output),
        Operation::Unit {
            directory,
            binary,
            user,
            output,
        } => xtask::operations::private_file(
            &output,
            xtask::manager::systemd_unit(&binary, &xtask::manager::absolute(&directory)?, &user)?
                .as_bytes(),
        ),
        Operation::Provision {
            directory,
            firmware,
            mac,
            server,
            output,
        } => {
            let contents = std::fs::read_to_string(directory.join("config/secrets.env"))?;
            let password = contents
                .lines()
                .find_map(|line| line.strip_prefix("HOMEPHONE_GATEWAY_PASSWORD="))
                .ok_or_else(|| anyhow::anyhow!("missing gateway secret"))?;
            xtask::operations::private_file(
                &output,
                xtask::provisioning::render(&xtask::provisioning::Input {
                    firmware: &firmware,
                    mac: &mac,
                    server,
                    password,
                })?
                .as_bytes(),
            )
        }
        Operation::Init {
            directory,
            address,
            origin,
            owner,
            production,
        } => xtask::operations::initialize(&directory, address, origin, owner, !production),
        Operation::GatewayStart {
            directory,
            image,
            name,
        } => xtask::operations::start_asterisk(&directory, &image, &name),
        Operation::Assets { output } => xtask::write_assets(&output),
        Operation::Corpus { output } => xtask::assurance::write_corpora(&output),
    }
}
