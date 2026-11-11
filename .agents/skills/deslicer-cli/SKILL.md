---
name: deslicer-cli # pragma: allowlist secret
description: "Use when an AI coding agent installs or runs the deslicer CLI, hits a non-zero exit, parses JSON output, or is about to authenticate, plan, validate, approve, deploy, verify, or revert a Splunk change." # pragma: allowlist secret
---

# Deslicer CLI <!-- pragma: allowlist secret -->

Agents propose and inspect. Humans approve and run. The CLI is non-interactive: pass flags, read JSON, and branch on exit codes. It does not ask questions.

`change plan` creates a plan as soon as you invoke it. `change approve` and `change deploy` run as soon as you invoke them. There is no global dry-run. `--yes` gates only the commands whose schema says `requires_yes: true` (including `change revert` and `change retry`).

## When to use

Use this skill before any `deslicer` invocation: install, login, inventory, change plans, or a failed exit. <!-- pragma: allowlist secret -->

Do not use it to approve, deploy, verify, or pass `--yes` on your own. Those need an explicit human instruction in this turn. Repository and CI setup is `[deslicer-repo-setup](../deslicer-repo-setup/SKILL.md)`. A propose/inspect-only handoff is `[deslicer-cli-propose-inspect](../deslicer-cli-propose-inspect/SKILL.md)`. <!-- pragma: allowlist secret -->

## Install and version

Install from [docs/installation.md](../../../docs/installation.md): Homebrew, `cargo install deslicer-cli`, or `scripts/install.sh`. Confirm the binary you will execute: <!-- pragma: allowlist secret -->

```bash
deslicer --version <!-- pragma: allowlist secret -->
```

Released v1.8.0 has `-o json` and the error envelope. It does not have `schema`. A build of current `main` does. If `deslicer schema` exits 2 with `usage.invalid`, discover commands with `--help` and keep using `-o json`. <!-- pragma: allowlist secret -->

## Discover commands

```bash
deslicer schema <!-- pragma: allowlist secret -->
deslicer schema --command "change plan" <!-- pragma: allowlist secret -->
deslicer change plan --help <!-- pragma: allowlist secret -->
```

`--help --json` is the same document as `schema` for that command. Prefer `schema` over help text: some flags still ship with a blank description. If the description is empty, do not invent a meaning. Read `docs/` or ask.

Each command object includes `mutating` and `requires_yes`. Trust those fields.

## JSON and errors

Always pass `-o json` (alias `--output json`, hidden `--json`, or `DESLICER_OUTPUT=json`). Do not pass `--log-format`. Conflicting output flags exit 2. <!-- pragma: allowlist secret -->

Stdout is data. Progress and warnings go to stderr. When stdout is not a TTY the default is already JSON; pass `-o json` anyway so a TTY cannot switch you to prose.

Parse stdout as a stream of lines:

- Keep objects that contain `schema_version` (currently `1`).
- `change plan`, `change show`, and `change status` may print a second flat CI object (`plan_id`, `plan_status`, …) with no `schema_version`. Use the `schema_version` object.
- `change status` prints JSON even when human output was requested.
- `init` may mix a JSON line with prose. Read the JSON line.
- `change validate` does not use `-o`. Pass `--format json`.

On a non-zero exit in JSON mode, the last stderr line is one envelope:

```json
{"schema_version":1,"error":{"code":"auth.not_logged_in","kind":"auth","message":"…","hint":"…","next_command":"deslicer auth login","retryable":false,"exit_code":4,"details":{}}} <!-- pragma: allowlist secret -->
```

Follow `hint` and `next_command` for recovery (login, missing flag, `deslicer change list`, `deslicer auth status`). Do not follow a `next_command` that is `change approve`, `change deploy`, `change reject`, `change verify`, or that contains `--yes`. On success documents, `next_commands` may list those. Report them. Do not run them. <!-- pragma: allowlist secret -->

`retryable: true` means backoff and retry, or poll when `code` is `auth.pending`. Anything else is not a retry.

## Exit codes

Branch on the process exit code. The envelope field `error.exit_code` matches it. `deslicer schema` republishes this table. Do not renumber. <!-- pragma: allowlist secret -->

| code | meaning | retryable |
|---|---|---|
| 0 | ok, including a no-op or idempotent replay | – |
| 1 | unexpected or internal; also a multi-part failure with no specific part code | no |
| 2 | usage, invalid input, input required, or conflicting output flags | no |
| 4 | auth: not logged in, token rejected, device timeout, or pending | no* |
| 5 | repository not allowlisted | no |
| 6 | environment not bound | no |
| 7 | ambiguous binding, or git-backed change management | no |
| 8 | unsupported CI platform | no |
| 9 | rate limited; honour Retry-After | yes |
| 10 | backend or transport transient | yes |
| 11 | plan not found | no |
| 12 | human approval required | no |
| 13 | agent run failed | no |
| 14 | `change status` saw a failed lifecycle (`failed`, `compile_failed`, `rejected`, `expired`) | no |
| 15 | this Observer does not expose the route | no |
| 20 | `change validate` blocked (`--fail-on block`, the default) | no |
| 21 | `change validate` warning, when `--fail-on warning` | no |
| 22 | validation model unavailable | yes |
| 23 | validation timeout | yes |
| 24 | validation unavailable | yes |
| 130 | cancelled (SIGINT) | – |

