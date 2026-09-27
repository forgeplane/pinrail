---
title: Scripts and CI
description: "Put a person in a script or a CI job: submit a review, branch on the exit code, and act on the decision as JSON."
---

An agent reads a decision as markdown. A script wants something it can branch on: JSON, an exit code, and a file it already knows how to read. Pinrail gives it all three.

## The shape of a gated step

```sh title="apply-with-approval.sh"
#!/usr/bin/env bash
set -euo pipefail

./propose-changes > proposals.json            # what the step wants to do

status=0
pinrail submit list --title "Prune stale branches in acme/api" \
  --origin repo=acme/api,workflow=prune,run_id="$RUN_ID" \
  --data proposals.json --wait --json --decision-out decision.json > review.json || status=$?

case $status in
  0) ./apply-decisions decision.json ;;        # decided: act on the verdicts
  5) echo "Discarded: $(jq -r .discarded_reason review.json)"; exit 1 ;;
  *) echo "No decision (exit $status)"; exit 1 ;;
esac
```

:::caution[set -e and exit codes]
With `set -e`, a non-zero exit from `pinrail` ends the script before you can branch on it. Capture the code with `|| status=$?`, as above, or run the command as the condition of an `if`.
:::

```mermaid title="A gated step"
flowchart LR
  P["propose"] --> S["pinrail submit --wait"]
  S --> Y(["a person decides"]):::you
  Y --> S
  S -->|"0 · decided"| A["apply decision.json"]
  S -->|"5 · discarded"| X["stop"]
  S -->|"3 or 4 · no decision"| L["stop, or try later"]
```

## Reading the decision

`--decision-out <file>` writes the decision's data, the part the plugin defines, to a file, so a step that already reads a decisions file keeps working. It is always JSON, and it is written only when the review was decided.

For the built-in [List](/docs/plugins/list/) plugin, acting on accepted items is a line of `jq`:

```sh
jq -r '.decisions[] | select(.action == "accept") | .id' decision.json | while read -r id; do
  ./apply "$id"
done
```

Everything else about the review, who decided, when, and their note to the agent, is in the review printed on stdout.

## Running in CI

Pinrail's server accepts requests only from programs on the same computer. It listens on `127.0.0.1` and refuses requests addressed to any other host, so a hosted CI runner cannot reach it.

To request a review from a CI job, run the job on the computer where Pinrail runs, for example on a self-hosted runner installed on the reviewer's computer. If the app might not be open when the job runs, tell the CLI how to start the server:

```sh
export PINRAIL_SERVER_CMD='pinrail-desktop --headless'   # Linux, from the .deb or .rpm
```

If no server is running, `submit` starts one. The server runs in the background and writes its log to `server.log` in its data directory.

## Don't block the pipeline

A job that waits on a person can wait a long time. Two ways to keep it bounded:

- **`--timeout <seconds>`** gives up after that long with exit 4. The review stays pending, and a later job can pick it up with `pinrail wait <id>`.
- **`--expires-at <time>`** closes the review if nobody decides in time. Its waiter exits 3.

For a step that should not hold a runner at all, split it in two: submit without `--wait` and store the id, then have a later job, or a scheduled one, run `pinrail wait <id>` and apply the decision.

## Label reviews so you can find them

`--origin` tells the app where a review comes from. It groups reviews by project in the inbox, links back to the run, and lets you filter:

```sh
--origin repo=acme/api,workflow=prune,run_id=$RUN_ID,ref=main,url=$RUN_URL
```

```sh
pinrail list --repo acme/api --workflow prune --status pending
```

`workflow` names the job, so the person deciding knows which pipeline is asking. `--requested-by` (or `PINRAIL_REQUESTED_BY`) names who is asking: by default the coding agent the job runs, when it runs one, and otherwise `pinrail-cli`.
