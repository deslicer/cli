# Repo init and enroll

`deslicer init` writes CI templates that Observer pins and serves
(`GET /api/v1/bootstrap-templates`). The CLI does not embed workflow YAML.

```text
deslicer init [--provider github|github-token|gitlab|bitbucket|azure|auto]
              [--environment NAME] [--target-group UUID] [--dir PATH]
              [--bind] [--offline] [--force]
```

`--provider auto` reads `git remote get-url origin`. A `github.com` remote
selects the **OIDC/App** provider (`github`). Path A2 (Observer API token,
no GitHub App) is always explicit:

```bash
deslicer init --provider github-token --force
# then set GH secrets/vars and commit
# later: deslicer docs path-a2
```

| Secret / variable | Path A2 |
| --- | --- |
| `DESLICER_API_TOKEN` (secret) | tools-scope Observer key |
| `OBSERVER_API_URL` (var or secret) | Observer management URL |
| `TARGET_GROUP_ID` (var) | Host group UUID |
| `DESLICER_API_URL` (var) | Portal base for plan links |

Unknown hosts must pass one of the named providers. `--dir` defaults to the
current directory. Existing workflow files are not overwritten unless
`--force`.

Writing files is not Path A OIDC. `--bind` is opt-in for GitHub App / GitLab
environment bindings and uses a device session (`deslicer auth login`).
Path A2 does not use `--bind`; without `--bind` the command prints the next
step and exits 0.

Azure DevOps and Bitbucket stay bundle-only this release
(`deslicer change plan --source-dir`) — that is Path B, not Path A2.

## Enrollment

```text
deslicer enroll create --purpose insights|bootstrap [--name TEXT]
                       [--max-hosts N] [--expires-days N]
                       [--bind-host UUID] [--write-file PATH]
deslicer enroll list
deslicer enroll revoke --jti UUID
```

Create prints the one-time token once. `list` never reprints it. When stdout
is not a terminal, `--write-file` is required (mode `0600`, refuse overwrite).
Bootstrap tokens need the tenant worker plane enabled; the CLI does not mint
insights as a fallback. After create, run `deslicer worker instructions`.
Pending Approvals in the portal remain the trust gate.

## Worker instructions

```text
deslicer worker instructions [--format shell|ansible|manual]
                             [--product splunk-enterprise|splunkforwarder|otel]
                             [--channel prod|staging|development]
                             [--token-file PATH] [--embed-token]
```

Recipes come from the portal. The default snippet prompts for the token and
passes it on stdin (`--token-stdin`). `--embed-token` requires a TTY and
`--token-file`. The CLI does not SSH to hosts.

## GitHub App remote (`deslicer repo`)

```text
deslicer repo bootstrap --installation ID --name REPO [--description TEXT] [--yes]
deslicer repo refresh   --installation ID --repo-id ID
deslicer repo status    --installation ID
```

These commands wrap the existing GitHub App routes. They require
`deslicer auth login`. Without `--yes`, `repo bootstrap` prints the
organization, name, and `private` visibility and does not create the
repository. GitLab, Azure DevOps, and Bitbucket never call provision;
use `deslicer init --provider` for those hosts.

Canonical contract: DAP `docs/components/dap/cli-repo-init-and-enroll-spec.md`.
