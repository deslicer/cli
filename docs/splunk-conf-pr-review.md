# Splunk conf PR review (Code Review Agent)

Review Splunk `*.conf` (and optional `default.meta` / `local.meta`) changes on
pull requests using the public **Code Review Agent** in deslicer-ai, invoked
through `deslicer agent run`. The reusable workflow lives in this public
repository so customer config repos can call it without private harness access.

## What you get

- Advisory PR comment with a findings table
- `findings.json` artifact for auditing
- Check **fails only** when the agent reports `severity: error` (or JSON is missing/invalid)
- Warnings and infos do not fail the check

v1 is **diff-only** (no DAP / observed-host comparison yet).

## Prerequisites

1. A deslicer-ai tenant where **Code Review Agent** is seeded
   (`bun run db:seed-public-agents -- --override-existing` on the DAI side).
2. A portal **service account** (or user) that can run that public agent.
3. A **device session** for that account (CI OIDC is rejected for `deslicer agent`).

## Mint a device session for CI

On a trusted machine:

```bash
deslicer auth login --deslicer-api-url https://<your-portal>
# Prefer file store so you can copy the session without keychain tooling:
DESLICER_TOKEN_STORE=file deslicer auth login --deslicer-api-url https://<your-portal>
```

Session file (TOML):

```text
${DESLICER_CONFIG_DIR:-~/.config/deslicer}/credentials.toml
```

Fields: `cli_session_token`, `expires_at`, `tenant_id`, `display_name`,
`observer_api_url`, optional `tenant_slug`, `deslicer_api_url`.

Store the **entire file contents** (or a JSON object with the same fields) as a
GitHub Actions secret named `DESLICER_DEVICE_SESSION`. Rotate before
`expires_at`.

Never commit the session file. Treat it like a password.

## Wire the workflow

Copy [`.github/workflows/examples/splunk-conf-review-caller.yml`](../.github/workflows/examples/splunk-conf-review-caller.yml)
into your config repo, or add:

```yaml
jobs:
  review:
    uses: deslicer/cli/.github/workflows/splunk-conf-review.yml@main
    with:
      agent_name: "Code Review Agent"
    secrets:
      DESLICER_DEVICE_SESSION: ${{ secrets.DESLICER_DEVICE_SESSION }}
```

Optional inputs: `deslicer_api_url`, `cli_version`, `cli_scripts_ref`,
`post_pr_comment`.

## Local helpers

From this repo:

```bash
BASE_SHA=… HEAD_SHA=… ./scripts/ci/splunk-conf-review/collect-diff.sh
./scripts/ci/splunk-conf-review/test-parse-findings.sh
```

## Findings contract

Agent stdout must include JSON shaped like:

```json
{
  "summary": "…",
  "ok": true,
  "findings": [
    {
      "severity": "error",
      "file": "apps/…/props.conf",
      "stanza": "…",
      "key": "…",
      "message": "…",
      "suggestion": "…"
    }
  ]
}
```

See deslicer-ai skill `splunk-conf-pr-review` / `references/finding-schema.md`.

## Phase 2 (not in this workflow yet)

Compare PR desired stanzas to observed DAP `config_items` for destinations’
`inventory_group` values from `.deslicer/environments/<env>.yml`.
