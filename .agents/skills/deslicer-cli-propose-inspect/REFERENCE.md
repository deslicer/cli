# Commands and troubleshooting

## Contents

- Version and authentication
- Choosing source and target
- Propose examples
- Inspecting output and state
- Validation and failures
- Human handoff

## Version and authentication

Use installed `--version` and subcommand `--help` as the command-surface authority. Consult source/docs for that release for semantics. A newer compatible version is not a reason to downgrade. An event-specific exact pin must come from confirmed session requirements.

If installation is requested, use the repository's documented Homebrew, cargo, or release installer paths; there is no npm installation path documented by this repo. For an exact release, set `DESLICER_VERSION` and use the installer from the same tag. Resolve the approved tag before running it; do not copy an old workshop pin automatically.

Legacy: pre-v1.6.0 self-updaters cannot follow GitHub asset redirects. They need a one-time documented installer/Homebrew upgrade. This is an installation migration, not a reason to downgrade newer binaries.

For this workflow, the operator provisions `OBSERVER_API_URL` and a tools-scope `DESLICER_API_TOKEN` out of band. The token has no CLI flag. `DESLICER_DEV_TOKEN` is retired. Do not obtain Splunk credentials or change authentication mode just to bypass an error.

```bash
deslicer --version
deslicer auth whoami --log-format json
deslicer groups list --environment "$DESLICER_ENVIRONMENT" --log-format json
```

`DESLICER_ENVIRONMENT` here is a shell variable containing the confirmed environment, not a documented CLI environment setting. The explicit flag passes it to the CLI.

In direct mode, `whoami` checks locally configured credentials; `logged_in: true` does not prove token validity, tenant identity, or permission. A successful groups request proves that operation was accepted at the configured Observer. Confirm the intended tenant/backend with the operator's configuration. A 401/403 remains a failure even if `whoami` succeeds.

## Choosing source and target

Use `groups list --log-format json` for exact `name` and `id`; the human table can use `display_name`, which is not the exact-name lookup key. Inspect `inventory list` if host membership needs checking. A UUID-shaped target is accepted locally without a lookup, so passing one does not verify membership or scope. Do not broaden to another group because the intended one is empty.

Supply `--environment` and `--target-group` explicitly. Automatic environment discovery can create multiple plans in CI; direct git mode can infer a target from one environment-YAML destination, but an explicit target makes the handoff auditable.

Choose source deliberately:

| Source | Preconditions and consequences |
|---|---|
| Git, default | Verify repository identity and remote availability of the exact commit. Local uncommitted edits are excluded. Private repos need an available clone credential; the tools API token is not itself a git credential. |
| `--source-dir` bundle | Needed when local bytes are the intended proposal or cloning is unavailable. Requires a target group. Review the actual source tree before uploading; use a directory containing only intended Splunk configuration. |

Do not commit/push local edits merely to make a git plan work unless the task authorizes those actions. If source or permission to upload is unclear, resolve it before creation.

The reviewed packer recursively includes regular files, including hidden and Git-ignored files. It skips symlinks and directories named `.git`, `.svn`, `.hg`, `node_modules`, and `__pycache__`; it does not apply `.gitignore`. Review filenames without printing secret contents. Never assume `--source-dir .` is safe. If a tree contains secrets, use a reviewed, sanitized staging tree preserving the required app layout. Do not remove files from the user's original tree to sanitize it.

The reviewed compressed-bundle limit is 32 MiB. Bundles do not resolve Git LFS pointers; use a valid git source or ensure intended materialized contents and supported layout before uploading. Source availability does not establish that every app in the resulting reconcile is safe.

`--changed-paths` and `--changed-paths-file` are PR preview labels only. They do not restrict app packing, reconcile, or eventual execution.

## Propose examples

These are alternatives, not commands to run in sequence. Shell variables must already contain confirmed non-secret values; credentials remain out of band. Omit `--no-wait` unless early return is specifically useful.

Git source, with the intended commit available to Observer:

```bash
deslicer change plan \
  --environment "$DESLICER_ENVIRONMENT" \
  --target-group "$DESLICER_TARGET_GROUP" \
  --name "Review Splunk configuration" --log-format json
```

Reviewed local source:

