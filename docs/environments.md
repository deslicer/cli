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

Environment files carry metadata used by the Action layer and portal (Plan 1d). Typical fields:

```yaml
# .deslicer/environments/production.yml
tenant_code: acme-prod
description: Production Splunk fleet
target_group: plant-a
```

Exact schema validation is enforced by the composite Action and portal — the CLI consumes the resolved binding after OIDC auth, not by walking these files itself.

## Enumeration scope

**Important:** walking `.deslicer/environments/` to list available environments is performed by the **GitHub Action / composite Action layer** (Plan 1d — `deslicer/change-action`), not by the `deslicer` CLI binary.

The CLI accepts `--environment <name>` and validates the binding through deslicer-ai after authentication. It does not glob the directory locally.

For local discovery, list files manually:

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
