# deslicer

Vendor-neutral CI client for planning, approving, and shipping Splunk configuration changes via the [Deslicer Automation Platform](https://deslicer.ai) (DAP).

The CLI uses a **resolve-then-direct** architecture: it calls deslicer-ai at `POST /api/cli/resolve-backend` to discover the correct Observer backend for your tenant, then talks to that backend directly for plan lifecycle operations. OIDC audience is always `https://api.deslicer.ai`.

Repository: [github.com/deslicer/cli](https://github.com/deslicer/cli) · crates.io: [`deslicer-cli`](https://crates.io/crates/deslicer-cli)

## Install

**Homebrew**

```bash
brew install deslicer/tap/deslicer
```

**cargo**

```bash
cargo install deslicer-cli
```

**curl**

```bash
curl -fsSL https://raw.githubusercontent.com/deslicer/cli/main/scripts/install.sh | bash
```

**CI runners** — install the binary in your pipeline (GitHub Actions, GitLab CI, Azure DevOps, Bitbucket Pipelines). See [docs/installation.md](docs/installation.md) for per-platform OIDC setup.

**Updating** — `deslicer update` self-updates from GitHub Releases (Linux/macOS); Homebrew users run `brew upgrade deslicer`. See [docs/installation.md](docs/installation.md#updating).

## Quick start

New to the CLI? Follow the [Quickstart](docs/quickstart.md). The fastest way to a first plan — no GitHub App or OIDC setup required — is the bundle flow:

```bash
export OBSERVER_API_URL="https://observer.example.com:8088"
export DESLICER_API_TOKEN="<api-key-with-tools-scope>"

deslicer change plan \
  --source-dir ./my-splunk-config \
  --target-group <host-group-uuid> \
  --name "my-first-plan"
```

See [docs/bundle-flow.md](docs/bundle-flow.md) for the full walkthrough, limits, and security model. For how the CLI fits into DAP and deslicer-ai, see [docs/architecture.md](docs/architecture.md). Testing from a source checkout against a local Observer? Follow [docs/local-testing.md](docs/local-testing.md).

## Commands

| Group | Command | Description |
|-------|---------|-------------|
| **auth** | `deslicer auth login` | Exchange CI OIDC for a session; resolve Observer backend |
| **auth** | `deslicer auth status` | Print OIDC/platform binding diagnostics |
| **groups** | `deslicer groups list` | List host groups (`id` is the value for `--target-group`) |
| | `deslicer completion bash\|zsh\|fish` | Print shell completions to stdout |
| **change** | `deslicer change plan` | Create or refresh a change plan (add `--source-dir` for the GitHub-App-free bundle flow) |
| **change** | `deslicer change show` | Show plan details |
| **change** | `deslicer change approve` | Approve a pending plan |
| **change** | `deslicer change reject` | Reject a pending plan |
| **change** | `deslicer change deploy` | Execute an approved plan |
| **change** | `deslicer change verify` | Verify deployment outcome |
| **change** | `deslicer change status` | Poll plan/execution status |
| | `deslicer update` | Self-update the binary from GitHub Releases (`--check` to preview) |

### Global flags and environment

| Flag / env | Default | Purpose |
|------------|---------|---------|
| `--deslicer-api-url` / `DESLICER_API_URL` | `https://api.deslicer.ai` | deslicer-ai portal (resolve-backend) |
| `--observer-api-url` / `OBSERVER_API_URL` | _(unset)_ | Air-gapped escape hatch — skip resolve |
| `DESLICER_API_TOKEN` (env only) | _(unset)_ | Observer API key (`tools` scope) for direct Observer access (bundle flow or git-sourced CI). Create under **Platform → API keys**. Not DAI's stored admin/read key. |
| `--ci-platform` | `auto` | Force platform: `github`, `gitlab`, `azure`, `bitbucket`, `local` |
| `--log-format` | `human` | `human` or `json` |

## GitHub Actions

Grant OIDC to the job, then invoke the composite action (ships from a separate repo — Plan 1d):

```yaml
permissions:
  id-token: write
  contents: read

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: deslicer/change-action@v1
        with:
          environment: production
          command: deploy
          plan-id: ${{ inputs.plan_id }}
```

For raw CLI usage inside a workflow, install `deslicer` and either:

- grant `id-token: write` and run `deslicer auth login` (OIDC via DAI), or
- set `OBSERVER_API_URL` + `DESLICER_API_TOKEN` and run `deslicer change plan --target-group <uuid>` (direct Observer; no OIDC).

See [docs/installation.md](docs/installation.md) and [docs/quickstart.md](docs/quickstart.md#path-a2-ci-pipeline-with-an-observer-api-token).

## Documentation

- [Architecture](docs/architecture.md) — how the CLI integrates with DAP and deslicer-ai
- [Quickstart](docs/quickstart.md) — zero to an approved, deployed change (both auth paths)
- [Bundle flow](docs/bundle-flow.md) — GitHub-App-free plans from a local directory
- [CI outputs](docs/ci-outputs.md) — output variables per command and CI platform
- [Local testing](docs/local-testing.md) — build from source and test against a local Observer
- [Installation](docs/installation.md) — Homebrew, cargo, curl, Docker, and CI platform matrix
- [OIDC troubleshooting](docs/oidc-troubleshooting.md) — exit codes and fixes
- [Environments](docs/environments.md) — `.deslicer/environments/` convention
- [Contributing](docs/contributing.md) — local dev and PR guidelines
- [Release process](docs/release-process.md) — tagging, signing, and publish pipeline

## License

Apache-2.0
