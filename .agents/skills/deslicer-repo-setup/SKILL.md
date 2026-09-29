---
name: deslicer-repo-setup
description: Use when setting up a Deslicer configuration repository, initializing local Git, connecting an existing repo, scaffolding CI, configuring GitHub token or App authentication, binding environments, or provisioning a private GitHub organization repository with deslicer repo bootstrap.
---

# Deslicer repository setup

Keep three operations distinct: `git init` creates local Git metadata;
`deslicer init` writes configuration; `deslicer repo bootstrap` creates a remote
GitHub repository. None substitutes for the others.

## Preflight

1. Confirm the intended directory, existing/new repository, provider, environment,
   and authentication path. Read `deslicer --version` and relevant `--help`.
2. Inspect `git rev-parse --show-toplevel`, `git status --short`, and origin
   without exposing embedded credentials. Inspect workflows, environment files,
   and app mappings. A parent Git root is not permission to initialize inside it.
3. Preserve dirty changes. Do not reset, silently change origin, stage unrelated
   files, or use `--force`. For conflicting templates, scaffold into an existing
   temporary directory with explicit provider, then review and integrate.
4. Read [REFERENCE.md](REFERENCE.md) for the selected path before execution.
   Repository creation, binding, CI-secret writes, commits/pushes, and plan
   creation must stay within the user's requested scope. Never print credentials.

## Choose the path

| Situation | Action |
| --- | --- |
| Existing local Git repository | Reuse it; no `git init` |
| Existing remote, no checkout | Clone the verified URL |
| New local repository only | In the confirmed directory, `git init -b main`; add only an approved remote |
| New GitHub App org repository | Device login; preview `repo bootstrap`, then `--yes` if authorized; clone returned URL |
| GitHub with Observer tools token | Explicit `--provider github-token`; no `--bind` |
| GitHub App / OIDC | Device login; `--provider github`; optional authorized `--bind` |
| GitLab, Azure, Bitbucket | Use the explicit provider and limitations in the reference |

## Example: existing token-path repo

With Observer credentials securely configured and no conflicting templates:

```bash
deslicer auth whoami
deslicer groups list
deslicer init --provider github-token --environment acme-prod
deslicer inventory validate --environment acme-prod
git diff --check
git status --short
```

Before validation, review generated files and map app paths to
inventory groups. Separately configure the printed GitHub
Environment recipe when authorized; it is not executed by init.

## Completion and traps

- `auto` selects GitHub App/OIDC for GitHub origins, **not** token mode.
- `--bind` needs online device auth, an origin, environment, and target-group
  **UUID**. Do not combine with `--offline`; failed init can leave written files.
- `--force` can overwrite templates including README files. Never use it merely
  to bypass a conflict; existing app mappings still need review.
- Offline init needs a previously fetched provider cache and skips live tenant
  inventory generation. It is scaffolding, not completed onboarding.
- Report separately: local Git, remote, scaffolding, app validation, binding,
  CI credentials, committed/pushed state, and first-plan evidence. Exit zero
  does not prove these all succeeded.
- Stop before approval/deployment. For a requested first plan, hand off to
  `deslicer-cli-propose-inspect` if available; setup alone does not authorize it.
