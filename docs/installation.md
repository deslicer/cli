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

Once release artifacts are published, the install script will be hosted at:

```bash
curl -fsSL https://get.deslicer.ai/cli/install.sh | bash
```

The script downloads the matching release archive for your OS/arch from [GitHub Releases](https://github.com/deslicer/cli/releases) and installs `deslicer` to `/usr/local/bin` (override with `DESLICER_INSTALL_DIR`).

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
          curl -fsSL https://github.com/deslicer/cli/releases/download/v1.0.0/deslicer-x86_64-unknown-linux-musl.tar.gz \
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
