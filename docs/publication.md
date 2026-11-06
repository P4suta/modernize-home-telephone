# Prepare public source publication

## Select the source boundary

The original local Git history contains manufacturer originals with separate copyright restrictions.
Removing a file from the current tree leaves its historical bytes reachable.
Publish a fresh repository from the reviewed source archive instead of pushing the original branch, tags, or all refs.
Keep the original checkout and its private archive locally.

The [source distribution decision](adr/distribute-authored-sources-without-vendor-originals.md) records the boundary and alternatives.
`.gitattributes` excludes the manufacturer archive, local installations, cache files, environment files, databases, and recordings from `git archive`.
The source inventory also excludes manufacturer originals from package identities and package source copies.
The Docker context excludes the same archive.

## Verify the candidate

Commit the intended public source changes before creating the archive.
Use an absent destination under the ignored local artifact directory.
The command exports the committed tree without Git history or private refs.

```sh
mise x -- just check
mise x -- just security
git archive --format=tar --output .local/public-source.tar HEAD
```

Unpack the archive into an empty directory and verify the fresh checkout with the pinned tools.
Run `just check` and `just security` there without copying a vendor archive, installation, cache, or credentials from the original checkout.
Initialize fresh Git history only in the extracted directory.
Inspect the exact candidate file inventory and scan its new history for secrets before the first push.

The normal PR workflow uses `pull_request` and `merge_group`, read-only repository access, and actions pinned by commit SHA.
The stable `gate` job requires the fast verification job to succeed.
The manual Software assurance workflow runs `just release-check` on the selected commit and publishes nothing.
The full gate requires current-source results for browser and SIPp calls, deployment recovery, Kani, Miri, fuzzing, and both mutation campaigns.

## Configure the hosted repository

Repository creation, the first push, and hosted settings need their own observed results.
Local files establish no GitHub protection or successful hosted workflow.
After publication, enable private vulnerability reporting, secret scanning, push protection, and security alerts.
Require the observed `gate` check on the protected default branch, signed commits, linear history, and pull request review.
Use squash-only merges and delete merged branches.
Preserve existing stricter protections.
The [GitHub security guidance](https://docs.github.com/en/actions/reference/security/secure-use) explains immutable action references and untrusted workflow inputs.

`renovate.json` extends the shared dependency policy but disables automatic merging for the initial repository configuration.
Install or enable the Renovate GitHub App before expecting dependency pull requests.
Review toolchain and dependency updates through the same PR gate.

Hardware acceptance, Raspberry Pi boot, real power loss, paid AI providers, and external notifications remain separate installation evidence.
No source publication or hosted software run substitutes for those observations.
