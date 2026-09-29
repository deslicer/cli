# Repository skills

## Use the two skills together

Start with [deslicer-repo-setup](deslicer-repo-setup/SKILL.md) for authentication,
Git, CI configuration, and publishing; continue with
[deslicer-cli-propose-inspect](deslicer-cli-propose-inspect/SKILL.md) for a
requested plan and its evidence. Both stop before approval/deployment.

**A2 is the default for new GitHub setups unless another path is requested.**
Existing App/OIDC setups are preserved, not automatically migrated.

| | A2 — default | GitHub App/OIDC — opt-in or existing |
| --- | --- | --- |
| CI identity | Observer tools token | Short-lived GitHub OIDC |
| Init | `--provider github-token` | `--provider github --bind` with device login |
| Deslicer GitHub App | Not required | Connected installation required |
| Git-sourced proposal | Token CLI or configured CI | Real configured OIDC CI, not local device login |

GitHub/Git credentials are separate in both paths. `--provider auto` still selects
App/OIDC for GitHub remotes, so the skills use explicit `github-token` for A2.
Bundle upload (`--source-dir`) is a source choice, not another authentication mode.

Example end-to-end request:

> Use deslicer-repo-setup with the default A2 path, then use
> deslicer-cli-propose-inspect to create or inspect the first plan for my
> confirmed environment and group. Reuse a matching CI-created plan. Do not
> approve or deploy.

For OIDC, explicitly request “preserve/use GitHub App/OIDC.” The handoff includes
path, portal/backend, environment/group, source commit, and plan/run IDs—never
credentials. Copy each complete skill directory when installing separately.

## Deslicer CLI propose and inspect

[deslicer-cli-propose-inspect](deslicer-cli-propose-inspect/SKILL.md) guides agents through creating and inspecting Splunk change proposals, followed by human approval and execution in the portal.

Agents that support repository-local `.agents/skills/` discovery can load it here. Otherwise, explicitly point the agent to its SKILL.md or copy the complete `deslicer-cli-propose-inspect/` directory into the agent's configured skill directory. Keep all four files together so the reference links work.

Example request:

> Use deslicer-cli-propose-inspect to inspect my existing plan. Credentials are already configured out of band. Report its lifecycle state and validation findings, then hand it back for human review.

Workshop settings are optional and apply only when explicitly selected. Review the skill's SOURCES.md when updating it for a new CLI release.

The [evaluation report](../../docs/skills/deslicer-cli-propose-inspect-evaluation.md) records the source review and simulated behavior tests. These tests do not establish live Observer or portal behavior.
