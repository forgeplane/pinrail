#!/bin/sh
# Records one agent session for the corpus and scrubs it.
#
#   corpus/tools/record-session.sh NAME [--plan PLAN.json] -- COMMAND [ARGS...]
#
# The command runs in a new throwaway folder holding a copy of demo/app.py.
# Without --plan you drive the session yourself; resizing your terminal is
# recorded. With a plan (see plans/), it runs unattended. The scrubbed file
# is written to corpus/recordings/NAME.cast; read the findings it prints,
# and the recording, before keeping it.
set -e
tools=$(cd "$(dirname "$0")" && pwd)
corpus=$(dirname "$tools")
name=$1
shift
plan=""
if [ "$1" = "--plan" ]; then
  plan="--plan $2"
  shift 2
fi
[ "$1" = "--" ] && shift
work=$(mktemp -d "${TMPDIR:-/tmp}/pinrail-rec.XXXXXX")
mkdir "$work/demo"
cp "$tools/demo/app.py" "$work/demo/"
raw="$work/raw.cast"
python3 "$tools/record.py" -o "$raw" --cwd "$work/demo" $plan --title "$name" -- "$@"
python3 "$tools/scrub.py" "$raw" "$corpus/recordings/$name.cast" --replace "$work=/tmp/rec" || true
echo "raw recording kept in $raw; delete it when done"
