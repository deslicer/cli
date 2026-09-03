# Installation

Install `deslicer` on developer machines or CI runners. The binary name is **`deslicer`**; the Rust package on crates.io is **`deslicer-cli`**.

## Homebrew

```bash
brew tap deslicer/tap
brew install deslicer
```

Formula source: [github.com/deslicer/homebrew-tap](https://github.com/deslicer/homebrew-tap).

## cargo install

Requires Rust 1.88+ (see `rust-toolchain.toml` in the repo):

```bash
cargo install deslicer-cli
```

Verify:

```bash
deslicer --version
```

## curl install

```bash
curl -fsSL https://raw.githubusercontent.com/deslicer/cli/main/scripts/install.sh | bash
```

The script detects your OS/arch, downloads the matching release archive from [GitHub Releases](https://github.com/deslicer/cli/releases), verifies the SHA-256 checksum, and installs `deslicer` to `/usr/local/bin`. Overrides:

| Variable | Effect |
|----------|--------|
| `DESLICER_INSTALL_DIR` | Install destination (default `/usr/local/bin`) |
| `DESLICER_VERSION` | Pin a specific tag, e.g. `v1.3.2` (default: latest stable) |

Re-running the script updates an existing installation in place. It will be mirrored at `https://get.deslicer.ai/cli/install.sh` once that host is live.

## Updating

Pick the channel you installed with:

```bash
deslicer update            # self-update from GitHub Releases (Linux/macOS)
deslicer update --check    # report whether a newer release exists
brew upgrade deslicer      # Homebrew installs
cargo install deslicer-cli # crates.io installs (add --force to reinstall)
```

`deslicer update` downloads the release archive for your platform, verifies the SHA-256 sidecar, and atomically replaces the running binary. It never installs prereleases unless you pass `--version vX.Y.Z-rc.N` explicitly. On Windows, download the new `.zip` from the releases page instead — in-place replacement of a running `.exe` is blocked by the OS.

If the binary lives in a root-owned directory (e.g. `/usr/local/bin`), re-run the install script with `sudo` instead of `deslicer update`.

## Docker

```bash
docker run --rm -it \
  -e DESLICER_API_URL=https://api.deslicer.ai \
  -e DESLICER_DEV_TOKEN="${DESLICER_DEV_TOKEN}" \
  ghcr.io/deslicer/cli:latest deslicer auth status
```

For CI, mount OIDC-related env vars from the runner instead of `DESLICER_DEV_TOKEN`.

---

## CI platform matrix

All platforms use OIDC audience **`https://api.deslicer.ai`**. After the runner exposes a token, run:

```bash
deslicer auth login --environment <name>
deslicer change <subcommand> ...
```

### GitHub Actions

```yaml
permissions:
  id-token: write
  contents: read

jobs:
  plan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install deslicer
        run: |
          curl -fsSL https://github.com/deslicer/cli/releases/download/v1.3.2/deslicer-x86_64-unknown-linux-musl.tar.gz \
            | tar -xz
          sudo install deslicer /usr/local/bin/deslicer

      - name: Authenticate
        run: deslicer auth login --environment production
        env:
          # Provided automatically when permissions.id-token: write is set:
          # ACTIONS_ID_TOKEN_REQUEST_URL
          # ACTIONS_ID_TOKEN_REQUEST_TOKEN
          DESLICER_API_URL: https://api.deslicer.ai

      - name: Plan change
        run: deslicer change plan --environment production
```

| Variable | Source |
|----------|--------|
| `ACTIONS_ID_TOKEN_REQUEST_URL` | GitHub Actions (automatic) |
| `ACTIONS_ID_TOKEN_REQUEST_TOKEN` | GitHub Actions (automatic) |
| `DESLICER_API_URL` | Optional override (default `https://api.deslicer.ai`) |

Set `--ci-platform github` only if auto-detection fails.

### GitLab CI

```yaml
plan:
  image: alpine:latest
  id_tokens:
    DESLICER_OIDC_TOKEN:
      aud: https://api.deslicer.ai
  script:
    - apk add --no-cache curl
    - curl -fsSL ... | tar -xz && install deslicer /usr/local/bin/deslicer
    - deslicer auth login --environment production
    - deslicer change plan --environment production
```

| Variable | Source |
|----------|--------|
| `DESLICER_OIDC_TOKEN` | GitLab `id_tokens:` block (audience `https://api.deslicer.ai`) |
| `CI_JOB_JWT` | Legacy fallback on older GitLab versions |

### Azure DevOps

```yaml
steps:
  - task: Bash@3
    inputs:
      targetType: inline
      script: |
        deslicer auth login --environment production
        deslicer change deploy --environment production
    env:
      SYSTEM_OIDCREQUESTURI: $(System.OidcRequestUri)
      SYSTEM_ACCESSTOKEN: $(System.AccessToken)
      DESLICER_API_URL: https://api.deslicer.ai
```

Enable **Allow scripts to access the OAuth token** on the job. Azure exposes OIDC via `SYSTEM_OIDCREQUESTURI` and `SYSTEM_ACCESSTOKEN`.

| Variable | Source |
|----------|--------|
| `SYSTEM_OIDCREQUESTURI` | Azure Pipelines OIDC endpoint |
| `SYSTEM_ACCESSTOKEN` | Job OAuth token |
| `DESLICER_API_URL` | Optional override |

Set `--ci-platform azure` if auto-detection fails.

### Bitbucket Pipelines

```yaml
pipelines:
  default:
    - step:
        oidc: true
        script:
          - curl -fsSL ... | tar -xz && install deslicer /usr/local/bin/deslicer
          - deslicer auth login --environment production
          - deslicer change plan --environment production
```

| Variable | Source |
|----------|--------|
| `BITBUCKET_STEP_OIDC_TOKEN` | Bitbucket (requires `oidc: true` on the step) |
| `DESLICER_API_URL` | Optional override |

Set `--ci-platform bitbucket` if auto-detection fails.

### Local development

For laptop testing without CI OIDC:

```bash
export DESLICER_DEV_TOKEN="<portal-issued dev token>"
deslicer auth login --environment local
deslicer change status --environment local
```

| Variable | Purpose |
|----------|---------|
| `DESLICER_DEV_TOKEN` | Non-production bearer for local/dev auth |
| `DESLICER_API_URL` | Portal URL (default `https://api.deslicer.ai`) |
| `OBSERVER_API_URL` | Skip resolve-backend; talk to Observer directly |

Use `--ci-platform local` to force local mode.

---

## Supported platforms

| Platform | Auto-detect | Manual override |
|----------|-------------|-----------------|
| GitHub Actions | Yes | `--ci-platform github` |
| GitLab CI | Yes | `--ci-platform gitlab` |
| Azure DevOps | Yes | `--ci-platform azure` |
| Bitbucket Pipelines | Yes | `--ci-platform bitbucket` |
| Local / other | Fallback | `--ci-platform local` |

Unsupported CI without a matching override exits with code **8**.
