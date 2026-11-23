use sha2::{Digest, Sha256};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};
use uuid::Uuid;

const OUTPUT_LIMIT: u64 = 16384;
/// The pinned Samba client reads at most 127 password bytes from `PASSWD_FD`.
pub const MAX_PASSWORD_BYTES: usize = 127;
/// The largest archive the configuration can produce: 7200 seconds of 16-bit mono audio at 8000 Hz plus the header bound.
pub const MAX_ARCHIVED_BYTES: u64 = 7200 * 16000 + 4096;
const TRANSFER_SECONDS: u64 = 60;

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("invalid NAS configuration")]
    Configuration,
    #[error("NAS object is missing")]
    Missing,
    #[error("NAS object already exists")]
    Exists,
    #[error("NAS storage is full")]
    Capacity,
    #[error("NAS operation exceeded its deadline")]
    Timeout,
    #[error("NAS operation failed")]
    Unavailable,
    #[error("NAS audio did not match its verified identity")]
    Integrity,
}

#[derive(Clone)]
pub struct Destination {
    server: String,
    share: String,
    directory: String,
    port: u16,
    username: String,
    password: String,
    maximum: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub bytes: u64,
    digest: [u8; 32],
}

impl Identity {
    pub fn parse(bytes: u64, digest: &str) -> Result<Self, Failure> {
        if bytes == 0 || digest.len() != 64 {
            return Err(Failure::Integrity);
        }
        let mut decoded = [0; 32];
        for (index, pair) in digest.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let value = std::str::from_utf8(pair).map_err(|_| Failure::Integrity)?;
            decoded[index] = u8::from_str_radix(value, 16).map_err(|_| Failure::Integrity)?;
        }
        Ok(Self {
            bytes,
            digest: decoded,
        })
    }
    pub fn digest(&self) -> String {
        self.digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

pub async fn identify(path: &Path, maximum: u64) -> Result<Identity, Failure> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| Failure::Unavailable)?;
    let mut hash = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 65536];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|_| Failure::Unavailable)?;
        if read == 0 {
            break;
        }
        bytes = bytes
            .checked_add(read as u64)
            .filter(|total| *total <= maximum)
            .ok_or(Failure::Integrity)?;
        hash.update(&buffer[..read]);
    }
    if bytes == 0 {
        return Err(Failure::Integrity);
    }
    Ok(Identity {
        bytes,
        digest: hash.finalize().into(),
    })
}

fn identifier(value: &str, maximum: usize, punctuation: &[u8]) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || punctuation.contains(&b))
}

pub fn accepts_username(value: &str) -> bool {
    identifier(value, 128, b"_.-\\ ") && value.bytes().any(|byte| byte != b' ')
}

fn local_name(path: &Path) -> Result<&str, Failure> {
    let name = path.to_str().ok_or(Failure::Configuration)?;
    if !path.is_absolute()
        || name.len() > 4096
        || name
            .bytes()
            .any(|b| matches!(b, b'"' | b'\'' | b';' | b'\n' | b'\r' | b'\0'))
    {
        return Err(Failure::Configuration);
    }
    Ok(name)
}

impl Destination {
    pub fn new(
        server: String,
        share: String,
        directory: String,
        port: u16,
        username: String,
        password: String,
        maximum: u64,
    ) -> Result<Self, Failure> {
        if !identifier(&server, 253, b".-")
            || !identifier(&share, 64, b"_-")
            || directory.len() > 256
            || !directory
                .split('/')
                .all(|part| identifier(part, 128, b"_.-") && part != "." && part != "..")
            || !accepts_username(&username)
            || password.is_empty()
            || password.len() > MAX_PASSWORD_BYTES
            || password.bytes().any(|b| matches!(b, b'\0' | b'\n' | b'\r'))
            || port == 0
            || maximum == 0
        {
            return Err(Failure::Configuration);
        }
        Ok(Self {
            server,
            share,
            directory,
            port,
            username,
            password,
            maximum,
        })
    }

    fn command(&self, action: &str) -> Command {
        self.command_within(action, self.maximum)
    }

    fn retrieval(
        &self,
        id: Uuid,
        destination: &Path,
        expected: &Identity,
    ) -> Result<Command, Failure> {
        if expected.bytes > MAX_ARCHIVED_BYTES {
            return Err(Failure::Integrity);
        }
        Ok(self.command_within(
            &format!("get {id}.wav \"{}\"", local_name(destination)?),
            expected.bytes,
        ))
    }

    fn command_within(&self, action: &str, file_limit: u64) -> Command {
        let mut command = Command::new("/usr/bin/prlimit");
        command.args([
            format!("--fsize={0}:{0}", file_limit),
            "--core=0:0".into(),
            "--".into(),
            "/usr/bin/smbclient".into(),
            format!("//{}/{}", self.server, self.share),
        ]);
        command.args([
            "--port",
            &self.port.to_string(),
            "--user",
            &self.username,
            "--use-kerberos=off",
            "--client-protection=encrypt",
            "--option=client min protocol=SMB3",
            "--name-resolve=host",
            "--timeout=15",
            "--debuglevel=0",
            "--quiet",
            "--stderr",
            "--directory",
            &self.directory,
            "--command",
            action,
        ]);
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C")
            .env("PASSWD_FD", "0");
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }

