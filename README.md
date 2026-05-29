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

**curl** (placeholder — see [docs/installation.md](docs/installation.md) for the install script URL once published)

```bash
curl -fsSL https://get.deslicer.ai/cli/install.sh | bash
```

**CI runners** — install the binary in your pipeline (GitHub Actions, GitLab CI, Azure DevOps, Bitbucket Pipelines). See [docs/installation.md](docs/installation.md) for per-platform OIDC setup.

## Commands

| Group | Command | Description |
|-------|---------|-------------|
| **auth** | `deslicer auth login` | Exchange CI OIDC for a session; resolve Observer backend |
| **auth** | `deslicer auth status` | Print OIDC/platform binding diagnostics |
| **change** | `deslicer change plan` | Create or refresh a change plan |
| **change** | `deslicer change show` | Show plan details |
| **change** | `deslicer change approve` | Approve a pending plan |
| **change** | `deslicer change reject` | Reject a pending plan |
| **change** | `deslicer change deploy` | Execute an approved plan |
| **change** | `deslicer change verify` | Verify deployment outcome |
| **change** | `deslicer change status` | Poll plan/execution status |

### Global flags and environment

| Flag / env | Default | Purpose |
|------------|---------|---------|
| `--deslicer-api-url` / `DESLICER_API_URL` | `https://api.deslicer.ai` | deslicer-ai portal (resolve-backend) |
| `--observer-api-url` / `OBSERVER_API_URL` | _(unset)_ | Air-gapped escape hatch — skip resolve |
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

For raw CLI usage inside a workflow, install `deslicer` and call `deslicer auth login` followed by the desired `deslicer change` subcommand. See [docs/installation.md](docs/installation.md).

## Documentation

- [Installation](docs/installation.md) — Homebrew, cargo, curl, Docker, and CI platform matrix
- [OIDC troubleshooting](docs/oidc-troubleshooting.md) — exit codes and fixes
- [Environments](docs/environments.md) — `.deslicer/environments/` convention
- [Contributing](docs/contributing.md) — local dev and PR guidelines
- [Release process](docs/release-process.md) — tagging, signing, and publish pipeline

## License

Apache-2.0
