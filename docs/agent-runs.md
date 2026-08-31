# Agent runs

`deslicer agent` talks to a deslicer-ai agent from a terminal or a pipeline
and streams the answer as it is produced. With no subcommand it opens a
conversation on the tenant Orchestrator. It talks to deslicer-ai directly —
the Observer backend is not involved — so `--observer-api-url` and
`DESLICER_API_TOKEN` play no part here.

## Authentication

Agent commands require a **device session**, the same credential the other
portal-backed commands (`enroll`, `worker`, `repo`) use:

```bash
deslicer auth login
```

CI OIDC sessions are rejected. An agent run acts as a person: it inherits your
tenant, your team, and the tools your role is allowed to reach, and there is no
principal behind an OIDC token that those checks can resolve. A pipeline that
needs to run an agent should carry a device session minted for a service
account.

## Listing agents

```bash
deslicer agent list
```

Prints the agents this session can run — the tenant's own agents plus the
public catalogue. The tenant Orchestrator is marked `(default)`. `--agent`
accepts either the id or the name.

## Conversation (REPL)

On a terminal, `deslicer agent` starts a line-oriented conversation. Each
prompt is one server run. The first prompt creates a conversation; later
prompts reuse it. Status and tools go to stderr; the answer goes to stdout.

```bash
deslicer agent
deslicer agent -a slicer
deslicer agent "Which indexers are missing the latest bundle?"
```

A bare prompt after `agent` is the same as `agent run`. Known subcommands
(`list`, `ls`, `run`, `logs`, `resume`) are never treated as prompts.

Inside the REPL:

| Input | Effect |
|-------|--------|
| empty line | ignored |
| `/help` | list the slash commands |
| `/exit`, `/quit`, Ctrl-D | leave |
| Ctrl-C during a stream | detach; the REPL stays open. Follow the run with `agent logs --follow` |

A new prompt is refused while the current run is still going. Non-TTY
invocations without a prompt still require `agent run` or a piped stdin.

## Resume

`deslicer agent resume` continues this session's last conversation — the
`conversationId` on your latest run. That is not the same as `agent logs`,
which reattaches to a *run*.

```bash
deslicer agent resume
deslicer agent resume "also check the SHC"
deslicer agent resume --follow
```

On a TTY with no prompt it enters the REPL on that thread. With a prompt it
starts a new run against the same conversation, using the last run's agent
unless `-a` is set. `--follow` attaches to the last run if it is still going
and does not start a second one. No history prints `No runs yet` and points
at `deslicer agent`.

## Running an agent

Omit `--agent` to run the tenant Orchestrator:

```bash
deslicer agent run "Which indexers are missing the latest bundle?"
```

To pick a different agent, pass a name or id:

```bash
deslicer agent run --agent slicer "Which indexers are missing the latest bundle?"
```

The answer streams to **stdout** as it arrives; the conversation id, each tool
the agent reaches for, and any diagnostics go to **stderr**, so `> answer.txt`
captures the answer and nothing else.

Omit the prompt to read it from stdin, which avoids the shell quoting and
argument-length limits of a long prompt:

```bash
cat incident-notes.md | deslicer agent run
```

Useful flags:

| Flag | Effect |
|------|--------|
| `-a`, `--agent <name-or-id>` | Run this agent instead of the tenant Orchestrator |
| `--conversation <id>` | Continue an existing thread instead of starting a new one |
| `--verbose` | Add the agent's reasoning to the progress already on stderr |
| `--no-wait` | Start the run, print its id, and exit |
| `--idempotency-key <key>` | Make a retried invocation join the original run |
| `--log-format json` | One JSON object per stream event instead of prose |

### Retries

A network blip between the CLI and deslicer-ai looks the same as a request that
never arrived, so a naive retry can start the agent twice. Pass a key that is
stable across attempts and the second request joins the first run rather than
starting another:

```bash
deslicer agent run \
  --idempotency-key "ci-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}" \
  "Summarise today's forwarder errors."
```

Keys are scoped to your session, so they cannot collide with anyone else's.

## Detaching and reattaching

The run's lifetime is not tied to your connection. Closing the terminal,
losing the network, or pressing Ctrl-C stops the *reading*, not the run — the
server keeps going and stores the transcript.

Start a run without waiting for it:

```bash
run_id=$(deslicer agent run --no-wait "Audit TLS settings across the fleet.")
```

List recent runs, or pick one up by id:

```bash
deslicer agent ls
deslicer agent logs "$run_id" --follow
```

`deslicer agent logs` without an id follows the most recent run.

`--follow` streams a run that is still going and waits for it to finish;
without it, `agent logs` prints where the run got to and exits. Either way the
run id is the handle — a conversation id is not, and `--conversation` starts a
*new* run against the same thread.

Ctrl-C during `agent run`, `agent logs --follow`, or a REPL turn detaches and
tells you the command that reattaches. In the REPL the loop stays open. The
run is unaffected.

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | The agent finished. With `agent logs` and no `--follow`, also "still running" — reporting the state is the job |
| `1` | Everything else, including a missing, expired, or unknown session; the message says what to do |
| `9` | Too many runs already in flight for this session; the message carries the wait |
| `10` | deslicer-ai unreachable |
| `13` | The agent run itself failed |
| `130` | Interrupted (Ctrl-C). The run continues server-side |

## Limits

- **Ten minutes** per run. A run that outlives the ceiling is marked failed and
  `agent logs` says to start it again.
- **Three concurrent runs** per session. Exceeding it is a `9` with a
  `Retry-After` of 30 seconds, not a queue.
- **32,000 characters** of prompt.
- **Tools that need a human decision are unavailable.** A terminal cannot
  render an approval card, so an agent reached from the CLI cannot take an
  action that would prompt for one. Approve those in the portal instead.
