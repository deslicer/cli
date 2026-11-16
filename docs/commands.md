# Command reference

Paths are arguments to the CLI binary (`<cli> <path>`). This page is generated from the clap tree (`<cli> schema`) so every command has a row. Prefer it over `--help` when an agent needs the full surface.

## Agent contract

- Success and errors are JSON when stdout is not a TTY. Force it with `--output json` (hidden alias `--json`). Human text is the TTY default.
- Exit codes are in [exit-codes.md](exit-codes.md) and in `<cli> schema`.
- Errors in JSON mode end with one stderr envelope: `code`, `hint`, `next_command`, `exit_code`.
- The CLI does not prompt. `--no-input` is implied when stdin is not a TTY or `CI=true`.
- Creating a plan (`change plan` and the other plan verbs) returns a plan id and does not approve it. Approve stays `change approve`. Deploy stays `change deploy`. Revert stays `change revert` (dry-run unless `--yes`).
- Apply commands marked **dry-run unless --yes** print the intended write and exit 0 until `--yes`.

## Names

Canonical nouns are singular. These hidden aliases still run and print a stderr warning for two minor versions:

| Alias | Canonical |
| --- | --- |
| `acs` | `cloud stack` |
| `models` | `model` |
| `environments` | `environment` |
| `groups` | `group` |
| `mappings` | `mapping` |

Splunk writes also live under `change`. The older path still works and warns:

| Older path | Canonical |
| --- | --- |
| `upgrade plan` | `change upgrade plan` |
| `catalog plan` | `change catalog plan` |
| `remediate draft` | `change remediate draft` |
| `inventory apps adopt` | `change app adopt` |
| `environment mapping set` | `change mapping set` |
| `environment mapping adopt` | `change mapping adopt` |

`doctor` reports API URL drift and local workflow readiness. `doctor --installation <id>` runs the same live GitHub App checks as `repo bootstrap --check`. `repo doctor` is not a command.

## Commands that were easy to miss

- **Cloud:** `cloud stack list`, `cloud stack credential list|test|set`, `cloud stack observe`. `credential set` is dry-run unless `--yes` and never takes the token as an argument.
- **Routines:** `routine list|enable|disable|run|runs|report`.
- **Remediate:** `remediate analyse` is read-only. `remediate draft` / `change remediate draft` creates a Pending plan id.
- **Models and billing:** `model list`, `model byok status|set`, `billing usage`, `billing credits`.
- **Inventory apps:** `inventory apps` lists; `inventory apps adopt` / `change app adopt` writes.
- **Environment mappings:** `environment mapping list|set|adopt`. Writes are dry-run unless `--yes`.
- **Agent clone:** `agent clone` (dry-run unless `--yes`).
- **Registry plans:** `change plan --from-registry <app>@<version> --target-group <group>`.
- **Repo readiness:** `repo bootstrap --check --installation <id>` (optional `--dir`, `--repo-id`). Create stays dry-run unless `--yes`.
- **Org, skill, license, catalog, upgrade:** `org roles|teams|switch|settings`, `skill list|import`, `license status|bind`, `catalog list|show|plan`, `upgrade readiness|preview|plan`.

List commands accept `--filter` (case-insensitive substring) plus `--limit` / `--cursor` where the row says paginated. `catalog plan` and `upgrade plan` / `change upgrade plan` take repeatable `--target` (`--targets a,b` still works).

## Full tree

