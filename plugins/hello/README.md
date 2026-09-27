# hello

The smallest complete plugin: one question, yes or no, with an optional
comment. Read `index.html` to see every protocol message handled in a few
lines, then copy the directory to start your own type.

Payload: `{ "message": "..." }`. Decision: `{ "ok": true }`, optionally
with `"comment"`.

Link the folder and send it a review:

```sh
pinrail plugins install ./plugins/hello --link
pinrail submit hello --title "Push the branch?" --data plugins/hello/example.json --wait
```
