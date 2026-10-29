# Splunk upgrades (`deslicer upgrade`)

Three commands (refs #149). `readiness` and `preview` are read-only. `plan`
only creates a Pending plan: approving and deploying it is a separate,
human-gated step with `deslicer change`. `upgrade` has no apply flag.

All three accept `--json` (same as `--log-format json`) and exit 2 on
missing or invalid arguments, or a `--target` that matches no enrolled host.
`--target` takes a hostname (case-insensitive) or host UUID, resolved via
`GET /api/v1/hosts`. Observer requires an admin, verified-human session
(`deslicer auth login`).

## readiness

```
deslicer upgrade readiness --target <host> [--json]
```

`POST /api/v1/splunk-packages/bootstrap/evaluate`. Reports the upgrade path,
installed and pinned versions, strict X.509 gate, SHC captain stability
(`stable` | `flapping` | `unknown`) and reasons. Creates nothing.

## preview

```
deslicer upgrade preview --family <f> --target <host>... --to-version <v> \
  [--captain <host>] [--role-group <g>]... [--json]
```

`POST /api/v1/splunk-packages/upgrade-rolling/schedule`. Shows the bound
wave schedule and the next publishable wave. No plan is created.

`deslicer upgrade status` is a deprecated alias for `preview`, kept for one
release; it prints a deprecation notice on stderr.

## plan

```
deslicer upgrade plan --family <f> --target <host>... --to-version <v> \
  [--captain <host>] [--role-group <g>]... [--json]
```

`POST /api/v1/splunk-packages/upgrade-rolling`. Prints `plan_id` and the next
steps:

```
plan_id: <id>
next: deslicer change approve --plan-id <id>
  then: deslicer change deploy --plan-id <id>
```

`--json` returns `{plan_id, status: "pending_approval", next_steps, plan}`.

## Values

- `--family`: `upgrade_rolling_clusters`, `upgrade_non_clustered`
- `--role-group` (repeatable, ordered; default is the full catalog order):
  `precheck`, `cluster_managers`, `dcn_mc`, `searchhead`, `index_cluster`,
  `non_clustered`
- `--captain`: current SHC captain hostname, needed when Insights has not
  reported one; must be one of the targets.
- `--to-version`: dotted numeric version, e.g. `9.4.1`.