| Path | Summary | Notes |
| --- | --- | --- |
| `agent` | Talk to an agent (REPL by default; device session) |  |
| `agent list` | List the agents this session can run |  |
| `agent ls` | List recent runs started by this session |  |
| `agent clone` | Clone an agent (and optional --bulk ids); dry-run unless --yes | mutating, dry-run unless --yes |
| `agent run` | Run an agent and stream its answer | mutating |
| `agent logs` | Read the output of a run that is already going |  |
| `agent resume` | Continue this session's last conversation |  |
| `agent repl` | Start a conversation. `cli agent` with no subcommand does this |  |
| `app-state` | Fingerprint a Splunk app directory and manage immutable local objects | |
| `app-state fingerprint` | Hash an app directory | |
| `app-state publish` | Store immutable objects for an app directory | mutating, dry-run unless --yes |
| `app-state fetch` | Read an object by digest | writes a file with --yes and --output |
| `auth` | Sign in, check CI identity, and manage stored credentials |  |
| `auth login` | Exchange CI OIDC or start device login and store credentials for later commands | mutating |
| `auth logout` | Clear the stored device session (environment tokens are unchanged) | mutating |
| `auth status` | Print CI platform, OIDC availability, and environment binding diagnostics |  |
| `auth whoami` | Print the current CLI identity without dumping tokens |  |
| `change` | Create, inspect, approve, and deploy configuration change plans |  |
| `change plan` | Create a change plan from the current repo commit (or a local bundle) | mutating |
| `change list` | List plans (filters + pagination) so agents can rediscover plan ids | paginated |
| `change show` | Show one plan or list recent plans for an environment |  |
| `change approve` | Approve a plan pending human review (CI proxy attests the approver) | mutating |
| `change reject` | Reject a plan with an optional reason |  |
| `change deploy` | Queue and monitor rollout of an approved plan | mutating |
| `change revert` | Create a Pending inverse plan from a completed plan (never auto-approves) | mutating, dry-run unless --yes |
| `change jobs` | List per-job state and retry_count for a plan's latest execution |  |
| `change retry` | Retry failed plan jobs (dry-run unless --yes) | mutating, dry-run unless --yes |
| `change validate` | Run or inspect validation for an existing persisted plan |  |
| `change verify` | Re-run compile verification and refresh the dry-run diff for a plan |  |
| `change status` | Report plan lifecycle status and rollout progress (for CI wait steps) |  |
| `change upgrade` | Create a Pending rolling upgrade plan (never approves or deploys) |  |
| `change upgrade plan` | Create a Pending rolling upgrade plan (never approves or deploys) | mutating |
| `change catalog` | Create Pending plan(s) from a catalog blueprint (dry-run unless --yes) |  |
| `change catalog plan` | Create Pending plan(s) from a blueprint (dry-run unless --yes) | mutating, dry-run unless --yes |
| `change remediate` | Create a draft remediation plan (dry-run unless --yes) |  |
| `change remediate draft` | Create a draft remediation plan (dry-run unless --yes) | mutating, dry-run unless --yes |
| `change app` | Adopt a live app (dry-run unless --yes) |  |
| `change app adopt` | Adopt a live app as Git-managed (dry-run unless --yes) | mutating, dry-run unless --yes |
| `change mapping` | Write an environment app mapping (dry-run unless --yes) |  |
| `change mapping set` | Upsert one environment app mapping (dry-run unless --yes) | mutating, dry-run unless --yes |
| `change mapping adopt` | Adopt an app into the environment mapping (dry-run unless --yes) | mutating, dry-run unless --yes |
| `cloud` | Splunk Cloud stacks, credentials, and observe (read-only) |  |
| `cloud stack` | List, credential, or observe one Splunk Cloud stack |  |
| `cloud stack list` | List Splunk Cloud stacks that have a stored credential |  |
| `cloud stack credential` | List, test, or set the credential for one stack |  |
| `cloud stack credential list` | List ACS stack credentials (metadata only; tokens are never shown) |  |
| `cloud stack credential test` | Test that the stored credential for a stack can reach ACS (read-only observe) |  |
| `cloud stack credential set` | Store or rotate the ACS JWT for a stack (dry-run unless --yes) | mutating, dry-run unless --yes |
| `cloud stack observe` | Observe a stack (read-only scan; never applies) |  |
| `routine` | DAI routines (list, enable/disable, run, runs, report) |  |
| `routine list` | List DAI routines for this tenant |  |
| `routine enable` | Enable a routine (dry-run unless --yes) | mutating, dry-run unless --yes |
| `routine disable` | Disable a routine (dry-run unless --yes) | mutating, dry-run unless --yes |
| `routine run` | Trigger a routine run (dry-run unless --yes); optional --model override | mutating, dry-run unless --yes |
| `routine runs` | List recent runs for a routine | paginated |
| `routine report` | Show a routine run report |  |
| `skill` | Skills marketplace (list, import) |  |
| `skill list` | List installed / marketplace skills (size, rename, scan results) |  |
| `skill import` | Import a skill from a path or marketplace id (dry-run unless --yes) | mutating, dry-run unless --yes |
| `catalog` | Catalog blueprints (list, show, multi-target plan) |  |
| `catalog list` | List catalog blueprints |  |
| `catalog show` | Show one catalog blueprint |  |
| `catalog plan` | Create Pending plan(s) from a blueprint for --targets (dry-run unless --yes) | mutating, dry-run unless --yes |
| `remediate` | Remediate analyse / draft (draft plan id) |  |
| `remediate analyse` | Analyse remediation candidates (read-only) |  |
| `remediate draft` | Create a draft remediation plan id (dry-run unless --yes) | mutating, dry-run unless --yes |
| `license` | Tenant license status and bind |  |
| `license status` | Show tenant license status |  |
| `license bind` | Bind a license key (dry-run unless --yes) | mutating, dry-run unless --yes |
| `model` | Model catalogue and BYOK |  |
| `model list` | List available models for this tenant |  |
| `model byok` | Bring-your-own-key status / set |  |
| `model byok status` | Show BYOK configuration status |  |
| `model byok set` | Set a BYOK provider key (dry-run unless --yes) | mutating, dry-run unless --yes |
| `billing` | Billing usage and credits (read-only) |  |
| `billing usage` | Show usage for the current billing period |  |
| `billing credits` | Show credit balance |  |
| `org` | Org roles, teams, switch, and plan settings |  |
| `org roles` | List roles for the current org/tenant |  |
| `org teams` | List teams for the current org/tenant |  |
| `org switch` | Switch the active tenant/org (dry-run unless --yes) | mutating, dry-run unless --yes |
| `org settings` | Plan approval / execution settings |  |
| `org settings approval` | Show or update plan approval settings | mutating, dry-run unless --yes |
| `org settings execution` | Show or update plan execution settings | mutating, dry-run unless --yes |
| `group` | List host groups (`id` or exact `name` for `change plan --target-group`) |  |
| `group list` | List host groups (`id` or exact `name` for `change plan --target-group`) | paginated |
| `inventory` | List Ansible inventory groups, sync, or validate tenant environment YAML |  |
| `inventory list` | List Ansible inventory groups and their hosts |  |
| `inventory apps` | List discovered apps (`--host` / `--group`); `apps adopt` to adopt | paginated |
| `inventory apps adopt` | Adopt a live app as Git-managed for one host (dry-run unless --yes) | mutating, dry-run unless --yes |
| `inventory sync` | Refresh `.cli/environments/<tenant-slug>.yml` from Observer host groups | mutating |
| `inventory validate` | Validate `.cli/environments/<stem>.yml` (shape + live host groups) |  |
| `inventory import-seed` | Declare hosts from a lab Ansible inventory seed YAML | mutating |
| `environment` | Build CI helpers from the tenant environments directory |  |
| `environment plan-matrix` | Build a CI matrix from environment destinations that contain apps |  |
| `environment mapping` | Environment app mappings (Observer source of truth) |  |
| `environment mapping list` | List environment app mappings (Observer source of truth) |  |
| `environment mapping set` | Set / upsert one app mapping (dry-run unless --yes) | mutating, dry-run unless --yes |
| `environment mapping adopt` | Adopt an app already on a host group into the mapping (dry-run unless --yes) | mutating, dry-run unless --yes |
| `init` | Write CI templates for a config repo (optional --bind) | mutating |
| `docs` | Print documentation topic URLs (`--open` for a browser) |  |
| `enroll` | Mint, list, or revoke enrollment tokens (device session) |  |
| `enroll create` | Mint a one-time enrollment token (shown once) | mutating |
| `enroll list` | List enrollment tokens (never reprints the secret) | paginated |
| `enroll revoke` | Revoke a token by UUID jti | mutating |
| `worker` | Print worker install recipes (device session) |  |
| `worker instructions` | Print a worker install recipe from the portal (no SSH) |  |
| `repo` | Provision or refresh a GitHub App config repo (device session) |  |
| `repo auth` | Show the supported GitHub authentication paths for repository bootstrap | |
| `repo bootstrap` | Create a private org repo via the GitHub App (dry-run unless --yes) | mutating, dry-run unless --yes |
| `repo refresh` | Open a workflow refresh pull request for one repo | mutating |
| `repo status` | List linked repos and bootstrap job fields |  |
| `completion` | Generate shell completions (bash, zsh, fish) |  |
| `update` | Update the cli binary to the latest release | mutating |
| `upgrade` | Splunk package upgrade intent (plan / flapping-gate status) |  |
| `upgrade readiness` | Read-only upgrade readiness for one host (path, pin, captain stability) |  |
| `upgrade preview` | Read-only rolling upgrade wave preview (no plan is created) |  |
| `upgrade plan` | Create a Pending rolling upgrade plan (never approves or deploys) | mutating |
| `doctor` | Readiness: API URL drift, local workflows, optional GitHub App check |  |
| `schema` | Emit machine-readable command / flag / exit-code schema (JSON) |  |
