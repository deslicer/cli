# Deslicer CLI: agent-native contract (spec v1.1, 2026-10-29)

Sources: clig.dev, aclig.dev (Agent CLI Guidelines), clispec.dev v0.2, gh `--json/--jq`, kubectl `-o json`, Stripe CLI `--format json`, and MCP tool `inputSchema` (JSON Schema) as an **external** adapter pattern (not a CLI deliverable).

This document is the contract for making the Deslicer CLI safe and predictable for non-interactive agents. It amends v1 with locked grill decisions (see [Appendix A](#appendix-a-decision-log)). It describes proposed work, not installed capabilities, except where the gap table notes landed fixes.

## 1. Output

- stdout carries data only; logs, progress and deprecation notices go to stderr.
- Canonical flag: global `-o` / `--output json|human`, with env override `DESLICER_OUTPUT`.
  - `--json` remains accepted (may be hidden from help).
  - `--log-format` is deprecated for two minor versions, then hidden; prefer `--output`.
  - Conflicting values among `--output`, `--json`, `--log-format`, and `DESLICER_OUTPUT` → exit **2**.
- **Default format:** JSON when **stdout is not a TTY**. A TTY stays human.
  - Do **not** flip the default on `CI=true` alone. CI runners that pipe or redirect stdout already get JSON; a local TTY in CI-labelled shells stays human.
  - Override always available via `--output` / `DESLICER_OUTPUT`.
- Every JSON document (success **and** error) carries `"schema_version": 1`, added in one wave. Fields are only added, never renamed. A breaking change bumps the version. Each version is snapshot-tested. Human output is unscathed (no version banner required).
- No ANSI colour when not a TTY or when `NO_COLOR` is set. A closed pipe (EPIPE / SIGPIPE) exits quietly — never panics (P0 #219).

## 2. Error envelope

**JSON mode:** the **last line of stderr** is exactly one error envelope object. Diagnostic logs may precede that line; nothing may follow it.

```json
{"schema_version":1,"error":{"code":"auth.not_logged_in","kind":"auth","message":"…","hint":"…","next_command":"deslicer auth login","retryable":false,"exit_code":4,"details":{}}}
```

- `code` is a stable dotted id.
- `kind` is the coarse class that maps to the exit code.
- `next_command` is runnable as-is (with real ids interpolated), or `null`.
- Usage (clap) errors use the same envelope in JSON mode.
- **Human mode:** free-form stderr; no requirement for a trailing envelope line.

## 3. Exit codes

Publish the **full** table in `deslicer schema` and `docs/exit-codes.md`. A test asserts every `CliError` variant maps to a row. **Do not renumber** existing codes.

| code | meaning | retryable |
|---|---|---|
| 0 | ok (also a no-op or idempotent replay) | – |
| 1 | unexpected or internal error (should be rare); also fallback when a multi-part command fails without a more specific part code | no |
| 2 | usage / invalid input / input required / conflicting output flags | no |
| 4 | auth (not logged in, token rejected, device-login timeout / pending) | no\* |
| 5/6/7 | repo not allowlisted / env not bound / ambiguous binding | no |
| 8 | unsupported platform | no |
| 9 | rate limited (honour Retry-After) | yes |
| 10 | backend or transport transient | yes |
| 11 | plan not found | no |
| 12 | human approval required | no |
| 13 | agent run failed | no |
| 14 | plan lifecycle failed | no |
| 15 | endpoint not available on this Observer | no |
| 20 | validation blocked | no |
| 21 | validation warning (treated as failure unless policy says otherwise) | no |
| 22/23/24 | validation model unavailable / timeout / unavailable | yes |
| 130 | cancelled (SIGINT) | – |

\* `auth.pending` after `--no-wait` is `retryable: true` in the envelope even though exit 4 is the auth class; agents may poll.

## 4. Self-description

- `deslicer schema [--command "<path>"]` is the **contract surface**. It emits JSON listing every command path, with: summary, flags (name, type, required, default, enum, env), positionals, `mutating`, `requires_yes` (only for true side-effect apply paths — see §6), `paginated`, the error codes and exit codes it can emit, and an output JSON Schema reference.
- `--help --json` is an alias for `schema --command <path>`.
- The output is generated from the clap tree and snapshot-tested.
- **MCP is an external consumer**, not shipped in this CLI programme. Do not treat “maps 1:1 onto an MCP tool” as a CLI deliverable. External adapters **may** generate MCP tools from `deslicer schema` output (`name` ≈ `deslicer_change_plan`, `inputSchema` ≈ flag schema).

## 5. Non-interactive

- `--no-input` / `DESLICER_NO_INPUT`, implied when stdin is not a TTY or `CI=true`.
- The CLI never prompts or opens a browser in that mode. Where it would prompt, it exits 2 with `code: input.required` and `next_command`.
- **Device login (non-interactive):**
  - Print `{verification_uri, user_code, …}` as JSON on stdout (or the documented success path).
  - **Never** open a browser.
  - Poll until success (exit 0) or timeout (exit 4).
  - `--no-wait` → exit 4 with `code: auth.pending`, `retryable: true`, so agents can re-poll without hanging.

## 6. Dry-run, `--yes`, and idempotency

**Dry-run / `--yes` is NOT universal.**

- Dry-run-by-default + `--yes` to apply remains only where it is **already gated** today (e.g. `acs credentials set`, `environments mappings set`, `inventory apps adopt`, `change approve`, `change deploy`, and similar apply paths).
- **Create-draft** paths such as `change plan` create on invoke **without** requiring `--yes`. Agents expect a plan id back from a single call.
- `schema` sets `requires_yes: true` **only** for true side-effect apply commands, not for draft creation.
- **Idempotency (#228):** covers replay of creates, not a forced `--yes` on plan create.
  - Accept `--idempotency-key`, sent as `Idempotency-Key`.
  - Default a key **only** when a CI run id is present **or** the caller set `--idempotency-key`.
  - **No** silent local input-hash replay (avoids surprising “same args → same plan” on a laptop).
  - A server-side replay returns the same id with `"replayed": true` and exit 0.

## 7. Handles and next steps

Every command that creates or advances a plan returns:

```json
{"schema_version":1,"plan_id":"…","status":"…","url":"…","next_command":"…","next_commands":["…"]}
```

Rules:

- Always set singular `next_command` when one primary next step exists.
- Use `next_commands[]` for alternatives / optional follow-ups.
- Use `null` / empty when there is no next step.
- Interpolate **real** ids into suggested commands (never placeholders like `<plan-id>` in machine output).
- Never suggest an unguarded mutating apply (any suggested mutate must include the same gates the command itself requires, e.g. `--yes` where applicable).
- Human output shows the same values in readable form.

## 8. Pagination and filtering

- List commands take `--limit` (default 50, max 500), `--cursor`, and typed filters such as `--status`, `--since`, and `--environment`.
- Output: `{schema_version, items[], next_cursor|null, partial:false}`.
- **`change list`:** read-only tenant list so agents can find a plan they created earlier.
  - Auth like `change show`.
  - Supports `--limit` / `--cursor` / filters.
  - If the Observer lacks the route → exit **15** (`endpoint not available`), not a misleading plan-not-found.

## 9. Timeouts, retries, and partial results

- Global `--timeout <s>` / `DESLICER_TIMEOUT` sets the total wall-clock limit. `--retries N` (default 2) applies jittered backoff to idempotent GETs on 429/502/503/504 and connect errors, honouring Retry-After.
- **Partial results (strict default):**
  - On partial failure, set `"partial": true` plus a `<part>_error` object and exit **non-zero**.
  - `--allow-partial` → exit **0** with `partial: true` (caller accepts incomplete data).
  - **No new exit code** for partial.
  - If only one part failed, use that part’s exit code; otherwise exit **1**.
  - Example: `change status --jobs` should report `jobs_error` when jobs cannot be loaded.

## 10. Stable naming

- Commands are noun-verb (`change plan`, `change approve`). Old names stay as hidden aliases with a stderr deprecation notice for two minor versions. See #187 / #211.
- Flag names are consistent: `--plan-id`, `--timeout` (not `--timeout-secs`, see #133), `--environment`.
- Output aliases follow §1 (`--output` canonical; `--json` retained; `--log-format` deprecated then hidden).

## Gap analysis: v1.7.3 (d338f72) baseline

Evidence comes from `check.sh` (check-v1.7.3.txt, raw outputs in run-v1.7.3/) and REPORT.md / sweep-173. Status is annotated with open/merged fix PRs where known. The weekly check dump below is a **snapshot** and may disagree with the gap table on individual rows (notably A7 / SIGPIPE): treat the gap table + issue numbers as authoritative for P0 work.

| # | Spec point | v1.7.3 | Status | Evidence | Issue / PR |
|---|---|---|---|---|---|
| 1 | JSON when not a TTY (not CI alone) | errors are plain text unless `--log-format json` | gap | A1 | #221 |
| 1 | global `--output` / `-o` | `unexpected argument '--output'`, exit 2 | gap | A2 | #221 |
| 1 | `schema_version` on all JSON | absent everywhere | gap | A5 | #223 |
| 1 | stdout clean / no ANSI | JSON errors go to stderr, stdout empty, no ANSI | ok | A4, A6 | – |
| 1 | EPIPE / SIGPIPE safe | `--help \| head` can panic (reporting.rs) | **gap (P0)** | A7 vs check dump | #219 — check dump showed PASS; still treat as P0 until verified fixed |
| 2 | envelope fields | has kind, message, hint; no `code` or `next_command` | partial | A3 | #221 |
| 2 | last stderr line = one envelope (JSON mode) | not guaranteed | gap | – | #221 |
| 2 | HTML in errors | raw HTML body embedded | gap (PR open) | REPORT | #206 → [#214](https://github.com/deslicer/cli/pull/214) |
| 3 | usage → 2 | missing arg / unknown cmd → 2; some invalid values → 1 | partial → landing | B1–B3 | #210 → [#218](https://github.com/deslicer/cli/pull/218) **merged** |
| 3 | auth → 4 | not logged in → 1; bad token → 4 | gap | B5 | #220 |
| 3 | 404 plan → 11 | approve on unknown plan → 15 (wrong) | fixed on main | REPORT | #208 → [#216](https://github.com/deslicer/cli/pull/216) **merged** |
| 3 | codes 14, 20, 21 published | not in docs/help | gap | B6 | #220 |
| 3 | published full table + exhaustiveness vs `CliError` | none in --help or docs (exists only in errors.rs) | gap | B6 | #220 |
| 4 | `schema` / `--help --json` | unknown subcommand; help wrapped as usage error | gap | C1, C2 | #222 |
| 4 | MCP not a CLI deliverable | n/a | contract clarified in v1.1 | – | – |
| 5 | non-interactive / device login | `auth login` no TTY fails fast; no `--no-input`; no JSON poll / `--no-wait` contract | partial | D1, D2 | #227 |
| 6 | dry-run + `--yes` (gated applies only) | present on credentials set, mappings set, adopt, approve, deploy | ok (keep scoped) | E.* | – |
| 6 | `change plan` creates without `--yes` | creates today; must stay create-on-invoke | ok / protect | – | – |
| 6 | idempotency keys (CI / explicit only) | none; duplicates detected by body text | gap | E9 | #228 |
| 7 | `plan_id` + `next_command` / `next_commands[]` | only `upgrade plan` has next_steps | partial | src/commands/upgrade/plan.rs | #225 |
| 8 | pagination / filtering | no `--limit`/`--cursor` on lists | gap | F.* | #224 |
| 8 | `change list` (read-only; exit 15 if missing) | missing | gap | F9 | #224 |
| 9 | timeout / retry | fixed 120 s; per-command `--timeout-secs` only | gap | G1, G2 | #226 |
| 9 | partial results (strict + `--allow-partial`) | `change status --jobs` exited 0 when load failed | fixed on main (strict exit); `--allow-partial` still gap | REPORT | #207 → [#215](https://github.com/deslicer/cli/pull/215) **merged** |
| 9 | whoami truthfulness | rejected token reported "Logged in" | gap (PR open) | REPORT | #209 → [#217](https://github.com/deslicer/cli/pull/217) |
| 10 | naming / bare groups | deprecated aliases hidden; bare-group exit 2 open | partial | H1, H2 | #187 → [#211](https://github.com/deslicer/cli/pull/211) open; #133 |
| – | Observer URL path prefix | – | fixed on main | – | #205 → [#212](https://github.com/deslicer/cli/pull/212) **merged** |
| – | release bump-PR reuse | – | fixed on main | – | #202 → [#204](https://github.com/deslicer/cli/pull/204) **merged** |

## Prioritised backlog (label `agent-native`)

**Sequencing (locked):**

1. Merge remaining fix PRs **#214, #217** (and **#211** if still open for #187). Already merged: #212, #215, #216, #218, #204. Land **#213** (or equivalent) as the **patch release** after those fixes.
2. Then agent-native P0s in order: **#219 → #221 → #220**.
3. Then P1 / P2 as below.

| Priority | Item | Notes |
|---|---|---|
| Gate | Merge #214, #217 [, #211]; patch release | Clears #206, #209 [, #187]; do not start agent-native P0 until patch is cut |
| P0 | #219 SIGPIPE / EPIPE panic | Most common agent pipe crash; still P0 even if one check dump said PASS |
| P0 | #221 error envelope v1 + JSON-when-not-TTY + `--output` | Includes last-line envelope rule; aliases; no CI-alone default flip |
| P0 | #220 exit-code contract | not-logged-in → 4; publish full table including 14, 20, 21; exhaustiveness test |
| P1 | #222 `deslicer schema` / `--help --json` | Contract surface; MCP adapters consume externally |
| P1 | #223 `schema_version: 1` one wave + snapshots | Success + error JSON; human unscathed |
| P1 | #225 `next_command` / `next_commands[]` everywhere | Singular primary; interpolate ids; no unguarded mutate |
| P1 | #224 `change list` + pagination | Read-only; auth like show; exit 15 if Observer route missing |
| P2 | #227 `--no-input` + device-login JSON poll / `--no-wait` | Never open browser; exit 0/4; `auth.pending` retryable |
| P2 | #226 `--timeout` and retries | Global flag + env; Retry-After |
| P2 | #228 idempotency keys | Default only with CI run id or explicit flag; no local input-hash replay |
| P2 | #187 / #133 naming | Bare groups, flag consistency; #211 may land in gate |

## Weekly check

`check.sh <bin> [outdir]` runs offline under `env -i` with an empty HOME. It prints `id | check | res | evidence` and exits 1 if anything fails. v1.7.3 result: **15 pass / 20 fail**.

**Note:** This dump is a historical snapshot. Prefer the gap table when it conflicts (e.g. A7 listed PASS here while SIGPIPE remains P0 #219).

```
id                     | check                                                | res  | evidence
A1                     | Error is JSON when stdout is not a TTY (no flags)    | FAIL | exit 1; not logged in. Run `deslicer auth login` and approve the code in the portal, or set OBSERV
A2                     | Global --output json accepted                        | FAIL | exit 2; error: unexpected argument '--output' found  Usage: deslicer [OPTIONS] <COMMAND>
A3.code                | Error envelope has error.code                        | FAIL | 
A3.message             | Error envelope has error.message                     | PASS | not logged in. Run `deslicer auth login` and approve the code in the p
A3.hint                | Error envelope has error.hint                        | PASS | run auth login and approve the code in the portal
A3.next_command        | Error envelope has error.next_command                | FAIL | 
A4                     | JSON error goes to stderr and stdout stays empty     | PASS | stdout=0B
A5                     | Output carries schema_version                        | FAIL | upgrade preview error JSON: none
A6                     | No ANSI colour when piped                            | PASS | help piped
A7                     | No panic on closed pipe (SIGPIPE)                    | PASS |  ← snapshot; still track as P0 #219 until verified
B1                     | Missing argument → 2                               | PASS | change approve → 2
B2                     | Unknown command → 2                                | PASS | → 2
B3                     | Invalid flag value → 2 (#210)                      | FAIL | --installation 0 → 1
B4                     | --json usage error is JSON (#210)                    | FAIL | exit 2
B5                     | Not logged in → 4                                  | FAIL | → 1
B6                     | Exit-code table in help or docs                      | FAIL | top-level --help
C1                     | 'deslicer schema' emits JSON                         | FAIL | exit 2
C2                     | --help --json gives a command tree, not an error     | FAIL | exit 0; envelope kind=usage
D1                     | auth login with no TTY fails fast (not hang)         | PASS | exit 1 in 0s
D2                     | Global --no-input / --non-interactive flag           | FAIL | top-level --help
E.acs_credentials_set  | 'acs credentials set' has --yes (dry-run default)    | PASS | help
E.environments_mappings_set | 'environments mappings set' has --yes (dry-run default) | PASS | help
E.inventory_apps_adopt | 'inventory apps adopt' has --yes (dry-run default)   | PASS | help
E.change_approve       | 'change approve' has --yes (dry-run default)         | PASS | help
E.change_deploy        | 'change deploy' has --yes (dry-run default)          | PASS | help
E9                     | change plan supports --idempotency-key               | FAIL | help
F.groups_list          | 'groups list' has --limit plus --cursor/--page       | FAIL | help
F.inventory_apps       | 'inventory apps' has --limit plus --cursor/--page    | FAIL | help
F.routine_runs         | 'routine runs' has --limit plus --cursor/--page      | FAIL | help
F.enroll_list          | 'enroll list' has --limit plus --cursor/--page       | FAIL | help
F9                     | 'change list' exists (find plan_ids)                 | FAIL | exit 2
G1                     | Global --timeout flag                                | FAIL | top-level --help
G2                     | Retry/backoff flag or env documented                 | FAIL | top-level --help
H1                     | Deprecation notice on stderr only                    | PASS | upgrade status
H2                     | Deprecated aliases hidden from help                  | PASS | upgrade --help
PASS=15 FAIL=20  bin=deslicer 1.7.3 (d338f72b00000d937b9c5498d1219404f0a9584f)
```

## Appendix A: Decision log

Locked grill outcomes for v1.1 (Daniel):

| # | Topic | Decision |
|---|---|---|
| 1 | §6 dry-run / `--yes` | Not universal. Dry-run default only on already-gated apply paths. Create-draft (`change plan`, etc.) creates on invoke without `--yes`. `requires_yes` only for true applies. Idempotency (#228) is replay, not forced `--yes`. |
| 2 | §1 JSON default | JSON iff stdout is not a TTY. Do not flip on `CI=true` alone. TTY stays human. Override: `--output` / `DESLICER_OUTPUT`. |
| 3 | §4 MCP | `deslicer schema` is the contract. MCP is an external consumer; not shipped in this CLI programme. Adapters may generate tools from schema. |
| 4 | §3 exits | Publish full table including **14**, **20**, **21**. Exhaustiveness test vs `CliError`. Do not renumber. |
| 5 | §7 next | Always set singular `next_command` when one primary exists; `next_commands[]` for alternatives; null/empty if none; interpolate real ids; never suggest unguarded mutate. |
| 6 | §9 partial | Strict default (non-zero on partial). `--allow-partial` → exit 0 with `partial: true`. No new exit code; single-part failure uses that part’s code else 1. |
| 7 | Aliases | Canonical `--output`/`-o` (+ `DESLICER_OUTPUT`). `--json` remains (hidden ok). `--log-format` deprecated two minors then hidden. Conflicts → exit 2. |
| 8 | `schema_version` | Additive `schema_version: 1` on all JSON success+error in one wave; snapshot tests; human unscathed. |
| 9 | `change list` | Read-only tenant list; auth like `change show`; `--limit`/`--cursor`/filters; missing Observer route → exit 15. |
| 10 | Idempotency | Default key only when CI run id present or `--idempotency-key` set; no silent local input-hash replay. |
| 11 | Sequencing | Merge fix PRs #212–#218 (#205–#210) [and #204/#211 if open] → patch release → then agent-native P0 **#219 → #221 → #220**. |
| 12 | Device login non-interactive | Print JSON codes; poll until success/timeout (0/4); `--no-wait` → exit 4 `auth.pending` retryable; never open browser. |
| 13 | Error envelope | JSON mode: last stderr line is exactly one envelope; logs may precede; nothing after. Human mode free-form. |
