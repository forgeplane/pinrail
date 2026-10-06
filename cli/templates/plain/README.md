# __TITLE__

A Pinrail plugin: what an agent asks (`schemas/payload.schema.json`), what
the person answers (`schemas/decision.schema.json`), and the view between
them (`view/index.html` and `view/view.js`). It starts as one yes-or-no
question; make it yours from there.

```sh
pinrail plugins check .             # what the app would refuse, and why
pinrail plugins install . --link    # the app follows this folder as you change it
pinrail submit __NAME__ --sample    # a real review of its sample, in the inbox
pinrail docs building               # how a plugin works, and how to build one
```
