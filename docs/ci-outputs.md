# CI outputs

Lifecycle-oriented `deslicer change` commands print JSON and write key/value
outputs to the detected CI platform. `change validate` instead writes the
requested report format directly to stdout so it can be redirected to a file
or piped into a PR-comment command.

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

### `change plan`, `change show`, `change approve`, `change reject`, `change verify`

| Key | Description |
|-----|-------------|
| `plan_id` | External plan id (UUID v4) — use this in all follow-up commands |
| `plan_row_id` | Internal row id (UUID v7) |
| `plan_status` | e.g. `draft`, `pending_approval`, `approved_unsigned`, `rejected`, `failed` |
| `plan_summary` | Human-readable summary (change counts when a dry-run diff is available) |
| `diff_total` | Total change items (verify / plan after compile) |
| `diff_additions` | Additions in the dry-run diff |
| `diff_modifications` | Modifications in the dry-run diff |
| `diff_deletions` | Deletions in the dry-run diff |
| `diff_has_destructive` | `true` when the diff includes deletions |
| `plan_ids` | Comma-separated external plan ids when `change plan` fans out across environments |
| `plan_count` | Number of plans created by environment fan-out (`1` when `--environment` is set) |
| `pr_touched_apps` | Comma-separated app names in the plan that overlap PR/changed paths (labeling only) |
| `also_still_drifted_apps` | Comma-separated plan apps that are still drifted but not touched by this PR |
| `pr_preview_summary` | Human line: `this PR touches: …; also still drifted: …` |

`pr_*` keys are emitted only when changed paths are available (`--changed-paths`,
`--changed-paths-file`, `DESLICER_CHANGED_PATHS`, or a GitHub `pull_request` git
range) **and** a dry-run diff with change items exists. They never filter which
apps the plan packs or executes — full desired-vs-observed reconcile remains the
execute semantics.

GitHub Actions also receives a **job step summary** (markdown table, plus a PR
preview section when labels are present) when `GITHUB_STEP_SUMMARY` is set.

### `change status`

| Key | Description |
|-----|-------------|
| `plan_id` | External plan id |
| `plan_status` | Plan lifecycle status from `GET /api/v1/plans/{id}` |
| `plan_summary` | Plan summary or change-count summary when diff is available |
| `progress_status` | Aggregate item-completion state |
| `total_items` | Change items in the plan |
| `fully_completed_items` | Items applied on every target host |
| `diff_*` | Same keys as verify when a persisted dry-run diff exists |

### `change validate`

Validation is plan-ID scoped. It validates only items already persisted in a
DAP plan; it does not upload or validate arbitrary local files.

```bash
# Human output; triggers validation and fails on blocking findings.
deslicer change validate \
  --plan-id "01994bdb-2d78-79d5-8f40-c03d342a3bc1" \
  --environment production

# Deterministic JSON for automation.
deslicer change validate \
  --plan-id "01994bdb-2d78-79d5-8f40-c03d342a3bc1" \
  --environment production \
  --format json > validation.json

# Markdown suitable for a GitHub PR comment.
deslicer change validate \
  --plan-id "01994bdb-2d78-79d5-8f40-c03d342a3bc1" \
  --environment production \
  --format markdown > validation.md
gh pr comment "${PR_NUMBER}" --body-file validation.md
```

Use `--report-only` to retrieve the latest report without running the model.
Direct Observer API keys can use report-only mode with `OBSERVER_API_URL` and
`DESLICER_API_TOKEN`; triggering validation requires the DAI proxy because DAI
owns model access and Observer accepts report writes only from the
portal-attested writer. `--force` bypasses reuse of an effective report.

The default `--fail-on block` exits zero for warnings. Use
`--fail-on warning` to gate on warnings, or `--fail-on never` to render
findings without gating. Operational validation errors still exit non-zero.

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

## Example: chaining steps in GitHub Actions

```yaml
- name: Plan
  id: plan
  run: deslicer change plan --environment production

- name: Deploy (after environment approval)
  run: deslicer change deploy --plan-id "${{ steps.plan.outputs.plan_id }}" --environment production
```

Omitting `--environment` asks deslicer-ai for every bound environment and creates one plan per name. Downstream steps that need a single plan should keep `--environment`, or split on `plan_ids`.

## Exit codes

`0` on success; non-zero codes map to specific failure classes. Plan
validation adds `20` (blocked), `21` (warning when configured), `22` (model
unavailable), `23` (timeout), and `24` (validation unavailable). The full table
lives in [oidc-troubleshooting.md](oidc-troubleshooting.md#exit-codes).
