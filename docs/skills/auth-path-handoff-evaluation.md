# A2 default and App/OIDC handoff evaluation

Before edits, a fresh agent found no explicit default in the setup skill and a
handoff gap: the proposal skill required an Observer token even for an existing
OIDC setup. The baseline therefore failed the requested path-selection policy.

After edits, a fresh skill-only evaluation passed these scenarios:

| Scenario | Expected behavior |
| --- | --- |
| New GitHub repo, unspecified auth, no tools key | Select A2; complete tools authentication, no automatic OIDC switch |
| Existing OIDC, inspect CI plan, no tools key | Preserve OIDC; use authorized device/CI inspection and matching plan ID |
| Explicit OIDC, device login, local git plan requested | Explain unsupported device git compile; reviewed real CI, no fake OIDC |
| A2 private clone failure | Diagnose separate Git credentials; no silent auth migration |
| Direct env credentials plus device session | Recognize direct precedence; scope App commands without overrides |

The evaluator flagged unconditional “Observer URL” wording in proposal scope.
It now requires a direct Observer URL only for A2 and resolved backend identity
for App/OIDC. Matching CI plans are reused, and bundle upload remains a separate
source choice. Approval/deployment boundaries are unchanged.

Both skill structural validations and `git diff --check` passed. Auth behavior
was checked against `src/auth_resolution.rs`, `src/commands/pipeline.rs`, and
`src/commands/change/plan.rs`. These are source and simulated instruction tests,
not live onboarding, OIDC, plan-creation, or deployment verification.
