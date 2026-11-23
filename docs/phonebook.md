# Public Phonebook caller names

The `plugin-phonebook` feature names incoming callers from a local copy of [Public Phonebook](https://www.dial.page).
The standard image includes it.
The [decision record](adr/name-callers-from-a-local-public-phonebook-copy.md) explains why the copy stays on the telephone server.

## Sync

Run the sync whenever the directory should be refreshed.
No timer, background task, or configuration section runs it.

```sh
phone-server --config /etc/homephone/homephone.toml sync-phonebook
```

The command reads the [dataset listing](https://www.dial.page/api/datasets) and selects the `public-phonebook-directory/1` archive.
It refuses redirects and archives outside the listing origin.
It verifies the listed byte length and SHA-256 before decoding, and it limits the compressed archive to 64 MiB and the decoded archive to 256 MiB.
It checks the format line, the header snapshot, every row and telephone, and the header's contact count.
An unchanged snapshot and digest skip the download.

The command writes `phonebook.sqlite` in the data directory beside the call database.
It builds the file as `phonebook.sqlite.partial`, synchronizes it, and renames it over the previous index.
A failed sync keeps the previous index and removes the partial file.
The call database is never modified.
A running server uses the new index for the next call without a restart.
The November 2026 snapshot of 667,696 contacts produced 561,495 numbers in a 41 MB index.
Backups omit the rebuildable index, so run the sync again after a restore or rollback.

Check one number after a sync.

```sh
phone-server --config /etc/homephone/homephone.toml lookup-phonebook 0112112111
```

The command prints the stored label or `未登録`.

## Labels

Matching uses the complete national number, so `0…` and `+81…` spellings of the same number share one label.
One distinct name shows its sorted categories, such as `札幌市役所（市区町村の役所）`.
Two or more distinct names on one number show `公共施設（名前がNつあります）`.
Labels longer than 512 bytes or containing control characters are skipped and counted in the sync report.

The server looks up an inbound caller once at admission and waits at most 200 ms.
A missing, slow, or damaged index admits the call without a label.
The label stays with the call, so a later sync leaves earlier history unchanged.
Deleting a call clears its label with the number.

The ntfy message starts with `公共電話帳:` and the label, and the web history shows the same text above the number.
The prefix marks a match in a public directory, not a verified caller identity.

## Privacy and data terms

Sync sends no caller number and no installation data, and lookups stay on the telephone server.
The index keeps only the matched names and categories, and each Public Phonebook record keeps the reuse terms of its source.
The project distributes no directory data.
