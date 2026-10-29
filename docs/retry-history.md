# Retry history (`change status --jobs`, `change retry`)

`deslicer change status <plan_id> --jobs [--execution-id <id>]` prints one line
per job from Observer `GET /api/v1/executions/{id}`:

```
job <job_id>  status <status>  retries <retry_count>  <hostname>  attempt <n>  phase <phase_group_id>  reason: <failure_reason>
```

- `retries` is Observer `JobSummary.retry_count`: manual operator retries
  (0 = never retried). Same number the portal run-detail page shows.
- `attempt` is the delivery attempt for the current run (NATS redelivery),
  not the retry count.
- `--log-format json` prints the full job rows (`id`, `status`,
  `retry_count`, `attempt`, `host`, `failure_reason`, `phase_group_id`,
  `queued_at`, `completed_at`).

Observer sends `job_id` and `hostname`; the CLI also accepts `id` and `host`.

`deslicer change retry <plan_id> --job <job_id> --yes` re-queues one failed or
canceled job. Observer's response has no `retry_count`, so the CLI reports the
previous count plus one. Without `--yes` it prints a dry run.