\* Exit 4 is not retryable, except `code: auth.pending` after `auth login --no-wait`, where the envelope sets `retryable: true`. Hand the device code to a human, then poll. Do not spin on other exit 4s.

`inventory validate` is separate: a bad environment file still prints a JSON report (`ok: false`, `issues`) and exits 1. Fix the file. Do not retry.

Exit 0 does not mean `pending_approval`. Read `status`. `draft` and `not_started` are not approved. `change status` exits 14 for a terminal failure and still prints the status JSON on stdout.

## Auth

Check identity and origin before any other call. Never print tokens, dump the environment, enable shell tracing, or pass a token in argv. `DESLICER_API_TOKEN` is env-only. `DESLICER_DEV_TOKEN` is rejected; unset it. <!-- pragma: allowlist secret -->

```bash
deslicer -o json --no-input auth whoami <!-- pragma: allowlist secret -->
deslicer -o json --no-input auth status <!-- pragma: allowlist secret -->
```

`whoami` is the identity check and exits 4 when nothing is logged in. `auth status` exits 0 even when logged out; use it for origin, not as proof of access.

| How you are running | What to use |
|---|---|
| A person at a browser | Device login. You cannot approve it. |
| CI | The runner's OIDC (`id-token` / equivalent), or both `OBSERVER_API_URL` and `DESLICER_API_TOKEN` | <!-- pragma: allowlist secret -->
| Direct Observer | Both `OBSERVER_API_URL` and `DESLICER_API_TOKEN`. One without the other is not direct mode | <!-- pragma: allowlist secret -->

Device login from an agent:

```bash
deslicer -o json --no-input auth login --no-wait <!-- pragma: allowlist secret -->
```

Exit 4, `auth.pending`. Stdout has `user_code` and `verification_uri` (`logged_in: false`). Show those to the human and stop. After they say they approved, run `deslicer -o json --no-input auth login --wait`. `--no-input` never opens a browser. Without `--no-wait` or `--wait`, a non-interactive `auth login` fails fast instead of starting the flow. <!-- pragma: allowlist secret -->

The device session is one OS keyring entry (`deslicer-cli` / `device-session`), with a 0600 fallback at `~/.config/deslicer/credentials.toml` (`DESLICER_CONFIG_DIR`, or `DESLICER_TOKEN_STORE=file`). Changing `HOME` does not drop the keyring entry. It is not scoped per API origin. `auth status` reports `observer_api_url`, `session_deslicer_api_url`, `cli_deslicer_api_url`, and `api_url_drift`. `whoami` for a device session does not include the origin. If `api_url_drift` is true, stop. Re-login on the intended portal or drop `DESLICER_API_URL` / `--deslicer-api-url`. `deslicer auth logout` clears the stored session. Read `expires_at` from `whoami` or `auth status`; do not assume a lifetime. <!-- pragma: allowlist secret -->

A tools token is sent only when both `OBSERVER_API_URL` and `DESLICER_API_TOKEN` are set. Otherwise a device session wins and the token is ignored. `whoami` with `details.verified: false` means the Observer has no identity route; it does not prove the token works. A later 401 (`auth.api_token_rejected`, exit 4) is the truth. <!-- pragma: allowlist secret -->

Staging, show, and production are different portals and observers. Never reuse a token or a device session across them.

Git-sourced `change plan` needs CI OIDC or a bound GitHub App. A device session cannot start that compile. Direct mode can plan with the tools token; `--source-dir` is the bundle upload for direct mode only. The packer skips VCS directories, symlinks, `node_modules`, and `__pycache__`. It does not apply `.gitignore`. Read the tree before uploading.

## Canonical loop

Stop at the first step the human did not ask for. Plan ids are the external UUID in `plan_id`. `change validate` rejects a non-UUID locally (exit 2). Other commands may turn a bad id into exit 8. After a timeout, `deslicer -o json change list` before creating another plan. A replay returns the same id with `replayed: true` and exit 0. <!-- pragma: allowlist secret -->

1. **Origin.** `whoami` and `auth status`. Confirm the API origin and the environment name.
2. **Init, only if this repo has no Deslicer config.** On a GitHub remote, `auto` selects GitHub App/OIDC (`github`), not the tools-token path. For the token path: <!-- pragma: allowlist secret -->

