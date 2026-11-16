# GitHub bootstrap auth

How the CLI creates or links a customer repository, and how the first push
and workflow run are authenticated. This is the decision for the open
question of a personal token versus a GitHub App.

## Decision

Use both, for different jobs. Do not add a third path.

| Path | Who authenticates to GitHub | What the CLI does |
| --- | --- | --- |
| Lab / personal | The operator, with their own GitHub token (`gh` or git). The CLI never reads or stores that token. | Prints the contract (`repo auth`). Does not push and does not dispatch a workflow. |
| Production | The GitHub App installation already connected in the portal. Observer holds the App credential. The CLI never receives the private key. | `repo bootstrap` creates a private repo (dry-run unless `--yes`). `repo refresh` opens a workflow-refresh pull request. `repo status` lists linked repos. |

## Secrets

The CLI does not write GitHub Environment secrets and does not push git
content. A person sets these in the GitHub Environment before a workflow can
run:

- API URL for the portal
- tools-scope API token, or OIDC (`id-token: write`) on the Path A workflow

`repo bootstrap --check --installation <id>` reports whether the installation
and linked repos look ready. The `secrets_and_push` check is intentionally
manual.

## What is not this issue

- Workflow reconcile (manifest to the reconcile API) is DAP work.
- Short-lived per-repo compiler tokens are DAP work, scoped to pull and diff,
  not this bootstrap path.

## Command

```bash
repo auth
repo auth --output json
```

JSON includes `cli_writes_secrets: false` and `cli_pushes_git: false`.
