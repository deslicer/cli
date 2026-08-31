# Quickstart

Get from zero to an approved, deployed Splunk configuration change. Pick the path that matches where you are running the CLI.

For system context (components, sequence diagrams, DAP vs DAI boundaries), see [architecture.md](architecture.md).

| Path | Where it runs | Auth | Requires |
|------|---------------|------|----------|
| [A. CI pipeline (OIDC)](#path-a-ci-pipeline-with-oidc) | GitHub Actions, GitLab CI, Azure DevOps, Bitbucket | CI OIDC token, exchanged automatically | Repo allowlisted + environment bound in the Deslicer portal |
| [A2. CI pipeline (Observer API token)](#path-a2-ci-pipeline-with-an-observer-api-token) | GitHub Actions, GitLab CI | Static Observer API key | `OBSERVER_API_URL` + `DESLICER_API_TOKEN` + `--target-group` (runner must reach Observer management) |
| [B. Bundle upload (GitHub-App-free)](#path-b-bundle-upload-github-app-free) | Any machine or CI runner | Static Observer API key | `OBSERVER_API_URL` + `DESLICER_API_TOKEN` |

Install the CLI first — see [installation.md](installation.md).

---

## Path A: CI pipeline with OIDC

This is the default mode: the CLI exchanges your CI runner's OIDC token via deslicer-ai, resolves the correct Observer backend for your tenant, and drives the full plan lifecycle.

### 1. Verify the runner can authenticate

```bash
deslicer auth status --environment production
```

Expect your CI platform to be detected and OIDC to be available. If not, see [oidc-troubleshooting.md](oidc-troubleshooting.md).

### 2. Create a plan

From a checkout of your Splunk configuration repository:

```bash
deslicer change plan --environment production
```

The compile-runner clones the repo at the current commit, diffs it against the observed state of your target hosts, and produces a plan. The command waits until the plan reaches `pending_approval` (use `--no-wait` to return immediately) and emits `plan_id`, `plan_status`, and `plan_summary` as CI outputs (see [ci-outputs.md](ci-outputs.md)).

### 3. Approve

```bash
deslicer change approve --plan-id "$PLAN_ID" --environment production
```

Plan approval requires a verified human identity. In CI this works when the job is gated by a GitHub Environment with required reviewers — the CI proxy attests the reviewer who approved the deployment. Without that gate, approve the plan in the Deslicer portal instead (**Automate → Plans**).

### 4. Deploy and verify

```bash
deslicer change deploy --plan-id "$PLAN_ID" --environment production
deslicer change verify --plan-id "$PLAN_ID"
```

`deploy` monitors the rollout until it finishes (use `--no-wait` to queue and return). `verify` re-runs a dry-run compile against the deployed state and confirms the change landed.

---

## Path A2: CI pipeline with an Observer API token

Use this when the runner can reach Observer's **management** plane and you do not want to set up CI OIDC. This is **not** DAI's stored admin/read key — mint a dedicated `tools`-scope key in the portal.

### 1. Create the key

In the Deslicer portal: **Platform → API keys** → create a key with scope **`tools`**. Copy the plaintext once. Store it as the GitHub Actions secret `DESLICER_API_TOKEN`. Also store `OBSERVER_API_URL` (management URL, not the data-plane port).

Do not reuse the keys DAI stores on the tenant for the dashboard/CI proxy.

### 2. GitHub Actions

```yaml
permissions:
  contents: read

jobs:
  plan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          lfs: true
      - name: Plan change
        env:
          OBSERVER_API_URL: ${{ secrets.OBSERVER_API_URL }}
          DESLICER_API_TOKEN: ${{ secrets.DESLICER_API_TOKEN }}
          # Forwarded to Observer for one clone of this commit. Never stored.
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: deslicer change plan --target-group ${{ vars.TARGET_GROUP_ID }}
```

The CLI reads `GITHUB_REPOSITORY` and `GITHUB_SHA` from the runner (no `id-token: write` needed) and registers them on the plan. Observer's ephemeral compile-runner then clones that exact commit, so **git-lfs pointers are resolved to their contents** — something a bundle upload cannot do.

### 3. Cloning a private repository without a GitHub App

If your tenant has the Deslicer GitHub App installed on the repository, Observer mints its own short-lived installation token and that always takes precedence.

Without an App installation, Observer has no credential of its own, so the CLI forwards the job's `GITHUB_TOKEN` for that single clone. The value is held in memory for one request, passed to the runner container as an environment variable, and never written to the database or a log line.

Set `DESLICER_GIT_CLONE_TOKEN` instead when the job token cannot read the repository — for example when your Splunk configuration lives in a separate repo that needs a fine-grained PAT.

Two boundaries are deliberate:

- A repository that **is** mapped to a tenant App installation whose App is misconfigured still fails closed. A forwarded token can never substitute a foreign identity for a tenant-bound one.
- GitLab remotes ignore `CI_JOB_TOKEN`; they continue to require a tenant repo binding plus `GITLAB_COMPILE_TOKEN` on Observer, because a GitLab job token needs a different HTTPS username and forwarding it would authenticate incorrectly rather than fail loudly.

### 4. Approve

Approve in the portal (**Automate → Plans**). A `tools` key cannot self-approve.

---

## Path B: Bundle upload (GitHub-App-free)

No GitHub App, no OIDC, no repository integration: package a local directory into a digest-pinned bundle and compile a plan from it. Ideal for evaluation, air-gapped environments, and CI systems without a supported OIDC integration.

### 1. Set credentials

```bash
export OBSERVER_API_URL="https://observer.example.com:8088"   # management plane
export DESLICER_API_TOKEN="<api-key-with-tools-scope>"
```

Create the API key in the Deslicer portal under **Platform → API keys** with the `tools` scope.

### 2. Lay out your configuration

The directory must contain your Splunk apps under `apps/`:

```
my-splunk-config/
└── apps/
    └── my_app/
        └── default/
            ├── app.conf
            └── props.conf
```

### 3. Create the plan

```bash
deslicer change plan \
  --source-dir ./my-splunk-config \
  --target-group <host-group-uuid> \
  --name "my-first-bundle-plan"
```

The CLI packages the directory (deterministic tar.gz), uploads it with its SHA-256 digest, creates a plan bound to that bundle, triggers compilation, and waits for `pending_approval`.

### 4. Approve and execute in the portal

Bundle-sourced plans are approved and executed from the Deslicer portal (**Automate → Plans**) — approval always requires a verified human identity.

For details, limits, and the security model of this flow, see [bundle-flow.md](bundle-flow.md).

---

## Inspect a plan at any time

Works for every path above:

```bash
deslicer change status --plan-id "$PLAN_ID"   # plan/execution progress
deslicer change show   --plan-id "$PLAN_ID"   # plan details
deslicer change show                          # list recent plans
```

---

## Where to next

- [bundle-flow.md](bundle-flow.md) — the GitHub-App-free flow in depth
- [ci-outputs.md](ci-outputs.md) — output variables the CLI writes for pipeline steps
- [environments.md](environments.md) — the `.deslicer/environments/` convention
- [oidc-troubleshooting.md](oidc-troubleshooting.md) — exit codes and per-platform OIDC fixes
