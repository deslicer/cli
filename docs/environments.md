# Environments

CI pipelines target a named **environment** (for example `production`, `staging`, `eu-prod`). The CLI resolves that name to a Deslicer tenant and Observer backend via deslicer-ai.

## File convention

Environment definitions live in the repository under:

```
.deslicer/environments/
├── production.yml
├── staging.yml
└── 550e8400-e29b-41d4-a716-446655440000.json
```

Supported extensions: **`.yml`**, **`.yaml`**, **`.json`**.

### Filename stem → environment name

The **filename stem** (without extension) is the environment name passed to `--environment`:

| File | `--environment` value |
|------|------------------------|
| `.deslicer/environments/production.yml` | `production` |
| `.deslicer/environments/staging.yaml` | `staging` |
| `.deslicer/environments/eu-prod.json` | `eu-prod` |

Example:

```bash
deslicer change plan --environment production
```

### UUID-named files (direct escape hatch)

When portal bindings are ambiguous (exit code **7**) or you need to pin an exact tenant without a friendly alias, name the file after the tenant UUID:

```
.deslicer/environments/550e8400-e29b-41d4-a716-446655440000.yml
```

Reference it directly:

```bash
deslicer auth login --environment 550e8400-e29b-41d4-a716-446655440000
```

UUID stems bypass fuzzy name matching and map one-to-one to the tenant record encoded in the file or resolved via the portal.

## File contents

Canonical Path A2 / Observer shape (byte-compatible header):

```yaml
# Deslicer environment configuration.
# File stem "acme-prod" maps to a workspace environment (tenant: Acme Prod).
# Add apps under each inventory_group as `- source_path: <relative-app-path>`.
# See README.md at the repository root for how this file is used.

destinations:
  - inventory_group: indexers
    apps:
      - source_path: apps/ta_nix
  - inventory_group: forwarders
    apps:
```

Each `inventory_group` is an Observer host group (`GET /api/v1/groups`). Apps are repo-relative `source_path` entries. The filename stem (`acme-prod` for `acme-prod.yml`) is the `--environment` value and the GitHub Environment name. A repo-level `DESLICER_ENVIRONMENT` variable is only the name pointer so `pull_request` can select that Environment.

### Path A2 (Observer API token)

`deslicer init --provider github-token --environment <tenant-slug>` writes `.deslicer/environments/<tenant-slug>.yml` with this tenant's host groups only. Refresh after inventory changes:

```bash
deslicer inventory sync
# or: deslicer inventory sync --environment acme-prod --dry-run
```

Merge rules: new groups are appended (existing `apps:` lists stay intact); removed groups with empty `apps:` are dropped; removed groups that still list `source_path` apps are kept and the command exits 2 until you delete those apps from the file (and the repo) and re-run.

`--force` on `init` overwrites workflow templates only — it does not wipe operator `apps:` lists.

`init` prints a `gh` recipe to create the GitHub Environment and pipe secrets via stdin. The CLI never creates Environments or writes secrets. A second Observer backend is a second Environment plus a workflow matrix row, not a second repo-level token.

GitHub App repos already receive this YAML from Observer `github_repo_sync`; do not use `inventory sync` as a substitute for App provision.

## Enumeration scope

`deslicer inventory sync` and Path A2 `init` read `.deslicer/environments/*.{yml,yaml}` locally to resolve a single filename stem when `--environment` is omitted. CI OIDC still resolves the binding through deslicer-ai after authentication.

For a manual listing:

```bash
ls .deslicer/environments/
```

## Portal bindings vs repo files

Two layers cooperate:

1. **Portal (deslicer-ai)** — maps `(repository, environment, OIDC subject)` → tenant + Observer URL. Required for CI OIDC.
2. **Repo files (`.deslicer/environments/*`)** — document and pin environment names for Actions/workflows; UUID stems disambiguate.

A missing portal binding causes exit code **6** even if a repo file exists. A missing repo file does not block the CLI when the portal binding is correct.

## Multi-environment repos

Monorepos may define many files:

```
.deslicer/environments/
├── production.yml
├── staging.yml
├── dev.yml
└── dr-failover.yml
```

Select per job:

```yaml
- run: deslicer change deploy --environment staging
- run: deslicer change deploy --environment production
  if: github.ref == 'refs/heads/main'
```

## Air-gapped override

When resolve-backend cannot reach deslicer-ai, operators may set `OBSERVER_API_URL` to talk to Observer directly. Environment files still name the logical target for workflows; the override is a break-glass path documented in [installation.md](installation.md).
