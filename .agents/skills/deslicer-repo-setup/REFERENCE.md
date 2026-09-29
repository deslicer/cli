# Setup command reference

Examples contain placeholders: resolve them before execution, never invent IDs
or credentials. Check installed CLI help if flags differ. This reference is
self-contained so the skill can be installed without a CLI source checkout.

## A2 default versus GitHub App/OIDC opt-in

For a new GitHub setup with no path specified, select **A2** and say so.
Explicit user choice wins; preserve existing working App/OIDC configuration.
Conflicting configuration needs clarification, not silent migration. This is a
skill policy: the CLI still maps `--provider auto` on GitHub to `github`.

| Decision | A2 — default | GitHub App/OIDC — explicit or existing |
| --- | --- | --- |
| Deslicer identity locally | Observer URL + tools-scope API key | Portal device login for setup/inspection |
| Deslicer identity in CI | Environment-scoped Observer tools key | Short-lived GitHub Actions OIDC identity |
| Init provider | `github-token` explicitly | `github` explicitly |
| Deslicer GitHub App required | No | Yes, connected installation and environment binding |
| Remote creation | Existing remote or authenticated `gh repo create` | Existing repo or device-authenticated `deslicer repo bootstrap` |
| Binding | No `init --bind` | Device-authenticated `init --bind`, target-group UUID |
| CI requirements | Environment secret `DESLICER_API_TOKEN`; Observer URL and environment variables | Generated OIDC workflow, `id-token: write`, portal resolution/binding |
| First git-sourced plan | Tools-token CLI or configured token CI; private clone access is separate | Reviewed real OIDC CI workflow; device login alone cannot create a git-sourced plan |

Both paths still need GitHub permissions for repository administration and Git
transport. A2's Observer token is neither a GitHub token nor a clone credential.
Device login is not a local GitHub OIDC token. `--source-dir` is a separate
bundle source choice (Path B), not a synonym for A2 or a third CI identity.

Default A2 sequence: secure Observer credentials → live groups check → GitHub
login/repo → `init --provider github-token` → app mappings/validation → CI
Environment configuration → reviewed push → propose/inspect. No App install,
device login, OIDC configuration, or `--bind` is needed for this sequence.

For explicit App/OIDC, follow device login → connected installation/repo →
`init --provider github --bind` → reviewed real CI plan. Do not install an
Observer token as a workaround; direct-token overrides take precedence over OIDC.

### Handoff to propose/inspect

Pass the selected path, portal/backend identity (no secrets), environment,
exact group, repo/commit, and any existing plan ID plus CI run/commit evidence.
Reuse a matching CI-created plan rather than creating a duplicate. On A2,
continue with configured tools credentials. On App/OIDC, inspect via an existing
authorized device session or real CI identity; create git plans in reviewed CI.
If access is missing, return to authentication setup without changing paths.

## Local Git versus remote provisioning

For an approved new local directory that is not inside another checkout:

```bash
git init -b main
git remote add origin <verified-remote-url>
```

Adding origin does not create the hosted repository. If a remote already exists,
clone it instead of creating a competing history. Never automatically force-push
or replace origin to resolve a mismatch. `deslicer init --dir PATH` requires an
existing directory; it does not run `git init`, clone, commit, or push.

GitHub App organization provisioning requires a device session and a connected
organization installation. Obtain its ID from the portal; `repo status` itself
requires that ID. An Observer tools token cannot substitute for device login.
Check `auth whoami` for the intended tenant; do not silently clear or switch
the user's configured authentication to make a command work.

```bash
deslicer auth login
deslicer auth whoami
deslicer repo status --installation 123
deslicer repo bootstrap --installation 123 --name splunk-apps
```

The last command authenticates and reads installation data but only previews
the resolved organization, name, and private visibility. After confirming the
target and authorization for creation:

```bash
deslicer repo bootstrap --installation 123 --name splunk-apps --yes
git clone <returned-repository-url>
```

Inspect the checkout before scaffolding: server provisioning may already have
created files. Do not blindly rerun init with `--force`. To request a workflow
refresh later, `deslicer repo refresh --installation 123 --repo-id <numeric-id>`
enqueues a mutation; use `repo status --installation 123` to inspect the resulting
PR/status. Enqueued is not merged or synchronized.

Cloning a private repository also requires the user's GitHub Git transport
credentials (SSH or an approved credential helper). Deslicer device login does
not configure Git authentication; never embed a token in the clone URL.

## GitHub Observer-token path (A2)

