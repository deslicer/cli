# Authentication and prerequisites

Authenticate three separate layers: Deslicer/Observer, Git-host administration,
and Git transport. Success in one does not establish the others.

## Tools and target

Check `git --version`, `deslicer --version`, and (for GitHub) `gh --version`.
Use the user's package manager/approved installer for missing tools; inspect
installation instructions rather than silently running a downloaded script.
On macOS, documented CLI installation is `brew install deslicer/tap/deslicer`;
Rust users can use `cargo install deslicer-cli`. Recheck installed help.
Do not upgrade an existing installation merely to perform setup.

Resolve the directory, Git host/owner/name, visibility, portal URL, Observer URL
if direct, tenant/environment, and intended groups. Portal and Observer URLs
are not interchangeable. Do not infer tenant identity from a directory name.

## Deslicer device login (App / portal)

`DESLICER_API_URL` or global `--deslicer-api-url` selects the portal API base.
For the user's verified portal, in an interactive terminal:

```bash
export DESLICER_API_URL="https://portal.example.com"
deslicer auth login
deslicer auth whoami
deslicer auth status
deslicer groups list
```

The CLI displays/opens a verification URL and code and polls for user approval.
Have the user authenticate and choose the intended tenant in the browser; never
handle their password/MFA or approve the consent yourself. Without a TTY, login
fails: offer an interactive terminal, not invented `--token`/`--api-key` flags
or fabricated CI credentials. After expiry, restart the login flow. Device
sessions are stored using the OS keychain with a private-file fallback; never
read or print session storage. Persist the nonsecret portal setting only in a
user-approved location when needed for subsequent commands.
Keep this portal selector on every subsequent portal command (environment or
explicit global flag); do not let a new shell silently revert to the default.
Use the same verified portal value for the CI `DESLICER_API_URL` variable.

Credential precedence is **Observer URL + token → detected CI OIDC → stored
device session → interactive login**. `auth login` reuses valid existing
credentials; changing the portal flag alone does not force account switching.

If App commands still resolve `observer_api_token`, inspect only whether
`OBSERVER_API_URL` and `DESLICER_API_TOKEN` are set, not their values. With the
user's selected auth path, omit these overrides in a scoped child process (e.g.
`env -u OBSERVER_API_URL -u DESLICER_API_TOKEN deslicer ...`), consistently for
all App commands. Do not edit global shell files or erase working credentials.
Likewise diagnose unexpected CI detection before proceeding. For an explicitly
requested device-account switch, explain that `deslicer auth logout` clears the
stored device session, then log into the intended portal again. Logout does not
clear environment credentials. Retired `DESLICER_DEV_TOKEN` must not be reused.

## Observer tools-token path

Obtain a dedicated tools-scope key through the intended tenant's approved
Observer administration process; if the user lacks key-creation rights, request
an administrator's help. This CLI has no API-key minting/login flag. Never
substitute a dashboard admin/read key or a device session token.

Supply both `OBSERVER_API_URL` and `DESLICER_API_TOKEN` through a secret manager
or secure terminal input. For Bash, an operator can enter the key without
putting its literal value into shell history:

```bash
set +x
export OBSERVER_API_URL="https://observer.example.com:8088"
read -r -s -p 'Observer tools token: ' DESLICER_API_TOKEN
export DESLICER_API_TOKEN
printf '\n'
deslicer auth whoami
deslicer auth status
deslicer groups list
```

Run this in the operator's trusted interactive shell, not a logged agent tool
input. Never request the key in chat. Environment credentials are not persisted
by `auth login`; retain them only using the user's approved secret mechanism.
Exports in another terminal do not reach the agent's shell. Continue authenticated
commands in that same trusted shell, or use an approved secret mechanism
available to the agent process; never bridge the gap by pasting tokens into chat.
In token mode, `whoami` and `status` report configured identity, not remote
validity. Require a successful live `groups list` and confirm the intended
backend/groups before scaffolding. Empty groups are not deploy readiness.

## GitHub account and Git transport

Use installed `gh` help to verify options. Start with:

```bash
gh auth status --hostname github.com
gh api user --jq .login
```

If unauthenticated, initiate `gh auth login --hostname github.com --git-protocol
https --web` in an interactive terminal and let the user complete consent.
Never use `--show-token`, `gh auth token`, or insecure storage for diagnostics.
If a credential store is unavailable, disclose `gh`'s plaintext fallback and
use the organization's approved secret mechanism instead. `GH_TOKEN` or
`GITHUB_TOKEN` can override stored login: inspect presence only and resolve the
intended account without silently clearing credentials. For an authorized
stored-account switch, use `gh auth switch --hostname github.com --user LOGIN`.

For HTTPS, `gh auth setup-git --hostname github.com` configures Git's credential
helper; explain this persistent host-level change before doing it. Preserve an
existing working helper. For SSH, reuse the user's configured key/agent and
verify repository access; do not generate/upload keys or disable host-key
checking without approval. Org SSO/installation access may need administrator
approval; do not broaden token scopes or change org policies automatically.

For an existing repo verify host permissions and transport separately:

```bash
gh repo view OWNER/REPO --json nameWithOwner,url,isPrivate,viewerPermission,defaultBranchRef
git ls-remote <verified-remote-url>
```

An empty remote can return no refs successfully. Read access is not push/admin
permission; verify the required repository role. Deslicer App installation
permission does not automatically grant the human GitHub account repo access.

Inspect `git config --get user.name` and `git config --get user.email`. If
missing, ask for the intended author identity (including privacy preference),
then set `git config --local user.name 'Approved Name'` and the approved email
inside the repo. Do not infer an email or alter global identity.

## Troubleshooting gates

| Symptom | Next check |
| --- | --- |
| Observer 401/403 | Correct backend and tools key, expiry/scope; never log key |
| TLS/network failure | DNS, connectivity, trusted certificates; no TLS bypass |
| App command says login required | Direct-token precedence, CI detection, device expiry |
| Clone denied but Deslicer login works | GitHub account, repo role, SSO, Git credential helper/SSH |
| Secret write denied | GitHub environment/admin permissions, not Observer auth |
| Browser approval/admin permission needed | Request that human step; resume after verification |

For non-GitHub hosts, use the host's native approved login/credential helper and
check read/push access; do not apply `gh` commands. Read generated CI templates
for provider-specific OIDC/service-connection variables and use their native
protected-secret store. Missing admin access is an explicit setup blocker,
not permission to fall back to credentials in repository files.