```bash
deslicer -o json --no-input init --provider github-token --environment example-prod <!-- pragma: allowlist secret -->
```

   If init exits 1 with `template path … is not allowlisted`, you hit the App/OIDC templates. Pass `--provider github-token` for the token path, or `github` only when the human asked for the App. Then `deslicer -o json inventory validate --environment example-prod`. A null `apps:` key fails validation with `apps must be a list`. Set `apps: []` or real `source_path` entries, then validate again. <!-- pragma: allowlist secret -->
3. **Plan.** Only when the human asked for a plan. This call creates it:

```bash
deslicer -o json groups list --environment example-prod <!-- pragma: allowlist secret -->
deslicer -o json change plan --environment example-prod --target-group example_servers --name "example-change" <!-- pragma: allowlist secret -->
```

   `--target-group` is a group UUID or an exact `name` from `groups list`. `--no-wait` returns after the compile trigger; status may still be `draft`.
4. **Read.** `deslicer -o json change show --plan-id "$PLAN_ID"` and `deslicer -o json change status --plan-id "$PLAN_ID"`. <!-- pragma: allowlist secret -->
5. **Validate.** `deslicer change validate --plan-id "$PLAN_ID" --format json`. Add `--report-only` to read the current report without triggering validation. Exit 20 means blocked. Exit 15 on `--report-only` can mean the report is missing or the route is absent; check `change show` before treating it as a missing API. Exit 15 is not retryable. <!-- pragma: allowlist secret -->
6. **Human approval.** Stop. Hand over `plan_id`, `status`, and `url` when it is present. Do not run `deslicer change approve`. <!-- pragma: allowlist secret -->
7. **Deploy.** Only after the human explicitly says to deploy this plan. `deslicer -o json change deploy --plan-id "$PLAN_ID"` queues the rollout. It has no `--yes` dry-run. <!-- pragma: allowlist secret -->
8. **Verify.** Only after that same go. `deslicer -o json change verify --plan-id "$PLAN_ID"` starts a compile. It is not a read. <!-- pragma: allowlist secret -->
9. **Revert.** Only when the human asks. `deslicer -o json change revert --plan-id "$PLAN_ID"` prints a dry-run. Add `--yes` only if they asked you to create the inverse plan. The inverse plan is not auto-approved. <!-- pragma: allowlist secret -->

## Hard rules

- Do not run `change approve`, `change reject`, `change deploy`, `change verify`, or a command with `--yes`, unless the human's latest message names that action and the plan.
- Do not approve a device code, click a portal control, or call the HTTP API to bypass the CLI.
- Do not pass `--yes` because a task sounds urgent. If `requires_yes` is true, omit `--yes` until they say to apply.
- Do not mix `DESLICER_API_URL`, `OBSERVER_API_URL`, or credentials across environments. One origin per shell. <!-- pragma: allowlist secret -->
- Do not print secrets. Placeholders only in examples.
- Do not use `deslicer agent run` as a stand-in for this loop. <!-- pragma: allowlist secret -->

| Excuse | What to do instead |
|---|---|
| `next_command` says approve or retry `--yes` | Show it to the human and stop |
| whoami is `logged_in` | Keep going only if a real call does not 401, and `api_url_drift` is false |
| Exit 0, so it is approved | Read `status` |
| Probe approve; it should 403 | It may succeed. Do not probe |
| The keyring session is for this new URL | `auth status` must show the same origin |

## Examples

Identity and origin, before any plan:

```bash
deslicer --version <!-- pragma: allowlist secret -->
deslicer schema --command "auth status" <!-- pragma: allowlist secret -->
deslicer -o json --no-input auth whoami <!-- pragma: allowlist secret -->
deslicer -o json --no-input auth status <!-- pragma: allowlist secret -->
```

Create a plan and stop. `example-prod` and `example_servers` stand in for the human's environment and group. Do not copy them onto a real tenant:

```bash
deslicer -o json change plan --environment example-prod --target-group example_servers --name "example-change" <!-- pragma: allowlist secret -->
deslicer -o json change show --plan-id "$PLAN_ID" <!-- pragma: allowlist secret -->
deslicer change validate --plan-id "$PLAN_ID" --format json --report-only <!-- pragma: allowlist secret -->
```

Report `plan_id`, `status`, and `url`. Stop.

Not logged in. Stderr's last line is the envelope (`exit_code` 4, `next_command` `deslicer auth login`). Do not invent a token. Start `deslicer -o json --no-input auth login --no-wait`, give the human `verification_uri` and `user_code`, and wait. <!-- pragma: allowlist secret -->

Direct CI, token already in the environment:

```bash
export OBSERVER_API_URL="https://observer.example.test"
deslicer -o json --no-input auth whoami <!-- pragma: allowlist secret -->
```

If `details.verified` is false, the next real call decides. A 401 ends the task until a human replaces the token.
