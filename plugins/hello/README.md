# hello

The smallest complete plugin: one question, yes or no, with an optional
comment. Read `index.html` to see every protocol message handled in a few
lines, then copy the directory to start your own type.

Payload: `{ "message": "..." }`. Decision: `{ "ok": true }`, optionally
with `"comment"`.

Register the parent directory and create a gate:

```sh
pinrail types add ./plugins
pinrail create hello --title "Push the branch?" --data <(echo '{"message":"3 commits, CI green. Push?"}') --wait
```
