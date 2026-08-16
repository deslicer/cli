# Local testing

Test the CLI end-to-end from your machine against a local (or any reachable) Observer API — no CI runner, GitHub App, or OIDC setup required. The bundle flow is the recommended local path because it authenticates with a static API key.

## 1. Build the CLI from source

```bash
git clone https://github.com/deslicer/cli.git
cd cli
cargo build
```

The binary is `target/debug/deslicer` (the crate is `deslicer-cli`, the binary is `deslicer`):

```bash
./target/debug/deslicer --version
```

## 2. Point at an Observer API

You need the Observer **management plane** URL and an API key with the `tools` scope.

**Against a deployed Observer** — create the key in the Deslicer portal (**Platform → API keys**, scope `tools`):

```bash
export OBSERVER_API_URL="https://observer.example.com:8088"
export DESLICER_API_TOKEN="<tools-scope-api-key>"
```

**Against a local DAP dev stack** — the management plane listens on `http://localhost:8080` by default. Plain `http://` is fine for localhost testing. Use the seeded dev key from the stack's `.env` (`SEED_API_KEY`, seeded when `ALLOW_SEED_API_KEY=true`):

```bash
export OBSERVER_API_URL="http://localhost:8080"
export DESLICER_API_TOKEN="<SEED_API_KEY value>"
```

Note: for the compile step to run, the Observer must be able to launch the compile-runner container (`COMPILE_RUNNER_IMAGE`) and that container must be able to reach the Observer's data plane. On a local stack where Observer runs natively, set `COMPILE_RUNNER_OBSERVER_API_URL=http://host.docker.internal:8443` in the Observer's environment.

## 3. Pick a target host group

Every plan targets a host group UUID. After `deslicer auth login`:

```bash
deslicer --deslicer-api-url http://127.0.0.1:3000 groups list
deslicer --deslicer-api-url http://127.0.0.1:3000 inventory list
```

Or with a direct Observer API key:

```bash
curl -s -H "Authorization: Bearer $DESLICER_API_TOKEN" \
  "$OBSERVER_API_URL/api/v1/groups" | jq -r '.[] | "\(.id)  \(.name)"'
```

The group may be empty — the plan still compiles (the diff runs against whatever observed state exists).

## 4. Create a scratch source directory

```bash
mkdir -p /tmp/deslicer-cli-test/apps/cli_demo_app/default
cat > /tmp/deslicer-cli-test/apps/cli_demo_app/default/app.conf <<'CONF'
[install]
state = enabled

[ui]
is_visible = false
label = CLI Demo App
CONF
```

## 5. Run the flow

```bash
./target/debug/deslicer change plan \
  --source-dir /tmp/deslicer-cli-test \
  --target-group <host-group-uuid> \
  --name "local-smoke-test"
```

Expected output:

```
packaged 1 files (245 bytes, sha256 ...)
bundle uploaded: 019f37fa-...
{"plan_id":"69074147-...","plan_status":"pending_approval",...}
```

The command uploads the bundle, creates the plan, triggers the compile-runner, and polls until the plan leaves `draft` — typically a few seconds on a local stack. Add `--no-wait` to return immediately after the trigger.

## 6. Inspect the result

```bash
./target/debug/deslicer change show --plan-id <plan_id>
```

Or in the Deslicer portal under **Automate → Plans**. Approval and execution require a verified human identity, so complete those steps in the portal.

## Troubleshooting

| Symptom | Likely cause |
|---------|--------------|
| `--source-dir requires direct Observer access` | `OBSERVER_API_URL` not exported |
| `403 Forbidden` on upload | API key lacks the `tools` scope |
| Plan never leaves `draft` | Compile-runner image missing or can't reach the Observer — check Observer logs |
| `connection refused` | Observer not running, or you used the data plane port instead of the management plane |

More detail in [bundle-flow.md](bundle-flow.md#troubleshooting). For testing the proxied CI path locally (`--ci-platform local`), set `DESLICER_DEV_TOKEN` to a pre-issued dev token — see [oidc-troubleshooting.md](oidc-troubleshooting.md).
