# Repository evidence and maintenance

Authentication-path follow-up checked against main after PRs #117/#118 merged:
`src/auth_resolution.rs` and `src/commands/pipeline.rs` support direct tools,
device, and real CI identities. `src/commands/change/plan.rs` rejects device
sessions for git-sourced plans; bundle source is separate. `change show` now
supports human display; explicit JSON behavior remains the parsing contract.
This is source validation, not runtime proof of an OIDC deployment.

Reviewed 2026-09-29 against `deslicer/cli` origin/main at `c95a2671d1b2ad1e203325586d75f2aeae8b61ff` (v1.7.0). Installed binary help was checked on v1.6.0 (`840382ed352891e77bfd0bda34c7a4343a374eba`). This is source/help validation, not a live Observer or portal test. Version-sensitive details live in the reference rather than a permanent global pin.

All paths below are relative to the [reviewed source tree](https://github.com/deslicer/cli/tree/c95a2671d1b2ad1e203325586d75f2aeae8b61ff).

| Claim | Source |
|---|---|
| Plan flags, source flows, compile polling, no-wait | `src/commands/change/plan.rs` |
| Group UUID/exact name resolution | `src/target_group.rs`, `src/commands/groups/list.rs` |
| Bundle inclusion, symlinks, 32 MiB cap | `src/bundle.rs` |
| Clone token environment handling | `src/clone_token.rs` |
| Direct whoami is local configuration reporting | `src/commands/auth/whoami.rs` |
| External ID fallback and reduced plan model | `src/observer_client/types.rs` |
| Multiple stdout records and CI sinks | `src/output.rs`, `src/commands/change/show.rs` |
| Progress versus lifecycle | `src/commands/change/status.rs` |
| Report-only, formats, verdict exit codes | `src/commands/change/validate.rs` |
| Verify triggers compilation | `src/commands/change/verify.rs` |
| Install and pre-v1.6.0 redirect recovery | `docs/installation.md` |

The input `deslicer-cli-usage-only.md` supplies propose/inspect intent and workshop settings. Its `/workspace/...` references were unavailable and are not dependencies of this package. The CLI repo does not prove portal labels, MFA policy, route validity in every deployment, or a universal UI hang.

Authoring used `write-a-skill` and `writing-skills`: concise entry point, descriptive kebab-case name, only standard frontmatter fields, one-level reference links, repository checks, and baseline/revised behavior evaluations. Their description advice differs; this package uses the more specific trigger-only `Use when...` rule from `writing-skills`, keeping workflow detail in the body.

On CLI upgrades, compare installed help and the matching source for all allowed commands, output shape, bundle inclusion, auth behavior, and exit codes. Re-run the scenarios in the accompanying evaluation report. Update only claims supported by new evidence. Keep this skill scoped to propose/inspect; expanding it to deployment requires a separate workflow design.
