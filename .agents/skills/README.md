# Repository skills

## Deslicer CLI propose and inspect

[deslicer-cli-propose-inspect](deslicer-cli-propose-inspect/SKILL.md) guides agents through creating and inspecting Splunk change proposals, followed by human approval and execution in the portal.

Agents that support repository-local `.agents/skills/` discovery can load it here. Otherwise, explicitly point the agent to its SKILL.md or copy the complete `deslicer-cli-propose-inspect/` directory into the agent's configured skill directory. Keep all four files together so the reference links work.

Example request:

> Use deslicer-cli-propose-inspect to inspect my existing plan. Credentials are already configured out of band. Report its lifecycle state and validation findings, then hand it back for human review.

Workshop settings are optional and apply only when explicitly selected. Review the skill's SOURCES.md when updating it for a new CLI release.

The [evaluation report](../../docs/skills/deslicer-cli-propose-inspect-evaluation.md) records the source review and simulated behavior tests. These tests do not establish live Observer or portal behavior.
