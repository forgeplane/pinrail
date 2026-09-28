---
title: Asking
summary: Submit a review, wait for the decision, read it, and handle every exit code.
menu: []
---
# Asking

```sh
pinrail submit <plugin> --title "<title>" --data payload.json --wait
```

- `--title`: what the person sees in the inbox. Required.
- `--data <file>`: the payload, JSON the plugin's schema accepts; `-` reads
  stdin.
- `--origin repo=…,ref=…,url=…`: the project, branch or pull request, and a
  link back. `repo` and `ref` come from git when you are in a checkout.
- `--wait`: block until the review ends, then print it.
- `--json`: the whole review as JSON; the person's answer is its
  `decision.data`, shaped by the plugin's decision schema
  (`pinrail plugins describe <plugin> --decision-schema`). Use it when you
  process the result; read the default markdown otherwise.
- `--decision-out <file>`: with `--wait`, write only `decision.data` to a
  file as well.
- `--dry-run`: every check a submission gets, and nothing created: exit 0
  or 2.
- `--timeout <seconds>`: with `--wait`, stop waiting after this long, exit 4.
  If your commands have a time limit, keep it below that. `PINRAIL_TIMEOUT`
  sets it once for every wait.
- `--summary '<json>'`: the counts the inbox shows beside the title.
- `--request <file>`: the whole request as one JSON file; flags override
  its keys.

Without `--wait`, `submit` prints the new review and returns: wait on it
later with `pinrail wait <id>`.

- stdout is the result: the review, and with `--wait` the review as it
  ended, its decision included. Read or parse only stdout.
- stderr is errors, warnings, and `review <id> submitted` as soon as the
  review exists, so you have the id while `--wait` blocks. `--verbose`
  adds what happened along the way.

## Files

Some plugins take files beside the payload: `pinrail plugins` says
"Takes files" and which kinds, and the plugin's payload schema marks
where each goes, as `{"$attachment": "<name>"}`. Put that in the payload
and send each file it names:

```sh
pinrail submit <plugin> --title "<title>" --data payload.json --attach render.png --attach out/v2.glb=model.glb --wait
```

`--attach PATH` sends a file under its own name, `PATH=NAME` under the name
the payload uses. `--dry-run` checks the files against the plugin's limits
before anything is uploaded.

## Exit codes

- `0`: decided. Act on the decision.
- `1`: bad arguments, the request could not be sent, or the app failed on
  its side. Report the error. A wait keeps waiting through an app error.
- `2`: refused by the app: a field that fails the plugin's schema, a
  review no longer pending, a plugin not installed. stderr says which;
  fix it and submit again.
- `3`: withdrawn, or expired undecided. Stop, and say nobody decided.
- `4`: still pending, `--timeout` ran out. Go on with other work, and run
  the command it prints later.
- `5`: discarded, the person said no, and stop. Stop the work, report their
  reason, and don't ask again.

## Several questions

If the plugin takes several questions or items in one review, send them
together. Otherwise submit each without `--wait`, then
`pinrail wait <id>` on each; every wait exits with its own code.

## After asking

- The person asked for changes: send the whole new version with
  `--revises <id>`; `pinrail rounds <id>` prints every round. Revise the
  latest round, with the same plugin. A round still pending when you revise
  it is withdrawn, and its waiter exits 3.
- Sending the same review again while it is still pending answers the one
  already waiting, so running the command twice asks the person once.
- `pinrail show <id>`: where a review stands, and its decision.
- `pinrail withdraw <id> --reason "<why>"`: you no longer need the answer.
- `pinrail list`: what is waiting on the person in this project; `--all`
  for every review.
