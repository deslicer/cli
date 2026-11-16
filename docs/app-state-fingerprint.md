# App-state fingerprint (local object store)

CLI-side contract for immutable Splunk app bytes. It hashes an app directory,
stores each file by SHA-256, and reads an object back by digest.

This is a local stand-in for an object store. It is not a RustFS client. It
does not mint pre-signed DTS links, and it does not ask a worker to pull or
apply anything.

## Commands

```bash
app-state fingerprint --dir ./TA-nix
app-state publish --dir ./TA-nix --store ./object-store
app-state publish --dir ./TA-nix --store ./object-store --yes
app-state fetch --store ./object-store --digest <sha256>
app-state fetch --store ./object-store --digest <sha256> --output ./out.conf --yes
```

`publish` and `fetch` are dry-run unless `--yes`. Objects are never
overwritten: publishing the same bytes again reports `already_present`, and a
changed file is a new digest. The previous object stays.

`link` in the publish JSON is a `file://` path. That is the local equivalent
of a pre-signed URL. A worker in production would receive a platform URL
instead.

## What is still platform work

- Drift reconcile that tells a worker which objects changed
- Pre-signed DTS links from RustFS (or another object store)
- Registry rows that bind app + version to a manifest and per-target config
- Applying the fetched bytes on a Splunk host

Those pieces are not in this repository. Do not point this command at a
production bucket.
