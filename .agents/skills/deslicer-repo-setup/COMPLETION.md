# Finish repository setup

Do not stop after generating files. Complete the requested setup steps and
record evidence for each; pause for missing credentials, access, or decisions.
These commands are examples with verified operator inputs, not permission to
change an arbitrary repository.

## Create a GitHub repository without the Deslicer App

The token path can use a normal GitHub repository. After GitHub authentication,
verify the owner, repository name, visibility, and creation rights. For an
approved new private remote from an existing local repo without origin:

```bash
gh repo create OWNER/REPO --private --source=. --remote=origin
```

This creates a remote and adds origin, but does not push without `--push`.
Do not use it if the remote already exists; verify and reuse/clone that remote.
For a new checkout instead: `gh repo create OWNER/REPO --private --clone`.
Use explicit owner/name to avoid accidentally creating under the personal
account. No-App creation is not `deslicer repo bootstrap`; that command needs
device auth and a connected organization installation. Never create a duplicate
repo merely because App provisioning or clone authentication failed.

## Configure token-path GitHub CI

Use verified `REPO_SLUG=OWNER/REPO` and `ENVIRONMENT=acme-prod` values, plus
securely supplied Observer credentials and the nonsecret portal URL. Validate
the environment name against the generated YAML stem. Inspect existing
Environment settings, secret/variable names, and generated workflows before
writing. Preserve required reviewers, branch restrictions, and existing secrets;
do not overwrite a secret without authorization (its old value cannot be read
back). If the Environment does not exist (confirmed 404, not a permission or
network error), create it:

```bash
gh api --method PUT "repos/$REPO_SLUG/environments/$ENVIRONMENT"
```

Apply the init-generated recipe, supplying secret values only via stdin:

```bash
set +x
printf '%s' "$DESLICER_API_TOKEN" | gh secret set DESLICER_API_TOKEN --env "$ENVIRONMENT" --repo "$REPO_SLUG"
printf '%s' "$OBSERVER_API_URL" | gh variable set OBSERVER_API_URL --env "$ENVIRONMENT" --repo "$REPO_SLUG"
printf '%s' "$ENVIRONMENT" | gh variable set DESLICER_ENVIRONMENT --env "$ENVIRONMENT" --repo "$REPO_SLUG"
printf '%s' "$ENVIRONMENT" | gh variable set DESLICER_ENVIRONMENT --repo "$REPO_SLUG"
printf '%s' "$DESLICER_API_URL" | gh variable set DESLICER_API_URL --repo "$REPO_SLUG"
```

Require all inputs to be present before writes; do not write empty values.
`TARGET_GROUP_ID` is optional, only for templates that still require a UUID.
Use exact group names and per-environment matrices where generated workflows
support them. Separate Observer backends need separate GitHub Environments.

Verify configuration without reading secret values:

```bash
gh api "repos/$REPO_SLUG/environments/$ENVIRONMENT" --jq .name
gh secret list --env "$ENVIRONMENT" --repo "$REPO_SLUG"
gh variable list --env "$ENVIRONMENT" --repo "$REPO_SLUG"
gh variable list --repo "$REPO_SLUG"
```

Secret presence is not proof of the key's validity; a correctly scoped CI run
is the runtime check. Do not retrieve secrets to compare them. Treat variable
output as private configuration and avoid unnecessarily reproducing it in chat.

For App/OIDC, follow the generated workflow instead of installing an Observer
token: verify its `id-token: write`, environment selection, and portal binding.
Never manufacture runner OIDC variables locally. Azure/Bitbucket remain bundle
paths; GitLab.com binding has the limitations in REFERENCE.md. Use native CI
secret/service-connection controls for other providers, not GitHub recipes.

## Review, commit, and first push

Before publishing, inspect every workflow trigger and job: pushing a branch,
opening a PR, or dispatching a workflow can have remote effects including deploy.
Do not publish a trigger that deploys without that scope; use a reviewed
plan-only path or ask the user how to gate it. Keep approval protections intact.

1. Confirm live groups, actual app directories, YAML mappings, and binding where
   applicable. Run `deslicer inventory validate --environment NAME`.
2. Inspect `git status --short`, `git diff`, and new files; run
   `git diff --check`. Check generated content for credentials, local artifacts,
   and unwanted app changes. Add appropriate ignore rules, not secrets.
3. Verify local author identity, current branch, and origin. Stage explicit
   intended paths; inspect `git diff --cached` before committing. Do not use
   `git add .` in a dirty repo. Create the requested setup commit.
4. If push is in scope, fetch and check remote history/default branch first.
   For an empty remote, push the agreed initial branch with upstream tracking.
   For an existing repo, push a setup branch and open a PR when requested.
   Never force-push, overwrite unrelated history, or bypass branch protections.

## First safe CI check and handoff

List actual workflows (`gh workflow list --repo OWNER/REPO`) and inspect their
checked-in YAML. Do not guess workflow names or dispatch a deploy workflow.
Use the requested branch/PR trigger; manually dispatch only a reviewed workflow
that supports `workflow_dispatch` and whose effects are authorized. A first
plan is a remote mutation; confirm it is included before triggering a plan job.

```bash
gh run list --repo OWNER/REPO --branch SETUP_BRANCH --limit 5
gh run view RUN_ID --repo OWNER/REPO
```

Match the run to the exact pushed commit and intended environment; inspect
failed-job logs if needed, redacting sensitive output. Pending, skipped, or
approval-blocked jobs are not successful onboarding. For an authorized plan
smoke test, retain the plan ID and inspect it; stop before approve/deploy.

Return a completion checklist with evidence and remaining blockers:

- Tools and versions; intended Deslicer identity/backend and successful live read.
- Git-host account/access and Git transport; local author identity.
- Local root, branch, remote URL, and intended visibility.
- Generated workflows and nonempty app mappings; validation result.
- Binding result or token-path Environment/secret/variable configuration.
- Commit SHA, pushed branch/PR, and matching CI run/result when requested.
- First-plan ID/status if authorized; otherwise explicitly not run.

Report “scaffolded,” “configured,” and “runtime verified” separately. Resume a
partially completed setup by inspecting existing state, not recreating repos,
rotating credentials, overwriting workflows, or re-running side effects blindly.
