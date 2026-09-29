# Deslicer CLI GitHub token-path initialization validation

Status: Proposed

Audience: Deslicer CLI maintainers, DAP/Observer maintainers, release engineering

Target: A backward-compatible minor CLI release after v1.7.0

Source baseline: `deslicer/cli` `main` at `cff1718b3080dfc5baa2115de1e7f4b64463f9b2`

## Summary

Make GitHub token-path repository initialization trustworthy and diagnosable.

The CLI will:

1. Print the complete GitHub configuration recipe, including repository variable
   `DESLICER_API_URL`.
2. Parse and semantically validate fetched GitHub workflow templates before any
   template is written or cached.
3. Test the identity contract for both GitHub providers:
   - `github-token` uses the Observer tools-token secret and variables and does
     not request OIDC.
   - `github` uses GitHub OIDC and does not use the Observer tools-token secret.
4. Add a read-only `deslicer repo doctor` command that checks local scaffold
   files and, when explicitly requested, the GitHub Environment, secret name,
   and exact nonsecret variable values.

The CLI will continue to print instructions rather than create GitHub
Environments or write secrets. Agents propose; people approve and configure
credentials through an explicitly authorized GitHub session.

## Problem

`deslicer init` currently establishes transport integrity for templates but not
their operational correctness. It validates provider, allowlisted path, decoded
size, and SHA-256 before writing. It does not parse GitHub workflow YAML or check
whether a workflow uses the correct authentication contract.

Consequently, `init` can exit successfully after receiving a template set that:

- is invalid YAML;
- references `OBSERVER_API_URL` as a secret instead of a variable;
- references `DESLICER_API_TOKEN` as a variable instead of a secret;
- requests `id-token: write` in the tools-token provider;
- omits the environment selector required to access an Environment-scoped secret;
- contains an unexpected approve, deploy, reject, or verify command; or
- pins an unintended CLI version.

There is also a recipe/documentation mismatch. The operator guide requires the
repository variable `DESLICER_API_URL`, but the recipe printed by
`github_environment_recipe` does not set it.

Finally, the CLI has no single read-only command that distinguishes:

- scaffolded files;
- valid local configuration;
- configured GitHub metadata; and
- runtime-valid credentials.

## Goals

- Fail `init` before template writes when a fetched or cached GitHub template set
  is syntactically invalid or violates its provider identity contract.
- Keep the tools-token and OIDC paths visibly distinct.
- Print the complete documented GitHub Environment/repository recipe.
- Give operators and automation one deterministic, read-only diagnosis command.
- Support human and JSON output with stable check identifiers.
- Never print, retrieve, compare, or persist a secret value.
- Preserve the portal as the approval and deployment trust boundary.

## Non-goals

- Creating or deleting a GitHub Environment.
- Setting, replacing, retrieving, or validating the value of a GitHub secret.
- Installing or binding the Deslicer GitHub App.
- Approving, rejecting, deploying, or verifying a change plan.
- Dispatching a workflow or proving that a tools token works at runtime.
- Making the entire existing `init` operation transactional. This proposal only
  guarantees validation before template writes and cache updates; later tenant
  inventory or binding operations retain their existing partial-write behavior.
- Validating arbitrary GitHub Actions semantics beyond the explicit contract in
  this document.

## Existing behavior and constraints

- `deslicer init --provider github-token --environment NAME` authenticates to
  Observer, fetches `github-token` bootstrap templates, writes those templates,
  writes/merges `.deslicer/environments/NAME.yml`, and prints a GitHub recipe.
- `--offline` loads the last cached template bundle and skips live tenant YAML
  generation.
- `github-token` permits `README.md`, `.github/workflows/`, and
  `.github/scripts/` template paths.
- `github` is the GitHub App/OIDC provider and may include `.deslicer/` paths.
- `auto` selects `github`, not `github-token`, for a GitHub origin.
- `deslicer repo` currently contains GitHub App provisioning operations. Doctor
  must bypass their device-session requirement because it is read-only and also
  serves the no-App token path.
- `serde_yml` is already a production dependency and should be used for YAML
  syntax validation. No new YAML dependency is required.

## User experience

### Initialize the token path

