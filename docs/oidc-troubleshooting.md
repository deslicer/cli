# OIDC troubleshooting

Use `deslicer auth status` as the first diagnostic step in any CI job. It prints the detected platform, OIDC availability, environment binding, and resolved Observer URL without mutating state.

```bash
deslicer auth status --environment production
```

Run it **before** `deslicer auth login` when debugging token or binding failures.

---

## Per-platform walkthrough

### GitHub Actions

1. Confirm the workflow job declares `permissions: id-token: write`.
2. Run `deslicer auth status` — expect `platform: github` and OIDC request URL present.
3. If OIDC vars are missing, GitHub did not inject `ACTIONS_ID_TOKEN_REQUEST_URL` / `ACTIONS_ID_TOKEN_REQUEST_TOKEN` (missing permission or fork PR restrictions).
4. Run `deslicer auth login --environment <name>`.

### GitLab CI

1. Add an `id_tokens:` block with `aud: https://api.deslicer.ai`.
2. Run `deslicer auth status` — expect `DESLICER_OIDC_TOKEN` or `CI_JOB_JWT` to be detected.
3. Run `deslicer auth login`.

### Azure DevOps

1. Enable **Allow scripts to access the OAuth token** on the job.
2. Export `SYSTEM_OIDCREQUESTURI` and `SYSTEM_ACCESSTOKEN` into the step env.
3. Run `deslicer auth status`, then `deslicer auth login`.

### Bitbucket Pipelines

1. Set `oidc: true` on the step.
2. Confirm `BITBUCKET_STEP_OIDC_TOKEN` is set (visible in `deslicer auth status` metadata, not logged in full).
3. Run `deslicer auth login`.

### Local

1. Set `DESLICER_DEV_TOKEN` from the Deslicer portal (dev/staging only).
2. Run `deslicer auth login --environment local` with `--ci-platform local`.

---

## Exit codes

| Code | Name | Typical cause | Fix |
|------|------|---------------|-----|
| **0** | OK | Command succeeded | — |
| **1** | Generic error | Unexpected failure (network parse, internal error) | Re-run with `--log-format json`; check stderr |
| **4** | OIDC rejected | Token invalid, expired, or wrong audience | Verify audience is `https://api.deslicer.ai`; refresh job token; check clock skew |
| **5** | Repo not allowlisted | Repository not registered for this tenant/environment | Add repo allowlist in Deslicer portal → CI integrations |
| **6** | Env not bound | Named environment has no CI binding for this repo | Create binding in portal or pick a bound environment name |
| **7** | Ambiguous binding | Multiple bindings match (same repo + environment) | Deduplicate bindings in portal; use UUID-named env file (see [environments.md](environments.md)) |
| **8** | Unsupported platform | CI platform not recognized and no `--ci-platform` override | Set `--ci-platform` explicitly; see [installation.md](installation.md) |
| **9** | Rate limited | deslicer-ai or Observer returned 429 | Retry with backoff; reduce concurrent jobs |
| **10** | Backend unavailable | resolve-backend or Observer unreachable | Check `DESLICER_API_URL`; verify Observer health; use `OBSERVER_API_URL` air-gap override if approved |
| **11** | Plan not found | Plan ID does not exist or wrong tenant | Verify plan ID and environment; confirm auth succeeded first |

---

## Common failure patterns

### `auth status` shows platform `auto` but exits 8

The runner environment lacks recognizable CI variables. Set `--ci-platform` explicitly:

```bash
deslicer --ci-platform github auth status
```

### OIDC works locally in GitHub but fails on fork PRs

GitHub withholds `id-token: write` on pull requests from forks. Use `pull_request_target` only with extreme care, or run OIDC-gated steps on `push` to trusted branches.

### resolve-backend succeeds but change commands fail with 10

Observer may be down or firewalled from the runner. Confirm the resolved URL in `auth login` output. For air-gapped deployments, set `OBSERVER_API_URL` to bypass resolve (operator-approved only).

### Exit 5 on monorepo subdirectory builds

Allowlist entries may key off the top-level repository slug. Ensure the GitHub/GitLab project path registered in the portal matches `GITHUB_REPOSITORY` / `CI_PROJECT_PATH`.

### Exit 7 after renaming an environment

Stale and new bindings may both match. Remove duplicates in the portal, or pin the environment with a UUID filename under `.deslicer/environments/` (see [environments.md](environments.md)).

---

## Debug checklist

1. `deslicer auth status --log-format json`
2. Confirm OIDC audience `https://api.deslicer.ai`
3. Confirm environment name matches a file stem in `.deslicer/environments/` (or portal binding)
4. Confirm repo is allowlisted for that environment
5. `deslicer auth login` then retry the failing `change` subcommand