    async fn execute(&self, action: &str) -> Result<(), Failure> {
        self.run(self.command(action)).await
    }

    async fn run(&self, mut command: Command) -> Result<(), Failure> {
        let mut child = command.spawn().map_err(|_| Failure::Unavailable)?;
        let operation = async {
            let mut stdin = child.stdin.take().ok_or(Failure::Unavailable)?;
            stdin
                .write_all(self.password.as_bytes())
                .await
                .map_err(|_| Failure::Unavailable)?;
            stdin
                .write_all(b"\n")
                .await
                .map_err(|_| Failure::Unavailable)?;
            drop(stdin);
            let stdout = child.stdout.take().ok_or(Failure::Unavailable)?;
            let stderr = child.stderr.take().ok_or(Failure::Unavailable)?;
            let (stdout, stderr, status) =
                tokio::try_join!(limited_output(stdout), limited_output(stderr), async {
                    child.wait().await.map_err(|_| Failure::Unavailable)
                })?;
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
            if text.contains("NT_STATUS_OBJECT_NAME_NOT_FOUND")
                || text.contains("NT_STATUS_OBJECT_PATH_NOT_FOUND")
                || text.contains("NT_STATUS_NO_SUCH_FILE")
            {
                return Err(Failure::Missing);
            }
            if text.contains("NT_STATUS_OBJECT_NAME_COLLISION") {
                return Err(Failure::Exists);
            }
            if text.contains("NT_STATUS_DISK_FULL") {
                return Err(Failure::Capacity);
            }
            if !status.success() || text.contains("NT_STATUS_") {
                return Err(Failure::Unavailable);
            }
            Ok(())
        };
        match tokio::time::timeout(Duration::from_secs(TRANSFER_SECONDS), operation).await {
            Ok(result) => {
                if result.is_err() {
                    let _ = child.kill().await;
                }
                result
            }
            Err(_) => {
                let _ = child.kill().await;
                Err(Failure::Timeout)
            }
        }
    }

    pub async fn ensure_directory(&self) -> Result<(), Failure> {
        let mut parent = self.clone();
        parent.directory = "/".into();
        let mut prefix = String::new();
        for part in self.directory.split('/') {
            match parent.execute(&format!("mkdir {part}")).await {
                Ok(()) | Err(Failure::Exists) => {}
                Err(error) => return Err(error),
            }
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            parent.directory = prefix.clone();
        }
        Ok(())
    }

    pub async fn fetch(
        &self,
        id: Uuid,
        destination: &Path,
        expected: &Identity,
    ) -> Result<(), Failure> {
        self.run(self.retrieval(id, destination, expected)?).await?;
        if identify(destination, expected.bytes).await? != *expected {
            return Err(Failure::Integrity);
        }
        Ok(())
    }