```bash
export OBSERVER_API_URL="https://dap-102.deslicer.show"
export DESLICER_API_TOKEN="..." # supplied securely; never logged

deslicer init \
  --provider github-token \
  --environment dap-102 \
  --dir "$PWD"
```

Before writing a fetched workflow, `init` validates the complete template set.
Success means the template set passed the syntax and provider-contract checks.
It does not mean GitHub metadata or credential validity has been verified.

The printed recipe must include all of the following:

```bash
gh api -X PUT "repos/$OWNER/$REPO/environments/dap-102"
printf '%s' "$DESLICER_API_TOKEN" | gh secret set DESLICER_API_TOKEN --env dap-102 --repo "$OWNER/$REPO"
printf '%s' "$OBSERVER_API_URL" | gh variable set OBSERVER_API_URL --env dap-102 --repo "$OWNER/$REPO"
printf '%s' "dap-102" | gh variable set DESLICER_ENVIRONMENT --env dap-102 --repo "$OWNER/$REPO"
printf '%s' "dap-102" | gh variable set DESLICER_ENVIRONMENT --repo "$OWNER/$REPO"
printf '%s' "$DESLICER_API_URL" | gh variable set DESLICER_API_URL --repo "$OWNER/$REPO"
```

The token remains stdin-only. The recipe must state that it is print-only and
that the CLI has not changed GitHub.

### Diagnose local state

```bash
deslicer repo doctor \
  --provider github-token \
  --environment dap-102 \
  --dir "$PWD" \
  --expected-cli-version 1.7.0 \
  --portal-only
```

Local diagnosis does not authenticate to Deslicer or GitHub and does not mutate
files. It checks:

- `.deslicer/environments/dap-102.yml` or `.yaml` exists;
- the environment document parses as YAML;
- it contains at least one destination with a nonempty `inventory_group`;
- it contains at least one app with a nonempty `source_path`;
- at least one GitHub workflow exists and every workflow parses as YAML;
- the aggregate workflow set satisfies the selected provider contract;
- an expected CLI version is pinned when the option is supplied;
- obvious Deslicer token/private-key patterns are absent from generated paths;
- lifecycle commands are reported and, with `--portal-only`, rejected.

### Diagnose GitHub configuration

```bash
deslicer repo doctor \
  --provider github-token \
  --environment dap-102 \
  --dir "$PWD" \
  --github \
  --repo deslicer/splunk_ug_london \
  --observer-api-url https://dap-102.deslicer.show \
  --deslicer-api-url https://ops.deslicer.show
```

`--github` is an explicit opt-in to read-only `gh` calls. It requires an
installed and authenticated GitHub CLI. If `--repo` is omitted, the command may
resolve `OWNER/REPO` from the verified `origin` URL; ambiguity is an error.

Live diagnosis checks:

- the repository resolves to the expected `OWNER/REPO`;
- GitHub Environment `dap-102` exists;
- Environment secret name `DESLICER_API_TOKEN` exists;
- Environment variable `OBSERVER_API_URL` exactly equals the expected URL;
- Environment variable `DESLICER_ENVIRONMENT` exactly equals `dap-102`;
- repository variable `DESLICER_ENVIRONMENT` exactly equals `dap-102`;
- repository variable `DESLICER_API_URL` exactly equals the expected portal URL.

Secret presence is not secret validity. The command must say so in both human
and JSON output.

## CLI contract

```text
deslicer repo doctor \
  --provider github|github-token \
  --environment NAME \
  [--dir PATH] \
  [--expected-cli-version VERSION] \
  [--portal-only] \
  [--github] \
  [--repo OWNER/REPO]
```

Rules:

- `--provider` and `--environment` are required. Explicitness prevents silently
  auditing the OIDC contract as though it were the token contract.
- `--dir` defaults to the current directory.
- `--repo` is valid only with `--github`.
- `--github` is initially supported only for `github-token`. For `github`, App
  binding diagnosis remains outside this proposal.
- Live expected URLs come from the existing global CLI flags/environment:
  `--observer-api-url`/`OBSERVER_API_URL` and
  `--deslicer-api-url`/`DESLICER_API_URL`.
- `--github` requires both expected URLs so an absent local environment variable
  cannot turn an exact comparison into an accidental presence-only check.
