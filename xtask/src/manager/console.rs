use super::*;
use std::io::{Read, Write};

impl Installation {
    pub(super) fn verify_console_restart(&self) -> anyhow::Result<()> {
        ensure!(
            self.directory.join("config/ht813-console.toml").is_file(),
            "console lifecycle fixture is missing"
        );
        let name = self.console_name();
        ensure!(
            self.container(&name)?
                .is_some_and(|value| value["State"]["Running"] == true),
            "console must run before shutdown verification"
        );
        output(crate::container::command()?.args(["stop", "--time", "2", &name]))?;
        let stopped = self
            .container(&name)?
            .context("console disappeared during shutdown")?;
        let verdict = if stopped["State"]["ExitCode"] == 0 {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "console did not handle SIGTERM with a successful exit: {}",
                stopped["State"]["ExitCode"]
            ))
        };
        let restarted = output(crate::container::command()?.args(["start", &name]))
            .and_then(|_| self.wait_health());
        crate::update_flow::with_recovery(verdict, restarted)
    }

    pub(super) fn console_name(&self) -> String {
        format!("homephone-{}-console", self.id.simple())
    }

    pub(super) fn start_console(&self, release: &Release) -> anyhow::Result<()> {
        let configuration = self.directory.join("config/ht813-console.toml");
        if !configuration.try_exists()? {
            return Ok(());
        }
        let name = self.console_name();
        if let Some(container) = self.container(&name)? {
            if container["State"]["Running"] != true {
                output(crate::container::command()?.args(["start", &name]))?;
            }
            return Ok(());
        }
        let uid = String::from_utf8(output(Command::new("id").arg("-u"))?)?;
        let gid = String::from_utf8(output(Command::new("id").arg("-g"))?)?;
        let path = configuration
            .to_str()
            .context("console path must be UTF-8")?;
        ensure!(
            !path.contains(','),
            "container bind paths cannot contain commas"
        );
        output(
            crate::container::command()?
                .args([
                    "run",
                    "--detach",
                    "--name",
                    &name,
                    "--label",
                    &format!("homephone.instance={}", self.id),
                    "--restart",
                    "unless-stopped",
                    "--network",
                    "host",
                    "--user",
                    &format!("{}:{}", uid.trim(), gid.trim()),
                    "--read-only",
                    "--cap-drop",
                    "ALL",
                    "--security-opt",
                    "no-new-privileges",
                    "--pids-limit",
                    "128",
                    "--memory",
                    "256m",
                    "--cpus",
                    "1",
                    "--tmpfs",
                    "/tmp:rw,noexec,nosuid,size=16m",
                    "--mount",
                    &format!("type=bind,source={path},target=/homephone-console.toml,readonly"),
                    "--entrypoint",
                    "/usr/local/bin/ht813-console",
                ])
                .args(crate::container::selected()?.run_options())
                .args([
                    &release.app.reference,
                    "serve",
                    "--config",
                    "/homephone-console.toml",
                ]),
        )?;
        Ok(())
    }

    pub(super) fn console_health(&self) -> anyhow::Result<()> {
        let configuration = self.directory.join("config/ht813-console.toml");
        if !configuration.try_exists()? {
            return Ok(());
        }
        #[derive(Deserialize)]
        struct Console {
            listen: SocketAddr,
        }
        let config: Console = toml::from_str(&std::fs::read_to_string(configuration)?)?;
        ensure!(
            config.listen.ip().is_loopback(),
            "console requires loopback HTTP"
        );
        ensure!(
            self.container(&self.console_name())?
                .is_some_and(|container| container["State"]["Running"] == true),
            "console container is unavailable"
        );
        let timeout = std::time::Duration::from_secs(2);
        let mut socket = std::net::TcpStream::connect_timeout(&config.listen, timeout)?;
        socket.set_read_timeout(Some(timeout))?;
        socket.set_write_timeout(Some(timeout))?;
        write!(
            socket,
            "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            config.listen
        )?;
        let mut status = [0; 12];
        socket.read_exact(&mut status)?;
        ensure!(&status == b"HTTP/1.1 200", "console assets are unavailable");
        Ok(())
    }
}
