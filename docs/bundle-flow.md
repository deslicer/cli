# Bundle flow (air-gapped / no network path to the repo)

`deslicer change plan --source-dir <dir>` compiles a change plan from a locally packaged source bundle instead of a git clone. No GitHub App installation, no OIDC exchange, and no repository integration are required. This path bypasses deslicer-ai entirely — see [architecture.md](architecture.md#path-b--bundle-upload-github-app-free-no-dai).

> **A bundle cannot resolve git-lfs.** The archive carries whatever is in the working
> tree, so LFS-tracked files ship as pointer files and the resulting diff is wrong.
> Being GitHub-App-free is no longer a reason to choose this path: the git-sourced
> flow forwards the CI job's own `GITHUB_TOKEN` for a single clone, so a private repo
> with no App installation still compiles from a real checkout. See
> [quickstart.md § Path A2](quickstart.md#path-a2-ci-pipeline-with-an-observer-api-token).

Use it when:

- Observer has no network path back to your repository (air-gapped, or the repo lives on a host Observer cannot reach),
- your source is not in a git repository at all,
- you are packaging a directory assembled by an earlier build step rather than committed content.

## Prerequisites

| Requirement | How to get it |
|-------------|---------------|
| `OBSERVER_API_URL` | Your Observer **management plane** URL (ask your platform admin) |
| `DESLICER_API_TOKEN` | An Observer API key with the `tools` scope — Deslicer portal → **Platform → API keys**. Same secret can run git-sourced GitHub Actions (`change plan --target-group`) when the runner reaches Observer. Do not reuse DAI's stored admin/read keys. |
| Target host group UUID | Deslicer portal → host groups, or `GET /api/v1/groups` |

The bundle flow is **direct mode only**: it talks straight to the Observer API and ignores the deslicer-ai CI proxy. Both environment variables must be set (flags `--observer-api-url` works too; the token is env-only so it never appears in `ps` output).

## Usage

```bash
export OBSERVER_API_URL="https://observer.example.com:8088"
export DESLICER_API_TOKEN="<tools-scope-api-key>"

deslicer change plan \
  --source-dir ./my-splunk-config \
  --target-group 019f36d6-3f61-7eea-9417-7ac4a8a10f69 \
  --name "nightly-config-sync" \
  --no-wait   # optional: return right after compile is triggered
```

Example output:

```
packaged 2 files (330 bytes, sha256 90362bd8...)
bundle uploaded: 019f37fa-6010-707b-bdee-618edc72503a
{"plan_id":"69074147-...","plan_status":"pending_approval", ...}
```

### Source directory layout

The compile-runner looks for Splunk apps under `apps/` at the bundle root:

```
my-splunk-config/
└── apps/
    ├── app_one/
    │   ├── default/
    │   │   ├── app.conf
    │   │   └── props.conf
    │   └── local/
    └── app_two/
        └── default/
```

Only apps present in the bundle are managed — the diff never proposes deletions for apps the bundle does not contain.

## What happens under the hood

1. **Package** — the directory is walked deterministically (sorted entries, symlinks skipped) into a tar.gz whose SHA-256 is computed client-side. Unchanged content always produces the same digest.
2. **Upload** — `POST /api/v1/plan-sources/bundles` with the declared digest. The server re-hashes the bytes and rejects any mismatch (`400 sha256_mismatch`).
3. **Plan create** — `POST /api/v1/plans` with `source_type: bundle` and the bundle id. The plan is recorded with `source_tier: ci_bundle` and `is_trusted_source: false` — bundle plans can never claim repo-verified trust.
4. **Compile** — the ephemeral compile-runner downloads the bundle, re-verifies the SHA-256 against the digest pinned at upload, extracts it under hardened limits, diffs against observed state, and posts the plan draft.
5. **Wait** — the CLI polls until the plan leaves `draft` (typically seconds). `--no-wait` skips this.

## Limits and retention

| Limit | Value |
|-------|-------|
| Compressed bundle size | 32 MiB |
| Decompressed size | 256 MiB |
| Tar entries | 10,000 |
| Bundle retention | 7 days (content scrubbed after expiry; metadata kept for audit) |

A plan must be compiled while its bundle is live. Re-running `change plan --source-dir` uploads a fresh bundle, so expiry only matters for stale, never-compiled plans.

## Security model

- **Digest-pinned**: the SHA-256 is verified at upload *and* again by the compile-runner before extraction — the compiled content is exactly what the client packaged.
- **Never trusted**: bundle plans always carry `is_trusted_source: false` and `source_tier: ci_bundle`. Trusted-source status is reserved for the git-workflow tier (REQ-SIGN-008).
- **Human approval required**: like every plan, bundle plans need a verified human identity to approve — a raw API key cannot self-approve.
- **Hardened extraction**: path traversal rejected, symlinks skipped, decompression-bomb and entry-count caps enforced.

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `--source-dir requires direct Observer access` | `OBSERVER_API_URL` unset | Export it or pass `--observer-api-url` |
| `--source-dir requires the DESLICER_API_TOKEN env var` | Token unset/empty | Export a `tools`-scope API key |
| `403 Forbidden` on upload | API key lacks `tools` scope | Mint a key with the `tools` scope |
| `400 sha256_mismatch` | Bytes corrupted in transit | Retry; check any intercepting proxy |
| `404 bundle_not_found` on plan create | Bundle expired or wrong tenant | Re-run the command (fresh upload) |
| `413` on upload | Bundle over 32 MiB | Trim the source dir — package only `apps/` content |
| Plan stuck in `draft` | Compile-runner failed to launch | Ask your admin to check Observer logs / `COMPILE_RUNNER_IMAGE` |
