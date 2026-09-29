---
name: deslicer-repo-setup
description: Use when setting up a Deslicer configuration repository end to end, including missing login or credentials, local Git initialization, GitHub authentication, private repo creation, CI secrets, App bindings, first push, or onboarding verification.
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
4. Read [AUTHENTICATION.md](AUTHENTICATION.md), then [REFERENCE.md](REFERENCE.md)
   for the selected path, and [COMPLETION.md](COMPLETION.md) before publishing.
   Repository creation, binding, CI-secret writes, commits/pushes, and plan
   creation must stay within the user's requested scope. Never print credentials.

## Choose the path

| Situation | Action |
| --- | --- |
| Existing local Git repository | Reuse it; no `git init` |
| Existing remote, no checkout | Clone the verified URL |
| New local repository only | In the confirmed directory, `git init -b main`; add only an approved remote |
| New GitHub App org repository | Device login; preview `repo bootstrap`, then `--yes` if authorized; clone returned URL |
| GitHub with Observer tools token | GitHub login/create via `gh`; explicit `--provider github-token`; no `--bind` |
| GitHub App / OIDC | Device login; `--provider github`; optional authorized `--bind` |
| GitLab, Azure, Bitbucket | Use the explicit provider and limitations in the reference |

## End-to-end execution

Own setup through verification, not just a printed recipe. Check/install missing
tools, authenticate Deslicer and the Git host separately, verify live access,
create/clone/init the chosen repo, configure local Git identity, scaffold and
map apps, configure CI credentials/bindings, then review, commit, and push when
requested. Inspect workflow triggers before pushing: they may deploy.

Execute in-scope steps. Pause only for consent, credentials, administrator
access, or target choices; resume afterward. Never request passwords/tokens
in chat or approve device consent for the user.

For example, token setup uses `deslicer init --provider github-token
--environment acme-prod` after authentication, then app mapping and
`deslicer inventory validate --environment acme-prod` before publishing.

## Completion and traps

- `auto` selects GitHub App/OIDC for GitHub origins, **not** token mode.
- `--bind` needs online device auth, an origin, environment, and target-group
  **UUID**. Do not combine with `--offline`; failed init can leave written files.
- `--force` overwrites templates; offline scaffolding is not completed onboarding.
- Report each checkpoint with evidence. Exit zero alone is not setup verification.
- Stop before approval/deployment. For a requested first plan, hand off to
  `deslicer-cli-propose-inspect` if available.
