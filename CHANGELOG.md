# Changelog

User-facing notes for the CLI. Older releases are on the
[GitHub Releases](https://github.com/deslicer/cli/releases) page. <!-- pragma: allowlist secret -->
The published v1.7.1 notes are generated from merged pull requests when the
tag is pushed; this file is the prepared copy.

## 1.7.1 — Unreleased

Patch release. Compare with
[v1.7.0...main](https://github.com/deslicer/cli/compare/v1.7.0...main). <!-- pragma: allowlist secret -->

### Fixes

- Creating a change plan on a git-backed tenant reports the git-backed mode
  conflict and points at GitHub App or OIDC setup, instead of an ambiguous
  binding error. Path A2 failures no longer suggest `--source-dir`. Bundle
  upload is preflighted against change-management settings, so a git-backed
  tenant fails before the upload.
  ([#135](https://github.com/deslicer/cli/pull/135), <!-- pragma: allowlist secret -->
  fixes [#134](https://github.com/deslicer/cli/issues/134)) <!-- pragma: allowlist secret -->
- `deslicer update` human output and the OIDC troubleshooting docs show the <!-- pragma: allowlist secret -->
  CLI name again. They had been printing a literal scrub placeholder.
  ([#127](https://github.com/deslicer/cli/pull/127), <!-- pragma: allowlist secret -->
  fixes [#126](https://github.com/deslicer/cli/issues/126)) <!-- pragma: allowlist secret -->
- `deslicer change status` returns immediately when a plan is already failed, <!-- pragma: allowlist secret -->
  compile-failed, rejected, or expired, and exits 14. `--timeout-secs`
  defaults to 60 seconds, with a clearer message while a rollout is still
  running. `auth` and `change` help now includes command descriptions, flag
  help, and workflow examples.
  ([#124](https://github.com/deslicer/cli/pull/124), <!-- pragma: allowlist secret -->
  fixes [#122](https://github.com/deslicer/cli/issues/122) and <!-- pragma: allowlist secret -->
  [#123](https://github.com/deslicer/cli/issues/123)) <!-- pragma: allowlist secret -->
- `deslicer change show` renders a human-readable plan. <!-- pragma: allowlist secret -->
  ([#118](https://github.com/deslicer/cli/pull/118)) <!-- pragma: allowlist secret -->

### Documentation

- Agent skills default to authentication path A2 and describe the GitHub OIDC
  handoff.
  ([#119](https://github.com/deslicer/cli/pull/119)) <!-- pragma: allowlist secret -->
- Repository setup skill, plus end-to-end authentication and onboarding docs.
  ([#118](https://github.com/deslicer/cli/pull/118)) <!-- pragma: allowlist secret -->
- Propose-and-inspect skill for creating or inspecting a change plan.
  ([#117](https://github.com/deslicer/cli/pull/117)) <!-- pragma: allowlist secret -->

### Maintenance

- TruffleHog GitHub Action bump.
  ([#116](https://github.com/deslicer/cli/pull/116)) <!-- pragma: allowlist secret -->
- `thiserror` 2.0.21 and `rand` 0.10.3.
  ([#115](https://github.com/deslicer/cli/pull/115)) <!-- pragma: allowlist secret -->