- `repo doctor` never uses the Deslicer authentication pipeline and never calls
  GitHub App provisioning routes.

Exit codes:

| Code | Meaning |
| --- | --- |
| `0` | Every required check passed; warnings may exist only when not promoted by an option |
| `1` | One or more deterministic configuration checks failed |
| `2` | Invalid arguments or unsupported option combination |
| `10` | An explicitly requested live GitHub check could not complete because `gh`, authentication, network, or GitHub API access was unavailable |

### Human output

Human output is concise and secret-free:

```text
Repository doctor: failed
PASS environment.file          .deslicer/environments/dap-102.yml
PASS workflow.yaml             1 workflow parsed
PASS workflow.identity         github-token contract
FAIL github.secret             DESLICER_API_TOKEN is missing from Environment dap-102
PASS github.repo-variable      DESLICER_API_URL=https://ops.deslicer.show

Summary: 4 passed, 1 failed, 0 warnings
Secret presence was checked; secret values and runtime validity were not checked.
```

### JSON output

The existing global `--log-format json` produces exactly one JSON object on
stdout and diagnostics only on stderr:

```json
{
  "status": "failed",
  "provider": "github-token",
  "environment": "dap-102",
  "repo": "deslicer/splunk_ug_london",
  "checks": [
    {
      "id": "github.secret",
      "status": "fail",
      "message": "DESLICER_API_TOKEN is missing from Environment dap-102"
    }
  ],
  "summary": {
    "passed": 4,
    "failed": 1,
    "warnings": 0
  },
  "limitations": [
    "Secret values and runtime credential validity were not checked."
  ]
}
```

Stable check IDs are part of the public automation contract:

- `environment.file`
- `environment.yaml`
- `environment.destinations`
- `workflow.present`
- `workflow.yaml`
- `workflow.identity`
- `workflow.cli-version`
- `workflow.lifecycle`
- `scaffold.credentials`
- `github.repository`
- `github.environment`
- `github.secret`
- `github.environment-variable.observer-api-url`
- `github.environment-variable.deslicer-environment`
- `github.repository-variable.deslicer-environment`
- `github.repository-variable.deslicer-api-url`

## Template validation contract

Validation runs in `decode_and_validate` after base64, path, size, tree SHA, and
per-file SHA checks, but before `write_offline_cache` and `write_templates`.
Fetched and offline-cached bundles use the same validator.

### Syntax rules

- Every `.yml` or `.yaml` file under `.github/workflows/` must decode as UTF-8.
- Every such file must parse through `serde_yml` as a mapping.
- A GitHub provider bundle must contain at least one workflow YAML file.
- Errors name the template path but never reproduce arbitrary template content.

### `github-token` identity rules

Across the complete workflow set:

- Require the literal GitHub expression
  `${{ secrets.DESLICER_API_TOKEN }}` at least once.
- Require `${{ vars.OBSERVER_API_URL }}` at least once.
- Require `${{ vars.DESLICER_ENVIRONMENT }}` at least once so jobs can select
  the matching GitHub Environment.
- Reject `${{ vars.DESLICER_API_TOKEN }}` anywhere.
- Reject `${{ secrets.OBSERVER_API_URL }}` anywhere.
- Reject effective workflow permission `id-token: write` at top level or job
  level. Comments and quoted run-script text do not count as permissions.

### `github` OIDC identity rules

Across the complete workflow set:

- Require effective `id-token: write` at top level or job level.
- Reject `${{ secrets.DESLICER_API_TOKEN }}` and
  `${{ vars.DESLICER_API_TOKEN }}` anywhere.
- Do not require `OBSERVER_API_URL`; OIDC/backend resolution follows the existing
  GitHub App binding path.

String-expression checks must traverse parsed YAML scalar values rather than
searching comments. Permission checks must inspect parsed `permissions` maps.

### Lifecycle policy

Template loading does not reject plan lifecycle commands because some supported
templates may intentionally expose manually gated actions. `repo doctor` reports
parsed/run-script occurrences of:

- `deslicer change approve`
- `deslicer change reject`
- `deslicer change deploy`
- `deslicer change verify`

These are warnings normally and failures with `--portal-only`. The check ignores
comments and searches scalar values under workflow `run` keys.

## Version-pin check