Supply `OBSERVER_API_URL` and a dedicated tools-scope `DESLICER_API_TOKEN`
through the user's secure credential mechanism. Never use a dashboard admin/read
key, put tokens in Git URLs, echo them, or commit them. GitHub permissions are
separate from Observer permissions.

```bash
deslicer auth whoami
deslicer groups list
deslicer init --provider github-token --environment acme-prod
```

Do not use `auto`: a GitHub origin selects `github` (App/OIDC).
Do not use `--bind`: A2 does not create an App binding. Online init fetches pinned
templates from Observer and writes/merges
`.deslicer/environments/acme-prod.yml`. Existing `apps:` mappings are preserved
by the merge but must be reviewed. Missing or ambiguous environment selection
should be resolved explicitly, not guessed.

The CLI prints a GitHub Environment recipe; **it never executes it or writes
secrets**. With separately authorized repo-admin GitHub access, configure:

| Scope | Type / name | Value |
| --- | --- | --- |
| GitHub Environment `acme-prod` | Secret `DESLICER_API_TOKEN` | Tools-scope token |
| Same Environment | Variable `OBSERVER_API_URL` | Observer management URL |
| Same Environment | Variable `DESLICER_ENVIRONMENT` | `acme-prod` |
| Same Environment | Optional variable `TARGET_GROUP_ID` | UUID only if the workflow uses it |
| Repository | Variable `DESLICER_ENVIRONMENT` | Environment-name pointer for PR jobs |
| Repository | Variable `DESLICER_API_URL` | Portal base for plan links |

Resolve `$OWNER/$REPO` in any printed recipe to the verified origin. Send secrets
via stdin (with shell tracing disabled), not literal command arguments. A second
Observer backend uses a second GitHub Environment and workflow matrix row, not a
replacement repo-level token. Inspect actual generated workflows for requirements.

Map real repo-relative app directories to exact live group names:

```yaml
destinations:
  - inventory_group: indexers
    apps:
      - source_path: apps/my_indexer_app
```

Refresh inventory carefully:

```bash
deslicer inventory sync --environment acme-prod --dry-run
# After reviewing intended changes:
deslicer inventory sync --environment acme-prod
deslicer inventory validate --environment acme-prod
```

Sync preserves mapped apps; stale groups with apps block removal and exit 2.
Non-dry-run sync can still write other changes. Do not delete apps just to make
validation pass; ask the operator which mappings should be retired or corrected.
An empty `apps:` placeholder is not a configured deployable app.

## App/OIDC binding and other providers

For GitHub App binding, first verify the connected installation, intended
environment, origin, and group UUID from `deslicer groups list`:

```bash
deslicer auth login
deslicer auth whoami
deslicer init --provider github --environment production \
  --target-group <host-group-uuid> --bind
```

`--bind` is an opt-in remote mutation. Unlike `change plan --target-group`, init
requires a UUID, not a group name. Missing GitHub connection may print portal
instructions and exit zero: require a confirmed binding result. App-provisioned
environment files come from server repo sync; inventory sync is not a substitute
for App provisioning.

| Provider | Setup boundary |
| --- | --- |
| `gitlab` | Device-session `--bind` supports GitLab.com; self-managed uses bundles |
| `azure` | Scaffold pipelines; portal setup and `change plan --source-dir` bundles |
| `bitbucket` | Scaffold pipelines; portal setup and `change plan --source-dir` bundles |

Unknown Git hosts need an explicit provider. Repo bootstrap/status/refresh are
GitHub-App-only, not remote provisioners for these other platforms.

## Offline, conflicts, and handoff

`--offline` loads the last successfully fetched provider cache; it is not an
authentication bypass for binding. Missing cache fails. Token-path offline init
skips tenant inventory generation. Never combine `--offline --bind`: templates
can be written before the command rejects binding. Invalid binding UUIDs and
later network errors can likewise occur after writes. Inspect status/diffs after
both success and failure; init is not transactional.

For existing templates, use an empty staging directory and explicit provider
with `--dir`, inspect the result, and integrate only intended changes. Do not
copy a fresh empty environment file over operator app mappings. `--force` also
overwrites README templates that are otherwise preserved.

Review `git diff`, new files from `git status --short`, and `git diff --check`.
Stage only intended paths when commit/push is requested; check for secrets.
Record what remains unverified: binding, GitHub credentials, workflow execution,
or first plan. A plan requires explicit scope and does not authorize approval,
deploy, enrollment, or host changes.
