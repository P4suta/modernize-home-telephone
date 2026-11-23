use crate::INDEX_FILE;
use anyhow::{Context, ensure};
use phone_core::directory::{CallerLabelText, NationalNumber};
use rusqlite::{Connection, OpenFlags, OptionalExtension as _, params};
use std::{path::Path, time::Duration};

const SCHEMA_VERSION: u32 = 1;

fn open(path: &Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(Duration::from_secs(1))?;
    Ok(connection)
}

pub fn lookup(data_dir: &Path, number: NationalNumber) -> anyhow::Result<Option<String>> {
    let path = data_dir.join(INDEX_FILE);
    if !path.try_exists()? {
        return Ok(None);
    }
    let connection = open(&path)?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        version == SCHEMA_VERSION,
        "unsupported Public Phonebook index"
    );
    let label: Option<String> = connection
        .query_row(
            "SELECT label FROM labels WHERE number = ?1",
            [i64::try_from(number.key())?],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(label) = &label {
        ensure!(
            CallerLabelText::new(label).is_ok(),
            "invalid Public Phonebook label"
        );
    }
    Ok(label)
}

pub(crate) fn installed(data_dir: &Path) -> Option<(String, String)> {
    let connection = open(&data_dir.join(INDEX_FILE)).ok()?;
    connection
        .query_row(
            "SELECT snapshot, archive_sha256 FROM meta WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok()
}

pub fn install(
    data_dir: &Path,
    snapshot: &str,
    sha256: &str,
    contacts: u64,
    labels: &[(NationalNumber, String)],
) -> anyhow::Result<()> {
    ensure!(
        crate::digest(snapshot) && crate::digest(sha256),
        "invalid Public Phonebook snapshot identity"
    );
    ensure!(
        labels
            .iter()
            .all(|(_, label)| CallerLabelText::new(label).is_ok()),
        "invalid Public Phonebook label"
    );
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder.create(data_dir)?;
    let partial = data_dir.join(format!("{INDEX_FILE}.partial"));
    let journal = data_dir.join(format!("{INDEX_FILE}.partial-journal"));
    for stale in [&partial, &journal] {
        match std::fs::remove_file(stale) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
    }
    let result = build(&partial, snapshot, sha256, contacts, labels).and_then(|()| {
        std::fs::rename(&partial, data_dir.join(INDEX_FILE))?;
        #[cfg(unix)]
        std::fs::File::open(data_dir)?.sync_all()?;
        Ok(())
    });
    if result.is_err() {
        for stale in [&partial, &journal] {
            let _ = std::fs::remove_file(stale);
        }
    }
    result
}

fn build(
    partial: &Path,
    snapshot: &str,
    sha256: &str,
    contacts: u64,
    labels: &[(NationalNumber, String)],
) -> anyhow::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(partial)?;
    let mut connection = Connection::open(partial)?;
    connection.pragma_update(None, "journal_mode", "DELETE")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE labels (number INTEGER PRIMARY KEY, label TEXT NOT NULL) WITHOUT ROWID;
         CREATE TABLE meta (
             id INTEGER PRIMARY KEY CHECK (id = 1),
             snapshot TEXT NOT NULL,
             archive_sha256 TEXT NOT NULL,
             contacts INTEGER NOT NULL
         );",
    )?;
    {
        let mut insert =
            transaction.prepare("INSERT INTO labels (number, label) VALUES (?1, ?2)")?;
        for (number, label) in labels {
            insert.execute(params![i64::try_from(number.key())?, label])?;
        }
    }
    transaction.execute(
        "INSERT INTO meta (id, snapshot, archive_sha256, contacts) VALUES (1, ?1, ?2, ?3)",
        params![snapshot, sha256, i64::try_from(contacts)?],
    )?;
    transaction.commit()?;
    connection
        .close()
        .map_err(|(_, error)| error)
        .context("close the Public Phonebook index")?;
    std::fs::File::open(partial)?.sync_all()?;
    Ok(())
}