`--expected-cli-version X.Y.Z` accepts `X.Y.Z` or `vX.Y.Z` and normalizes to
`X.Y.Z`. The doctor passes when a workflow pins that version through either:

- `DESLICER_VERSION` with optional leading `v`; or
- an installer URL containing `/deslicer/cli/vX.Y.Z/`.

If no expected version is supplied, the check is skipped rather than guessing
the latest release or enforcing the version of the running binary.

## Credential-pattern check

Scan only the intended scaffold surface:

- `.github/`
- `.deslicer/`
- `README.md`

Fail on high-confidence patterns such as a Deslicer `dslk_` token with at least
20 following alphanumeric characters or a PEM/OpenSSH private-key header.
Report file paths, never the matching text. This is a guardrail, not a general
secret scanner.

## GitHub integration design

Use `tokio::process::Command` to call the installed `gh` binary. Do not accept a
GitHub token as a doctor argument and do not read GitHub credential files.

Allowed commands are read-only:

```text
gh auth status --hostname github.com
gh repo view OWNER/REPO --json nameWithOwner,isPrivate,viewerPermission
gh api repos/OWNER/REPO/environments/ENVIRONMENT --jq .name
gh secret list --env ENVIRONMENT --repo OWNER/REPO --json name
gh variable list --env ENVIRONMENT --repo OWNER/REPO --json name,value
gh variable list --repo OWNER/REPO --json name,value
```

The command must not invoke `gh api --method PUT`, `gh secret set`, or
`gh variable set`. Capture subprocess output, pass it through existing secret
redaction, and translate launch/auth/network failures to exit code 10.

Tests must inject a `GhClient` trait/fake rather than shadowing a real executable
on `PATH`. Production has one process-backed implementation; unit tests provide
deterministic responses and record attempted operations.

## Recipe change

Update `github_environment_recipe` to print:

```bash
printf '%s' "$DESLICER_API_URL" | gh variable set DESLICER_API_URL --repo OWNER/REPO
```

The explanatory text must distinguish:

- Environment variables used by jobs after selecting the tenant Environment;
- repository `DESLICER_ENVIRONMENT`, which selects that Environment for PR jobs;
- repository `DESLICER_API_URL`, which generates portal links.

The recipe must continue to use stdin and must never interpolate a token value.

## Implementation outline

Suggested file-level changes:

| File | Change |
| --- | --- |
| `src/commands/init/github_env_recipe.rs` | Add the repository portal variable and focused unit assertions |
| `src/commands/init/templates.rs` | Add pre-write syntax/provider semantic validation and structured helpers |
| `src/commands/repo/mod.rs` | Register `Doctor` without applying App-only session requirements |
| `src/commands/repo/doctor.rs` | Parse arguments, execute local/live checks, aggregate results, emit human/JSON output |
| `src/commands/repo/doctor/github.rs` | Read-only `GhClient` abstraction and process implementation, if separation improves testability |
| `src/commands/repo/doctor/workflows.rs` | Parsed workflow checks shared with init validation; avoid two competing rule implementations |
| `tests/e2e_init.rs` | Replace permissive fixtures with valid provider fixtures and add malformed/semantic rejection cases |
| `tests/e2e_repo.rs` | Add doctor output, exit-code, and no-mutation coverage |
| `docs/repo-init-and-enroll.md` | Document validation meaning, doctor commands, and runtime limitations |

Avoid duplicating policy between `init` and `repo doctor`. Put parsed workflow
validation and findings in a provider-neutral internal module, then configure
severity by caller:

- `init`: syntax and identity findings are fatal; lifecycle is not evaluated.
- `repo doctor`: syntax and identity are failures; lifecycle is warning/failure
  according to `--portal-only`.

## Test plan

Implementation follows red-green-refactor. Each production behavior begins with
a focused failing test whose failure is observed before code is added.

### Recipe tests

- Recipe includes repository `DESLICER_API_URL`.
- Recipe sends its value through stdin.
- Recipe still includes the Environment secret and both Environment variables.
- Invalid environment names are not interpolated.
- No token-looking literal appears in output.

### Template unit tests

