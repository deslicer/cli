# Review: GitHub token-path initialization validation

Disposition: **Useful proposal; resolve the contract gaps below before implementation.**

Reviewed against CLI commit `efd556a7ff0604ab5efc221140bed440bc92ff17`.
The [original proposal](deslicer-cli-github-token-init-validation.md) is preserved
from `splunk_ug_london/docs/specs/deslicer-cli-github-token-init-validation.md`.
Adding it to this PR does **not** implement `repo doctor`, change template
validation, or authorize the deployment/rollout steps described in the proposal.

## Confirmed findings in current code

- `src/commands/init/github_env_recipe.rs` omits the repository
  `DESLICER_API_URL` operation required by the operator guide.
- `src/commands/init/templates.rs` checks provider, paths, byte limits, and
  per-file digests, but does not parse workflow YAML or validate identity use.
  The tree SHA is checked for hexadecimal format; it is not recomputed as a
  bundle digest. Avoid describing this as authenticated template provenance.
- Validation is already before online cache writes; offline decoding shares
  `decode_and_validate`. A shared workflow validator fits this boundary.
- `tests/e2e_init.rs` contains simplified workflow fixtures without a realistic
  complete identity contract. Updating them is necessary, not incidental.
- `src/commands/repo/mod.rs` has no doctor command. App-session requirements
  reside in individual subcommands, but a separate global guard also matters.

## Blocking contract gaps

### 1. Doctor cannot currently bypass the global token guard

`src/cli.rs:122` validates Observer token configuration before dispatch.
`src/observer_token.rs:14` rejects any Observer URL without a tools token.
Therefore the proposed `repo doctor --github --observer-api-url ...` cannot
remain independent of Deslicer credentials merely by bypassing repo session
checks.

Required design: route doctor through an explicitly credential-independent
configuration path, or exempt only doctor from this guard while keeping all
existing authenticated commands protected. Add `src/cli.rs` to implementation
scope. Test the real binary with an expected Observer URL, no tools/device
credentials, and both local and live-doctor modes. Fake GhClient unit tests
alone cannot catch this pre-dispatch failure.

### 2. Explicit expected portal cannot be inferred from Ctx

`src/cli.rs:18–24` supplies a default portal URL. `Ctx` holds the parsed URL but
not whether it came from a flag, environment, or default. Checking `Some(url)`
cannot enforce the proposal's requirement for explicitly supplied expectations.

Required design: preserve argument value-source information, or give doctor
optional `--expected-observer-url` and `--expected-portal-url` inputs. Define
which environment variables satisfy explicitness. Missing expectations must
fail with 2 before any GitHub call; never silently compare a custom deployment
against the default portal. Tests must distinguish flag, environment, and
implicit-default cases.

### 3. Aggregate expression presence is not an identity contract

A token reference in an unrelated scalar/job can satisfy the proposed checks
while the job running `deslicer change plan` has no `environment:` selector,
no token mapping, or no Observer URL. Merely finding
`${{ vars.DESLICER_ENVIRONMENT }}` does not prove Environment secret access.
Conversely, a valid matrix selector may not use that literal expression.

Required design: define the supported generated job shapes and validate each
Deslicer job's effective environment and credential mapping. Distinguish
literal expression formatting from meaning (whitespace, supported bracket
notation, inherited env, matrix selection). For unsupported indirection or
reusable workflows, report an explicit unsupported/unknown finding rather than
claiming validation. Define whether unrelated workflows receive syntax-only
checks or provider-policy checks. Do not let one healthy workflow hide another
broken plan job.

Test: token reference in a display-only field; environment variable mentioned
only in `run`; two jobs with only one correctly configured; matrix selectors;
workflow/job/step overrides; unrelated workflows.

### 4. Permission and version checks need effective job scope

Checking that *some* permission map grants OIDC can pass when the actual plan
job lacks it. Define permission inheritance, job overrides, and scalar forms
such as `write-all`/`read-all`/`{}` instead of examining maps only. A2 must not
accidentally permit OIDC through an unhandled permission form.

Likewise, a matching version anywhere can coexist with a different effective
version in the plan job. Specify precedence and reject conflicting pins for
relevant jobs; comments, echo text, and an unrelated URL are not installation
evidence. Test inherited/overridden permissions and mixed pins explicitly.

### 5. Portal-only detection is not a shell safety proof

Searching `run` scalars for four literal command strings misses commands in
`.github/scripts`, folded/split shell syntax, variable-selected subcommands,
and reusable actions. YAML parsing removes YAML comments, not shell comments
inside `run` strings; the stated comment exclusion needs a defined rule.

Required design: either limit support to a clearly defined generated script
grammar and inspect reachable bundled scripts, or explicitly describe
`--portal-only` as a best-effort detector of known direct calls. Unsupported
execution paths must not be reported as proven portal-only. The human review
gate in the skills must remain; a passing doctor is not permission to push a
workflow capable of deployment. Add direct, script-based, dynamic, and
comment-only lifecycle test cases.

## Additional requirements to settle

- **Bounded local reads:** define file count/size limits and deterministic
  ordering. Do not follow scaffold symlinks outside the selected root, read
  device files, execute scripts, or traverse arbitrary trees. Report skipped
  or unreadable files explicitly. A credential scan necessarily reads local
  bytes; clarify that the prohibition on retrieving secret values concerns
  credential stores/GitHub, while matching local bytes are never emitted.
- **GitHub errors and execution:** use argv, not a shell; validate owner/repo
  and environment, encode API path segments, and pin the intended GitHub host.
  Define timeout/output caps, pagination, redacted diagnostics, and ambiguity
  between a missing object and an inaccessible object. Exit 10 means checks
  could not complete; do not turn permission errors into “secret missing.”
- **Output schema:** the example JSON contains one check but summary counts
  five. Make examples internally consistent; define unique IDs or a separate
  subject/path field for multiple files, skipped/unknown outcomes, stable
  ordering, and exit precedence when local failures and live errors coexist.
  State whether parse/usage errors use the same JSON envelope.
- **Recipe safety:** require a nonempty verified portal value, avoid resetting
  existing Environment protection rules, and warn before replacing secrets.
  Print-only behavior remains correct; an authorized setup agent can execute
  reviewed configuration separately, as the existing setup skill describes.
- **Rollout:** stricter template acceptance is behavior-changing even if the
  CLI flags are additive. Record actual Observer template revisions/fixtures
  for both providers before enabling enforcement. The suggested feature-flag
  fallback needs a named switch, default, removal criterion, and clear skipped
  findings if adopted; it is not yet an agreed contract.

## Fit with the two skills

A2 remains the default for new unspecified GitHub onboarding; existing or
explicit App/OIDC is preserved. Requiring an explicit doctor provider is
compatible: the setup skill should pass its already selected provider.

Keep doctor examples in this proposed spec until implemented/released. Only
then add a capability-checked doctor step to setup; older CLIs retain current
manual checks. Doctor is configuration evidence, not a substitute for live
groups access, CI identity verification, plan evidence, or human approval.

## Review verification boundary

This review inspected implementation and test fixtures; no doctor runtime
exists to test. The source proposal is preserved with only its introductory
Markdown line breaks normalized; this companion review is linked separately.
No source-repository files, credentials, GitHub
Environments, or deployment state were modified by this review.
