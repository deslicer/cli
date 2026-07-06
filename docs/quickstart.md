# Quickstart

Get from zero to an approved, deployed Splunk configuration change. Pick the path that matches where you are running the CLI.

| Path | Where it runs | Auth | Requires |
|------|---------------|------|----------|
| [A. CI pipeline (OIDC)](#path-a-ci-pipeline-with-oidc) | GitHub Actions, GitLab CI, Azure DevOps, Bitbucket | CI OIDC token, exchanged automatically | Repo allowlisted + environment bound in the Deslicer portal |
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

### 5. Inspect at any time

```bash
deslicer change status --plan-id "$PLAN_ID"   # plan/execution progress
deslicer change show   --plan-id "$PLAN_ID"   # plan details
deslicer change show                          # list recent plans
```

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

## Where to next

- [bundle-flow.md](bundle-flow.md) — the GitHub-App-free flow in depth
- [ci-outputs.md](ci-outputs.md) — output variables the CLI writes for pipeline steps
- [environments.md](environments.md) — the `.deslicer/environments/` convention
- [oidc-troubleshooting.md](oidc-troubleshooting.md) — exit codes and per-platform OIDC fixes