- Valid `github-token` workflow passes.
- Invalid workflow YAML fails with its path.
- Non-mapping workflow YAML fails.
- Missing workflow file fails for GitHub providers.
- Token provider rejects OIDC permission.
- Token provider rejects token-as-variable.
- Token provider rejects Observer URL-as-secret.
- Token provider requires token, Observer URL, and environment expressions.
- OIDC provider requires `id-token: write`.
- OIDC provider rejects tools-token references.
- Comments containing forbidden strings do not trigger semantic failures.
- Parsed `run` values containing lifecycle commands produce doctor findings.

### Init end-to-end tests

- Update the existing token-path fixture to express the real A2 contract.
- A malformed server workflow causes exit 1 and writes no template files.
- A semantically invalid workflow causes exit 1 and writes no template files.
- Invalid fetched templates are not written to the offline cache.
- Valid online and offline template sets use identical validation.
- Existing overwrite and environment-merge behavior remains unchanged.

### Doctor tests

- Healthy local token-path scaffold returns 0.
- Missing/invalid environment YAML returns 1 with stable check IDs.
- Missing app mapping returns 1.
- Wrong provider contract returns 1.
- Lifecycle actions warn normally and fail under `--portal-only`.
- Expected CLI pin accepts both supported forms and rejects mismatches.
- Credential-pattern output reports only file paths.
- JSON is a single valid object with stable fields.
- Human output includes a summary and limitations.
- Live doctor detects a missing Environment, secret name, and each wrong variable.
- Live doctor never asks the fake `GhClient` to perform a write.
- Missing/unusable `gh`, auth failure, and GitHub transport failure return 10.
- `--repo` without `--github` and `--github` without expected URLs return 2.

### Regression suite

Required before merge:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Acceptance criteria

The work is complete only when all of the following are true:

1. The init recipe prints all six required GitHub configuration operations,
   including repository `DESLICER_API_URL`.
2. Invalid or provider-inconsistent GitHub workflows fail before templates are
   written or cached.
3. Existing valid GitHub OIDC and token-path initialization tests pass using
   semantically representative fixtures.
4. `deslicer repo doctor` performs no mutations in every code path.
5. Local doctor output differentiates syntax, identity, version, lifecycle, and
   credential-pattern findings.
6. Live doctor checks secret presence and exact nonsecret values without reading
   or implying knowledge of the secret value.
7. Human and JSON modes expose the documented status, check IDs, summary, and
   limitation statement.
8. Portal-only mode rejects lifecycle commands but never removes or rewrites
   workflows automatically.
9. Documentation states that successful scaffolding, configured GitHub metadata,
   and runtime-valid credentials are three separate states.
10. Formatting, Clippy, and the complete test suite pass in fresh CI.

## Compatibility and rollout

This is intentionally stricter behavior. A server-supplied GitHub template that
previously passed hash/path checks but violates its provider contract will now
cause `init` to fail. That is a correctness fix, not a silent compatibility mode.

Rollout order:

1. Update and validate Observer bootstrap templates against the new rules.
2. Add recipe and shared validator tests to the CLI.
3. Ship template validation and `repo doctor` in the same CLI release.
4. Update the repository-setup skill to call doctor after init and before push.
5. Monitor init failures by provider and check ID; do not log template bodies or
   credential values.

If Observer templates cannot be coordinated before release, gate only the new
fatal semantic validation behind a temporary environment-independent CLI feature
flag for one release. The recipe correction and doctor remain available. Do not
silently downgrade validation failures to success.

## Security and trust boundaries

- A tools token is distinct from a GitHub credential and from GitHub OIDC.
- The CLI never prints or retrieves the tools-token value.
- Doctor reads only names and nonsecret values from GitHub.
- `repo doctor --github` is explicit and read-only.
- Template error messages name paths and rule IDs, not arbitrary template text.
- Doctor does not approve or execute plans.
- A passing doctor result is configuration evidence, not human approval and not
  proof of a successful deployment.

## Definition of done evidence

The pull request implementing this specification should include:

- the failing-then-passing tests for each behavior group;
- example human and JSON doctor output with synthetic values;
- a statement that no GitHub mutation command is reachable from doctor;
- the exact Observer template revision validated against the new contract;
- local `fmt`, Clippy, and test results;
- remote CI results for the PR commit; and
- a note that human approval and merge remain outstanding until explicitly
  completed.
