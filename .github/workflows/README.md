# GitHub workflows — deslicer/cli

Review and auto-fix workflows are the same `deslicer-code-harness` `@v1` reuseables used by deslicer-ai (and the DAP review lane). Verify/test jobs are Rust-specific.

## Review / automation (harness)

| Workflow | Trigger | Reusable |
| --- | --- | --- |
| `cursor-code-review.yml` | Non-draft PRs (skips docs-only) | `core_cursor_code_review.yml` |
| `review-fix.yml` | `@slicer-fix` on a PR comment or review | `core_review_fix.yml` |
| `auto-fix-ci.yml` | Quality Gate failure on a PR | `core_auto_fix_ci.yml` |
| `issue-triage.yml` | New issue | `core_issue_triage.yml` |
| `issue-to-cursor-cloud-agent-pr.yml` | Issue labeled `agent:ready` | `core_issue_to_cursor_cloud_agent_pr.yml` |

## Verify / test

| Workflow | What it checks |
| --- | --- |
| `ci.yml` (`Quality Gate`) | rustfmt, clippy `-D warnings`, `cargo test --all-features` on Linux/macOS/Windows, `deslicer --help` smoke, `shellcheck` on `scripts/install.sh`. Draft PRs and bot automation branches skip. |
| `docs-sync.yml` | Push to `main` when `docs/**` (or the sync script/catalog) changes: copy curated pages into `deslicer/docs` `products/cli/` and open a PR. Does **not** publish to `docs.deslicer.io` (enterprise allowlist). Fail-closed without `CROSS_REPO_WORKFLOW_TOKEN`. |
| `secret-scan.yml` | TruffleHog `--only-verified` (same posture as DAP) |
| `workflow-syntax-check.yml` | actionlint on changed workflow YAML |
| `build-main.yml` | Five release-target edge builds after merge to `main` |
| `release.yml` / `homebrew.yml` / `crates-publish.yml` | Tag / publish (unchanged) |

Add **Quality Gate**, **Secret Scanning (Lightweight)**, and **Cursor Code Review** as required status checks on `main` once they have run once.

## Variables

| Variable | Scope | Used by |
| --- | --- | --- |
| `CURSOR_MODEL` | Org Actions variable (`vars.CURSOR_MODEL`) | `cursor-code-review.yml`, `review-fix.yml` — Cursor agent model id; defaults to `auto` when unset |

## Secrets

Copy these from deslicer-ai / DAP (org inherit is enough if the org already shares them):

| Secret | Required for |
| --- | --- |
| `CURSOR_API_KEY` | Code review, `@slicer-fix`, CI auto-fix, issue triage |
| `OPENAI_API_KEY` | Optional Codex fallback in code review |
| `CURSOR_CLOUD_API_KEY` | `agent:ready` → Cloud Agent PR (per-author `*_CURSOR_CLOUD_API_KEY` also works) |
| `CROSS_REPO_WORKFLOW_TOKEN` | `docs-sync.yml` PR into `deslicer/docs` (`contents:write` + `pull-requests:write` on that repo) |

Existing publish secrets (`CARGO_REGISTRY_TOKEN`, `HOMEBREW_TAP_TOKEN`) are unchanged.

## Labels used by automation

`agent:ready`, `agent:dispatched`, `no-autofix`, `priority: critical` / `high` / `medium` / `low`.
