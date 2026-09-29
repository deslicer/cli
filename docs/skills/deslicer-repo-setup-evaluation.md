# Repository setup skill evaluation

## End-to-end authentication expansion

Before this revision, a fresh agent using the original skill reported:
“the skill supports scaffolding an already authenticated repository, but does
not yet support complete fresh-laptop onboarding for this scenario.” It could
not retrieve custom-portal selection, auth precedence/status semantics, GitHub
login/helper setup, author identity, no-App repo creation, exact CI writes, or
first-run verification. That is the failing completeness baseline.

Added AUTHENTICATION.md and COMPLETION.md, with source-checked Deslicer auth
semantics and installed `gh` help checks. A fresh skill-only agent covered:

- Fresh laptop and custom portal, separate GitHub/Git and Observer credentials.
- Normal private GitHub repo creation without a Deslicer App installation.
- Stale direct-Observer overrides taking precedence over device login.
- Configured token status versus a live successful Observer read.
- CI configuration and exact-commit verification without triggering deployment.

The evaluator identified ambiguous persistence of the custom portal selector.
The example now exports `DESLICER_API_URL` and explicitly requires retaining it
for subsequent portal commands and CI configuration. No actual login, credential
write, repository creation, or CI deployment was performed for these scenarios.
The evaluator also flagged cross-terminal environment inheritance; authentication
guidance now requires the same trusted shell or an approved process-accessible
secret mechanism, never transferring the token through chat.

Scope: `.agents/skills/deslicer-repo-setup`. Instruction-only scenarios; no
customer repositories, GitHub secrets, bindings, or deployments were mutated.

## Baseline before authoring

Two fresh-agent evaluations distinguished repository knowledge from packaged
reference availability:

- With CLI source available, the agent correctly handled dirty-repo preservation,
  explicit token provider, bootstrap preview, UUID binding, and partial writes.
  This baseline passed; it did not establish a behavior failure.
- With no source or skill (the installed-skill use case), the agent could only
  initialize Git. It reported: “I can’t give the exact Deslicer initialization
  command or provider choice from the supplied information.” It could not supply
  CI scopes, flag compatibility, or installation-based provisioning. This failed
  the reference-retrieval objective, while correctly avoiding invented commands.

The skill addresses that observed knowledge gap with a self-contained command
reference, not claims that generic safety guidance was previously absent.

## With the skill

A new agent read only SKILL.md and REFERENCE.md and answered four scenarios:

| Scenario | Required outcome | Result |
| --- | --- | --- |
| New local Git, Observer token, no App | `git init -b main`; explicit `github-token`; exact CI scopes; init does not create origin | Pass |
| Dirty existing GitHub repo | Preserve changes and app mappings; stage generated files separately on conflict; no blind force | Pass |
| Private org repo, installation 123 | Device auth, preview, authorized `--yes`, clone; no competing Git history | Pass |
| Hurry: offline + bind + group name + force | Reject combination; UUID required; partial-write and exit-zero caveats; no deployment claim | Pass |

The evaluator identified one useful clarification: private cloning needs Git
transport authentication separate from Deslicer device login. The reference now
states that explicitly and forbids tokens embedded in clone URLs.
The follow-up scenario (bootstrap succeeds but private clone is denied) passed:
the agent rejected an Observer token in the URL and directed verification of
GitHub account access plus SSH or an approved Git credential helper.

Structural validation with `quick_validate.py` passed; the entrypoint remains
under 100 lines and 500 words. An independent source review found no blocking
contract mismatches.

## Verification boundary

These are agent instruction/retrieval evaluations and source-contract checks,
not live onboarding proof. Live remote provisioning, secret installation,
binding, CI execution, and first-plan creation remain operator-scoped actions.
