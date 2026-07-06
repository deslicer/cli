# CI outputs

Every `deslicer change` command prints a JSON object to stdout **and** writes key/value output variables to the native output mechanism of the detected CI platform, so downstream pipeline steps can consume results without parsing logs.

## Output sinks per platform

| Platform | Sink | Consumed as |
|----------|------|-------------|
| GitHub Actions | Appends to `$GITHUB_OUTPUT` | `${{ steps.<id>.outputs.plan_id }}` |
| GitLab CI | Appends to the file at `DESLICER_DOTENV_PATH` | `artifacts:reports:dotenv` |
| Azure DevOps | `##vso[task.setvariable ...]` logging commands | `$(plan_id)` in later steps |
| Bitbucket Pipelines | Appends to the file at `DESLICER_DOTENV_PATH` | `source` the file in later steps |
| Local / unknown | Single-line JSON on stdout | `jq` |

If the platform's file path variable is missing (e.g. `GITHUB_OUTPUT` unset), the CLI falls back to single-line JSON on stdout.

## Output keys per command

### `change plan`, `change show`, `change approve`, `change reject`

| Key | Description |
|-----|-------------|
| `plan_id` | External plan id (UUID v4) — use this in all follow-up commands |
| `plan_row_id` | Internal row id (UUID v7) |
| `plan_status` | e.g. `draft`, `pending_approval`, `approved`, `rejected`, `failed` |
| `plan_summary` | Human-readable summary (may be empty) |

### `change deploy` (queued, with `--no-wait`)

| Key | Description |
|-----|-------------|
| `execution_id` | Execution UUID |
| `execution_status` | e.g. `queued`, `executing` |
| `jobs_total` | Number of per-host jobs |
| `plan_id` | External plan id |

### `change deploy` (monitored to completion)

| Key | Description |
|-----|-------------|
| `execution_id` | Execution UUID |
| `execution_status` | Terminal status: `succeeded`, `partial`, `failed`, `canceled`, `timed_out` |
| `jobs_total` | Number of per-host jobs |
| `jobs_succeeded` | Jobs that completed successfully |
| `jobs_failed` | Jobs that failed |

### `change status`, `change verify`

| Key | Description |
|-----|-------------|
| `plan_id` | External plan id |
| `progress_status` | Aggregate progress state |
| `total_items` | Change items in the plan |
| `fully_completed_items` | Items applied on every target host |

## Example: chaining steps in GitHub Actions

```yaml
- name: Plan
  id: plan
  run: deslicer change plan --environment production

- name: Deploy (after environment approval)
  run: deslicer change deploy --plan-id "${{ steps.plan.outputs.plan_id }}" --environment production
```

## Exit codes

`0` on success; non-zero codes map to specific failure classes (OIDC rejected, repo not allowlisted, rate limited, ...). The full table lives in [oidc-troubleshooting.md](oidc-troubleshooting.md#exit-codes).