    pub async fn publish(
        &self,
        id: Uuid,
        source: &Path,
        readback: &Path,
        expected: &Identity,
    ) -> Result<(), Failure> {
        match self.fetch(id, readback, expected).await {
            Ok(()) => return Ok(()),
            Err(Failure::Missing) => {}
            Err(error) => return Err(error),
        }
        self.execute(&format!("put \"{}\" {id}.uploading", local_name(source)?))
            .await?;
        self.execute(&format!("rename {id}.uploading {id}.wav"))
            .await?;
        self.fetch(id, readback, expected).await
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), Failure> {
        for extension in ["wav", "uploading"] {
            match self.execute(&format!("del {id}.{extension}")).await {
                Ok(()) | Err(Failure::Missing) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

async fn limited_output(reader: impl AsyncRead + Unpin) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::new();
    reader
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| Failure::Unavailable)?;
    if bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(Failure::Unavailable);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn usernames_preserve_spaces_and_domain_separators_within_the_byte_limit() {
        for value in [
            "recorder".to_owned(),
            "recorder fixture".to_owned(),
            "DOMAIN\\recorder fixture".to_owned(),
            "a".repeat(128),
            "a ".repeat(64),
        ] {
            assert!(accepts_username(&value), "{value:?}");
        }
        for value in [
            "".to_owned(),
            " ".to_owned(),
            " ".repeat(128),
            "a".repeat(129),
            "recorder%password".to_owned(),
            "bad;name".to_owned(),
            "bad\nname".to_owned(),
            "bad\rname".to_owned(),
            "bad\tname".to_owned(),
            "bad\0name".to_owned(),
            "録音".to_owned(),
        ] {
            assert!(!accepts_username(&value), "{value:?}");
        }
    }
    #[test]
    fn command_keeps_passwords_off_arguments_and_bounds_the_child() {
        let destination = Destination::new(
            "nas.invalid".into(),
            "home".into(),
            "telephone/recordings".into(),
            445,
            "recorder fixture".into(),
            "private-test-value".into(),
            1024,
        )
        .unwrap();
        let command = destination.command("get object.wav /tmp/readback");
        let arguments = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect::<Vec<_>>();
        assert!(
            !arguments
                .iter()
                .any(|arg| arg.contains("private-test-value"))
        );
        assert!(arguments.contains(&"--fsize=1024:1024"));
        assert!(arguments.contains(&"--client-protection=encrypt"));
        assert!(
            arguments
                .windows(2)
                .any(|args| args == ["--user", "recorder fixture"])
        );
        assert!(command.get_kill_on_drop());
        assert!(
            command
                .as_std()
                .get_envs()
                .all(|(key, value)| key != "PASSWD"
                    && value != Some(std::ffi::OsStr::new("private-test-value")))
        );
    }
    #[tokio::test]
    async fn identities_refuse_empty_oversized_and_changed_audio() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("audio.wav");
        tokio::fs::write(&path, b"audio").await.unwrap();
        let identity = identify(&path, 5).await.unwrap();
        assert_eq!(identity.bytes, 5);
        assert_eq!(Identity::parse(5, &identity.digest()).unwrap(), identity);
        assert!(identify(&path, 4).await.is_err());
        tokio::fs::write(&path, b"other").await.unwrap();
        assert_ne!(identify(&path, 5).await.unwrap(), identity);
        tokio::fs::write(&path, b"").await.unwrap();
        assert!(identify(&path, 5).await.is_err());
    }
    #[test]
    fn configuration_refuses_command_and_path_injection() {
        for directory in [
            "../calls",
            "calls/../data",
            "calls;del *",
            "calls//data",
            "",
        ] {
            assert!(
                Destination::new(
                    "nas.invalid".into(),
                    "home".into(),
                    directory.into(),
                    445,
                    "recorder".into(),
                    "test".into(),
                    1024
                )
                .is_err()
            );
        }
        for path in [
            "/tmp/test;del *",
            "/tmp/test\nnext",
            "relative",
            "/tmp/test\"other",
        ] {
            assert!(local_name(Path::new(path)).is_err());
        }
        assert!(local_name(Path::new("/tmp/calls with spaces/audio.wav")).is_ok());
        assert!(local_name(Path::new(&format!("/{}", "a".repeat(4095)))).is_ok());
        assert!(local_name(Path::new(&format!("/{}", "a".repeat(4096)))).is_err());
        let destination = |password: String| {
            Destination::new(
                "nas.invalid".into(),
                "home".into(),
                "calls".into(),
                445,
                "recorder".into(),
                password,
                1024,
            )
        };
        assert!(destination("p".repeat(MAX_PASSWORD_BYTES)).is_ok());
        assert!(destination("p".repeat(MAX_PASSWORD_BYTES + 1)).is_err());
    }
    #[test]
    fn retrieval_is_bounded_by_the_archived_size_instead_of_the_capture_limit() {
        let destination = Destination::new(
            "nas.invalid".into(),
            "home".into(),
            "calls".into(),
            445,
            "recorder".into(),
            "test".into(),
            1024,
        )
        .unwrap();
        let archived = Identity::parse(964_096, &"0".repeat(64)).unwrap();
        let command = destination.retrieval(Uuid::nil(), Path::new("/tmp/readback.wav"), &archived);
        let arguments = command
            .unwrap()
            .as_std()
            .get_args()
            .map(|arg| arg.to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert!(arguments.contains(&"--fsize=964096:964096".to_owned()));
        let oversized = Identity::parse(MAX_ARCHIVED_BYTES + 1, &"0".repeat(64)).unwrap();
        assert!(
            destination
                .retrieval(Uuid::nil(), Path::new("/tmp/readback.wav"), &oversized)
                .is_err()
        );
    }

    #[tokio::test]
    async fn child_output_is_complete_at_the_limit_and_rejected_above_it() {
        let exact = vec![7; OUTPUT_LIMIT as usize];
        assert_eq!(limited_output(exact.as_slice()).await.unwrap(), exact);
        assert!(
            limited_output(vec![7; OUTPUT_LIMIT as usize + 1].as_slice())
                .await
                .is_err()
        );
        assert_eq!(limited_output(&b""[..]).await.unwrap(), Vec::<u8>::new());
        assert!(Identity::parse(0, &"1".repeat(64)).is_err());
        assert!(Identity::parse(1, &"1".repeat(63)).is_err());
        assert!(Identity::parse(1, &"g".repeat(64)).is_err());
        assert_eq!(
            Identity::parse(1, &"AF".repeat(32)).unwrap().digest(),
            "af".repeat(32)
        );
    }
}