```bash
deslicer change plan \
  --environment "$DESLICER_ENVIRONMENT" \
  --target-group "$DESLICER_TARGET_GROUP" \
  --source-dir "$DESLICER_SOURCE_DIR" \
  --ci-platform local --name "Review local Splunk configuration" --log-format json
```

For direct git mode, the code recognizes `DESLICER_GIT_CLONE_TOKEN`, with `GITHUB_TOKEN` fallback for GitHub/local platforms. Have the operator configure suitable repository access; never print or insert these credentials in argv. Do not assume GitLab job tokens are interchangeable.

If create times out or returns an existing plan, use scoped `change show --environment "$DESLICER_ENVIRONMENT"` and known plan IDs before another create. The CLI's reduced plan model may omit source and target details. If those cannot be established, flag the ambiguity for portal review rather than claiming verified reuse.

## Inspecting output and state

For follow-ups, prefer returned external `plan_id`, not internal `id`/`plan_row_id`. Older responses may lack `plan_id`; the CLI's `external_id()` falls back to `id`. Treat that as a compatibility fallback, not a general instruction to substitute row IDs.

At the reviewed revision, single-plan `show` and `plan` print a plan record and may append a second JSON record of CI values locally. Lists print an array. Other CI environments may write output files or Azure logging lines. `--log-format json` does not guarantee one JSON document for every lifecycle command. Human output for `change show` is also JSON at this revision.

Parse all JSON records, distinguish arrays from objects, and check consistent plan IDs/status values. Do not pick an arbitrary last line; conflicting records require fresh inspection. Never use `eval` or source output as shell code. Capture the CLI exit code before parsing so a parser success cannot hide a command failure. Preserve diagnostics separately from stdout.

| Evidence | Meaning |
|---|---|
| `draft`, `compiling`, `compile_pending` | Compilation or proposal readiness is not complete. |
| `pending_approval` from fresh `show` | Ready for human review; not proof of safe changes or valid execution. |
| `failed`, `compile_failed`, `rejected` | Report the observed failure; do not represent it as ready. |
| `approved_unsigned`, `executing`, `completed` | Report observed state; do not initiate additional transitions. |
| `progress_status: not_started` | Execution has not started; says nothing definitive about compile readiness. |

`change status` reports execution progress and may return exit 0 for incomplete progress. Refresh `show` for lifecycle state. If waiting for compile, use bounded read-only checks; report the last observed state if the deadline expires. Do not recompile with `verify` or create another plan to poll.

## Validation and failures

Always use `change validate --plan-id "$PLAN_ID" --report-only --format json` for inspection. `--format` selects the validation payload, separately from global `--log-format`. Direct tools tokens support report-only; triggering validation requires the DAI proxy in the reviewed code.

Report verdict, findings, and exit code. Default `--fail-on block` permits warning reports to exit 0. Validation verdict `block` exits 20, warning with `--fail-on warning` exits 21, and unavailable/error verdicts exit 24. Transport/auth failures use their own errors; an HTTP 502 is not necessarily exit 24. Do not add `--fail-on never` to conceal a failure.

| Symptom | Response |
|---|---|
| Missing token, 401, 403 | Report redacted error and endpoint; operator corrects credentials/scope out of band. |
| Missing/ambiguous group | Check exact name/ID and confirmed environment; do not substitute another target. |
| Validation unavailable/502 | Report “validation unavailable,” not pass or “safe to run”; preserve the gap in handoff. |
| Missing diff | Report unknown scope/destructive effects; a plan name or summary is insufficient proof. |
| Approval demo expected to fail | Do not attempt; tools scope and MFA rejection are not a universal safety guarantee. |
| `verify` suggested for assurance | Decline within this workflow: it triggers compilation; use show/status/report-only. |

## Human handoff

Provide the observed plan ID and lifecycle state even when still compiling. Build `<confirmed-portal-base>/dashboard/dap/plans/<external-plan-id>` only when that portal route is established for the deployment; otherwise provide the ID and request/locate the correct portal route. Never derive the portal host from the environment stem.

Include source identity, target, version, validation, diff limitations, and whether approval readiness was actually observed. Human approval/execution happens in the portal. Exact button labels and team selection are deployment-specific; the optional workshop profile records supplied labels as unverified examples. A plan compile is not deployment verification.
