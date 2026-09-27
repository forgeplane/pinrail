# Plugins

A gate type is a directory: a manifest, two JSON Schema documents, and a
self-contained HTML bundle that pinrail renders in a sandboxed iframe. Every
plugin lives here, the built-in ones included. `list` and `feedback` ship
inside the app, carried in the binary and written out at every start; the
others are installed one at a time:

| Plugin | What it gates |
|---|---|
| [`list/`](list/README.md) | a list of proposed actions, accepted or rejected one by one; built in |
| [`feedback/`](feedback/README.md) | questions an agent wants answered before it goes on: grouped, conditional, answered in one pass; built in |
| [`review/`](review/README.md) | a code review: the diff, the agent's proposed comments, the human's verdicts and own comments |
| [`email/`](email/README.md) | emails an agent wants to send: edit them with the changes showing, comment on a passage, send, revise or discard |
| [`artifact/`](artifact/README.md) | an HTML page an agent designed: pick elements the way DevTools does, comment on them, and the agent gets selectors back |
| [`calendar/`](calendar/README.md) | times to arrange around what is already booked: pick one suggested slot per item, conflicting ones step aside, or leave an item for the agent |
| [`logo/`](logo/README.md) | candidate logo marks, shown at every size, as app icons, in a tab and a menu bar: pick a favourite, keep or drop the rest, ask for changes to parts of a mark |
| [`model/`](model/README.md) | candidate 3D models (glTF or three.js JSON) on a stage to orbit, from set views and the agent's own, under three lights: pick a favourite, keep or drop the rest, ask for changes to parts of a model |
| [`hello/`](hello/README.md) | the smallest complete plugin, to copy from |

Link a plugin folder, and the app serves it as it is on disk:

```sh
pinrail plugins install ./plugins/review --link
```

## Layout

```
review/
  manifest.json           # name, version, the schemas and the entry, by path
  README.md
  view/index.html         # what the app serves: the entry, and anything it loads beside it
  schemas/                # payload.schema.json, decision.schema.json, by $ref
  templates/              # decision.md.j2, when the plugin renders its own markdown
  example.json            # a payload that passes payload_schema, shown to agents
  fixtures/               # payloads to develop and test with; *.decided.json with their .md
  tests/                  # the plugin's Playwright spec under the SDK's harness
  src/                    # only for a plugin that builds: the sources; the build writes view/
```

Every path is the manifest's to choose (`entry`, the `$ref`s,
`decision_template`); this is the layout the samples use and the SDK
scaffolds. The bundle the app installs is the folder without `src/`,
`tests/`, `fixtures/`, `node_modules/` and dot-entries.

`manifest.json`:

```json
{
  "name": "hello",
  "version": "1.0.0",
  "title": "Hello",
  "description": "One yes-or-no question with an optional comment.",
  "use_when": "You need a single yes or no from the person, such as whether to push.",
  "payload_schema": { "$ref": "schemas/payload.schema.json" },
  "decision_schema": { "type": "object", "required": ["ok"], "properties": { "ok": { "type": "boolean" } } },
  "example": "example.json",
  "entry": "view/index.html",
  "min_height": 200,
  "dev": false
}
```

- `name` is `[a-z][a-z0-9_-]*`, unique across the installed plugins.
- Schemas are JSON Schema 2020-12, inline or by relative `$ref` to files in
  the plugin's folder. A `$ref` cannot leave the folder.
- `version` is a semantic version, such as `"1.2.0"`; a plugin still
  finding its shape starts at `"0.1.0"`. Bump the major when
  a schema or the view changes in a way an old review would not survive: the app keeps one copy per major, and a review keeps
  rendering and validating from the major it was created under, even
  after the plugin moves on or is removed.
- A folder that is linked rather than copied (`--link`, or the toggle in
  the app) is served live, so a change shows on the next open, and its
  reviews render from the folder as it is now. Remove the link and they
  say the plugin is not installed until it is again.
- `description` says what the plugin is for, and `use_when` the situation
  an agent should ask with it in. `pinrail plugins describe` shows both to
  an agent choosing among the installed plugins, so write `use_when` for
  that reader: *You drafted emails on the person's behalf and need them
  approved before anything is sent.*
- `example` names a JSON file beside the manifest holding a payload that
  passes `payload_schema`. Agents get it as a starting point. One that does
  not pass costs the plugin its example, with the reason on its row.
- `settings_schema` declares settings of the plugin's own, shown as rows under
  the plugin in *Settings › Plugins*. See below.
- `shortcuts` declares the keys your view answers, so the app lists them and
  hands them over whether or not the frame has focus. See below.
- `decision_template` names a file beside the manifest that renders a
  decision as markdown, for the command's output and *Copy as
  markdown* in the app. Without one the app renders the decision by its
  shape. See *Decisions as markdown* below.
- `build` names the command that produces the bundle, for a plugin written
  with a framework: `"build": { "command": "npm ci && npm run build" }`.
  Installing runs it, in a copy, and keeps what it produced. See
  *Installing* below.

## Installing

The app installs one plugin at a time. The source is one string, and its
shape says where the plugin is:

```sh
pinrail plugins install ./plugins/review                                # a folder: a copy, in the app's store
pinrail plugins install ./plugins/review --link                         # a folder served live, while you work on it
pinrail plugins install github.com/acme/plugins/review@v3               # a folder in a repository, at a tag: "this folder, at this version"
pinrail plugins install https://github.com/acme/plugins/tree/v3/review  # the same, as the browser shows it
pinrail plugins install github.com/acme/pinrail-review                   # a repository's root, default branch
pinrail plugins install git@acme.internal:plugins.git --ref v3 --path review   # SSH, the two beside it
pinrail plugins install https://github.com/acme/pinrail-review/releases            # the latest GitHub release: its bundle, no build
pinrail plugins install https://github.com/acme/pinrail-review/releases/tag/v1.2.0 # that release, pinned
```

A ref is a branch, a tag or a commit; without one, the default branch. A
tag or a commit is pinned: checking for updates says so rather than
moving it. GitLab's `/-/tree/<ref>/<folder>` URLs read the same way.

The same sources go into *Settings › Plugins › Install…* in the app, which
looks before it installs: it fetches the source, shows the plugin the
manifest describes, where it comes from, whether a build runs and the
exact command, and what is installed under that name already. *Install*
is the yes. Each plugin's row says where it came from, checks for updates
on request and updates when there is something new, removes the plugin
(a store entry a review still renders from stays), and offers *Install a
copy* on a linked folder once you are done iterating. The CLI does the
same with `pinrail plugins update [name]` and `pinrail plugins remove
<name>`.

A copy lands in the app's store under the plugin's name and major
version, hashed and recorded with where it came from; a review renders
from it from then on. Versions are semantic (`"version": "1.2.0"`):
installing an equal or higher version replaces
the line in place, an older one is refused unless `--force`, and a new
major is a new line beside the old, which stays while a review still
renders from it. A major is the compatibility promise: every review
created under it renders with the latest copy of it, which is the one
with the fixes.

On disk, under the data directory, `plugins/` holds three folders:
`store/<name>/<major>/` for the installed copies, `fetch/` for the
scratch an install uses and empties, and `logs/` for the last few build
logs of each plugin.

A plugin that is built — React, Svelte, anything that compiles — declares
the command in its manifest, and never relies on the app guessing:

```json
"build": { "command": "npm ci && npm run build" }
```

Installing copies the folder without `node_modules` and `.git`, runs the
command there through the shell, shows its output as it comes, and keeps
the log; a non-zero exit stops the install with the log's tail. Whatever
the command needs (`node`, `pnpm`, `deno`) must be on the `PATH`. Only the
bundle enters the store: `src/`, `tests/`, `fixtures/`, `node_modules/`,
dot-entries and the package and tool config files stay behind. A manifest
without `build` whose `entry` is missing is refused with that said.

### What runs on your machine

A plugin's view always runs in a sandbox, however the plugin was
installed. Installing from a source that needs a build runs the build
command on the person's computer with their permissions, while installing
a release runs nothing. See [What runs where](../docs/concepts/trust.md)
for details.

### Publishing a release

A GitHub release spares the people installing your plugin the toolchain:
the app downloads its asset, checks it and serves it as it is, and never
runs a build. The release carries one `.zip` that is the bundle (or, with
several zips attached, one named `pinrail-plugin.zip`), with
`manifest.json` at the archive's root or inside the one folder there, as
most zip tools lay it out. The tag is the manifest's version, with or
without a leading `v`; a release tagged `v1.2.0` whose manifest says
`1.1.0` is refused. Installing from `/releases` follows the latest
release, and checking for updates compares its tag with what is installed;
installing from `/releases/tag/<tag>` pins that one.

This repository's workflow in `.github/workflows/plugin-release.yml` does
it for the plugins here: a tag `plugin-<name>-v<version>` builds the
plugin when its manifest says so, zips the bundle as
`<name>-<version>.zip` and attaches it to the release of that tag. For a
plugin of your own the recipe is the same three lines: build, zip the
folder without its sources, attach.

## Decisions as markdown

An agent that ran `pinrail submit … --wait` reads the
decision as prose: the title, where the review sits, who decided and
when, the reviewer's note, then the decision. The app renders
the decision by its shape: every array of objects becomes a headed list,
an `id` and an `action` or `verdict` lead each bullet in bold, a `file`
and `line` or a `selector` come next in backticks, a `title`, `subject`,
`text` or `body` follows a dash, a `note` becomes a nested quote, `edits`
read as before → after, and `undecided` is one line. Anything it does not
recognise prints as the key and its JSON, so nothing is dropped.

A plugin whose decision needs the payload to read well ships a template:

```json
"decision_template": "templates/decision.md.j2"
```

A [MiniJinja](https://docs.rs/minijinja) file beside the manifest that
renders the body only; the head stays the app's, so every plugin's
output starts the same way. The context: `review` (the envelope with its
payload), `decision`, `data` (the decision's data), `note`, and `items`,
every object in the decision's arrays with a `payload` field holding the
payload object of the same `id`, so `{{ item.payload.title }}` sits next
to `{{ item.action }}`. The code review plugin's `decision.md.j2` is the
example: each proposal by file and line with its verdict and note. A
template that does not compile is dropped with the reason on the
plugin's row, and one that fails while rendering falls back to the
rendering by shape.

### Adding a template

1. Write `templates/decision.md.j2`. Start from what the
   rendering by shape gives you (`pinrail show <id>` on
   a decided review) and improve the bullets that need the payload. The
   syntax is Jinja's: `{% for item in items %}`, `{% if item.note %}`,
   `{{ item.payload.title }}`, filters such as `selectattr`, `length`,
   `join`, and `verb`, which turns an action into the word the generic
   body uses (`accept` → `accepted`). Use `{%-` and `-%}` to keep blank
   lines out. The context is the four names above; anything else is
   undefined and renders empty.
2. Name it in the manifest: `"decision_template": "templates/decision.md.j2"`.
3. Reload. The manifest and the template are read when the plugin loads,
   so after an edit run `pinrail plugins reload` (or *Reload* in Settings
   › Plugins). A copied plugin needs installing again; a linked one only
   the reload.
4. Look at the row. `pinrail plugins` lists `template_error` on the
   plugin when the template does not compile, with the line, and the
   row in Settings says the same.

### Testing a template

The quickest loop is a decided review in the running app: create one
from a fixture, decide it, and read it back.

```sh
pinrail submit review --data <(jq .payload fixtures/dedup-round-2.json) --title "Template check" --json > /tmp/r.json
pinrail decide "$(jq -r .id /tmp/r.json)" --data <(jq .decision.data fixtures/dedup-round-1.decided.json) --note "looks right"
pinrail show "$(jq -r .id /tmp/r.json)"
```

A decided fixture (`fixtures/<name>.decided.json`: `title`, `payload`,
`decision`, and optionally `origin` and `agent_note`) is worth keeping
for exactly this. For the plugins in this repository the core's tests
render every one, through the plugin's template when it has one, and
compare the result with the `<name>.decided.md` beside it, byte for
byte, with times in UTC so the file holds anywhere. Change the template
or the renderer and the test shows the diff; when the new output is the
intended one, `UPDATE_FIXTURES=1 cargo test -p pinrail-core --lib
decided_fixtures` rewrites the expected files, and the diff of those
files in the commit is the review of the change. For a plugin outside
this repository, the three commands above in a script against a scratch
data directory (`pinrail serve` with `PINRAIL_DATA_DIR` set), diffed
against a checked-in `.md`, are the same test.

## Settings of your own

A plugin with knobs — a diff shown inline or side by side, how files are
ordered — declares them in the manifest, and the app draws a row for each
under the plugin in *Settings › Plugins*:

```json
"settings_schema": {
  "type": "object",
  "properties": {
    "diff": { "type": "string", "title": "Diff", "description": "How a file's changes are laid out",
              "oneOf": [{ "const": "inline", "title": "Inline" }, { "const": "split", "title": "Side by side" }],
              "default": "inline" },
    "wrap": { "type": "boolean", "title": "Wrap long lines", "default": true },
    "context": { "type": "integer", "title": "Context lines", "minimum": 0, "maximum": 20, "default": 3 }
  }
}
```

It is a JSON Schema, inline or by relative `$ref` like the other two, one
level deep: every property is a `boolean`, a `string`, an `integer` or a
`number`, and every property has a `default`. `title` is the row's label,
`description` the line under it, and property order is row order. A string
with an `enum`, or a `oneOf` of `const` values with titles, becomes a choice;
a number with `minimum` and `maximum` keeps to them. A schema that breaks
these rules does not break the plugin: it loads without settings and the
row in Settings says why.

The values live in the app's `settings.json` under `plugins.<name>`, and
`PATCH /api/v1/settings` with `{"plugins": {"review": {"diff": "split"}}}`
changes one; the app checks the change against your schema. The plugin's
row in `GET /api/v1/plugins` carries the schema and the values as they
stand.

The view gets them in `init` as `settings`, every key with its value, and
again as a `settings` message whenever they change; `plugin.settings` holds
them and `onSettings` fires on a change. A control in the view writes one
back with `plugin.setSetting("diff", "split")`: the app checks it against
the schema, keeps it, and every open view of the plugin hears the new values
— the toolbar in the view and the row in Settings are one control. A value
the schema refuses comes back as `violations`.

## Keys of your own

A view that answers keys says so in the manifest, and two things follow:
the app's keyboard-shortcuts dialog (`?`) lists them under your plugin
while one of its reviews is open, and the app forwards them to your view
when the person presses them with the app rather than the frame in focus —
after clicking the top bar, say, or arriving from the inbox.

```json
"shortcuts": [
  { "keys": "j", "does": "Next proposal" },
  { "keys": "a", "does": "Accept the focused proposal", "group": "Verdicts" },
  { "keys": "cmd+shift+f", "does": "Fold every file" }
]
```

`keys` is modifiers (`cmd`, `ctrl`, `alt`, `shift`) joined by `+` and one
key, named as `KeyboardEvent.code` spells it with `Key`/`Digit` dropped:
`j`, `1`, `/`, `enter`, `escape`, `arrowdown`. A bare key is fine. `does`
is the one-line label; `group` puts entries under a caption. Order is
display order. A key the app already uses on the review screen — `?`, `t`,
`[`, `]`, `esc`, ⌘⇧M, ⌘⏎ and the ⌘ keys of its menu — stays the app's: the
dialog says so next to your entry, and the key is not forwarded. The
app's dialog lists your keys, so a view needs no help overlay of its own.

A forwarded key arrives as a `keydown` on your document, exactly as a press
inside the frame would, so the listener you already have handles both.
Only declared keys are forwarded; a view that declares none gets none.

## Looking like the rest of pinrail

The app serves a stylesheet next to the SDK. Link it and your view gets the
app's tokens in both themes, the base typography and scrollbars, and a small
vocabulary of classes:

```html
<link rel="stylesheet" href="/sdk/v1/pinrail-plugin.css">
```

It also gives you icons. The app serves the [Lucide](https://lucide.dev) set a
file at a time at `/sdk/v1/icons/<name>.svg`, and `Pinrail.icon("check")` writes
the markup for one. Any name in the set works and you download only the ones
you name, so the set costs your view nothing. Icons take `currentColor`, so
they follow the theme along with the text around them.

`Pinrail.layout()` builds that skeleton for you, and hands back the elements
rather than markup, so the header and its controls keep their listeners while
you rewrite the body on every change:

```js
const view = Pinrail.layout({ title: "5 items", controls: [acceptAll, clear] });
view.content.innerHTML = rows;          // render into this
view.title("4 items").meta(["acme-api", "7 days"]);
```

Ask for a header by passing `title`, `meta`, `controls`, or `header: true`;
without any of them you get a body and nothing else. Pass `into` to build it
somewhere other than `<body>`. Writing the markup yourself with the classes
below works just as well, which is what a view with a header of its own
should do.

The body scrolls, not the document, so a `.plugin-subhead` inside it pins
directly under the header without having to know how tall the header is. The
frame still shrinks to a short view: the SDK measures the header and the body
rather than the document.

| Class | For |
|---|---|
| `.plugin-header`, `.plugin-title`, `.plugin-meta`, `.plugin-controls` | a bar that stays at the top of the frame |
| `.plugin-scroll`, `.plugin-content` | the body that scrolls, and the padded area inside it |
| `.plugin-subhead` | a heading in the body that pins under the header while its section is on screen |
| `.plugin-footer` | a bar that stays at the bottom of the body |
| `.item` with `.head`, `.id`, `.title`, `.body`, `.controls` | one thing the human says yes or no to |
| `.sev` with `.sev-blocker`, `.sev-major`, `.sev-minor`, `.sev-nit` | severity, in the app's four levels |
| `.btn` with `.primary`, `.ghost`, `.danger`, and `aria-pressed` | buttons |
| `.field`, `.note` | inputs and textareas |
| `.meta`, `.eyebrow`, `.dim`, `.faint`, `code.inl`, `kbd` | small text and chips |
| `.notice` with `.ok`, `.warn`, `.danger`, plus `.errors`, `.empty` | something to tell the human |

**These are defaults, not rules.** Your own `<style>` comes after the
stylesheet, so anything you write wins, and a view that needs a shape this
does not have should write it.

What you get by starting here is that the palette follows the app. When its
colours change your view changes with them, in both themes, and your bundle
carries no copy of them to keep in step.

`v1` in the path is the protocol major and only ever receives corrections: a
gate decided months ago still loads it, and it must render then as it did on
the day it was decided.

## The hand-over belongs to the shell

A view does not render its own submit button. The shell puts one control next
to the note box, in the same place for every gate, and pressing it (or
⌘/Ctrl+Enter anywhere) sends `collect`. Your view decides what that means: it
may submit at once, or show what would go back and submit on the next
`collect`. Say what the button should read with `status`, and the shell keeps
it disabled while a decision is in flight or the socket is down.

This is why a decision that is only a choice, such as yes or no, is held in
the view as state rather than as two submitting buttons: the human picks, then
hands over, and nothing leaves on a single click.

One field of the decision the shell does read: a top-level `verdict` of
`"approve"` or `"revise"`. A decided review with one shows as *approved* or
*changes requested* in the history, the search and the review's bar, instead
of a bare *decided*. Any other short string is shown as it is; without the
field the shell says *decided*. The `artifact` plugin uses it.

## Sandbox

The bundle loads in `<iframe sandbox="allow-scripts">` with a Content
Security Policy of `default-src 'none'` and `connect-src 'none'`. Scripts,
styles, images and fonts must be inline or files inside the plugin
directory. No fetch, no web fonts, no CDN. Everything the view needs must be
in the payload.

## The SDK

The app serves the plugin side of the protocol at `/sdk/v1/pinrail-plugin.js`.
Load it and let it do the handshake; the view only renders:

```html
<script src="/sdk/v1/pinrail-plugin.js"></script>
<script>
  const plugin = Pinrail.connect({
    resize: "auto",                                  // or "fill" for a viewport-height frame
    onInit({ gate, previous, readonly, draft }) { render(); },
    onViolations(errors) { showErrors(errors); },
    onSubmitted(decision) { render(); },             // read-only from here on
    onCollect() { submit(); },                       // ⌘/Ctrl+Enter
  });
  plugin.submit(data);
  plugin.draft(data);                                // debounced; { flush: true } posts now
</script>
```

`v1` only ever receives fixes. See [`pinrail-plugin/`](../pinrail-plugin/README.md),
the `pinrail-plugin` package, for the API, the helpers (`escape`, `markdown`,
`previousVerdict`), the dev shell and the test harness. The protocol below is what the SDK implements; a plugin can
speak it directly instead.

## Starting a plugin

`pinrail plugins new <name>` writes a folder in the layout above with a
working view, schemas, a sample and the SDK's types, and `--link` installs
it at once. For a view built with Vite, React, Vue or Svelte, the SDK's
`create` writes the same with a build, a test and a release workflow; see
[`pinrail-plugin/`](../pinrail-plugin/README.md). The samples here are what
to read once it runs.

## Testing a plugin

Ship `fixtures/*.json` (a partial gate: `title`, `payload`, optionally a
`decision`) and `tests/*.spec.ts` that mount the view alone under the
harness in `pinrail-plugin/testing`; `npm test` here (`mise run test:plugins`
from anywhere) runs them for every plugin in this folder and for the
built-in `list`, with Playwright from this folder's `package.json`. See any
shipped plugin for the pattern.

While building one, `mise run dev:plugin <directory>` (`npx pinrail-plugin
dev <directory>` outside this repository) opens the view in a browser under
a shell of its own: pick a fixture, collect a decision, read what the view
posts, and see every file change reloaded. No app needed; the app can link
the same folder meanwhile with `pinrail plugins install <directory> --link`.

## Protocol

All messages are `{ "pinrail": 1, "type": "...", ...fields }` over
`postMessage`. The plugin posts `ready` once its listener is installed; the
shell answers with `init`. The shell only trusts messages whose source is the
iframe; the plugin should remember `shell_origin` from `init` and ignore
other origins.

Shell → plugin:

| type | fields |
|---|---|
| `init` | `gate` (the full envelope, payload included), `previous` (the superseded gate's envelope or null), `readonly`, `draft` (what the plugin last posted as a draft, or null), `shell_origin`, `capabilities` (`["attachments"]` when the shell hands over files) |
| `attachment` | the answer to your `attachment`, with its `req`: `ok: true`, `name`, `media_type`, `size` and `bytes` (an `ArrayBuffer`, transferred), or `ok: false` and `error` |
| `violations` | `errors: [{path, message}]`, JSON pointers into the rejected decision |
| `submitted` | `decision` – the decision was accepted; render read-only |
| `collect` | the human asked to hand the gate over, with the shell's button or ⌘/Ctrl+Enter. Assemble the decision and submit it, or show a confirmation first and submit on the next `collect` |
| `appearance` | `theme: "dark" \| "light"` – the shell's theme, sent before `init` and again on every change. The SDK applies it as `data-theme` on your root element; write the CSS and you are done |
| `key` | `key`, `code`, `metaKey`, `ctrlKey`, `altKey`, `shiftKey` – one of the manifest's `shortcuts`, pressed while the app rather than your frame had focus. The SDK dispatches it as a `keydown` on your document (with `pinrailForwarded: true` on the event), so a view that listens for its keys needs no change; `onKey` is there as well |

`readonly` is true whenever the gate is not pending, and `gate.status` says
why: `decided`, `withdrawn` (the requester took it back), `discarded` (the
person said no and told the agent to stop; `discarded_by` and
`discarded_reason` on the envelope say who and why) or `expired`. A view
renders the same way for all four: what was there, nothing to submit.

Your frame's URL also ends in `#pinrail-theme=dark` or `#pinrail-theme=light`.
A message cannot reach your view before it paints, so this is how the first
theme gets there in time; the SDK reads it as it loads. Read it yourself if
you do not use the SDK.

Plugin → shell:

| type | fields |
|---|---|
| `ready` | – |
| `resize` | `height` in px; the shell sizes the iframe, the page scrolls. `"fill"` instead asks for a viewport-height frame that scrolls inside, for workbench-style views such as `review` |
| `draft` | `data`; the shell keeps it in sessionStorage and hands it back in `init` |
| `submit` | `data`; validated against `decision_schema` server-side |
| `status` | `label`; what the shell's hand-over button should read right now, e.g. "Hand over anyway" once you have warned about something |
| `settings_set` | `patch`; the values of your own settings to keep. The core checks them against `settings_schema` and everyone hears the result as `settings` |
| `attachment` | `req`, `name`, and `round: "previous"` for the superseded gate's file; asks for the bytes of a file the gate lists in `attachments`. Only a plugin whose manifest declares `attachments` gets files; `plugin.attachment(name)` does this for you |
| `open` | `url`; a link to open outside the app. Your frame is sandboxed and can open nothing itself, so a click on an `a[href]` becomes this message and the shell opens it in the system browser. Only `http`, `https` and `mailto` travel; the SDK does this for every link in your view, and `plugin.open(url)` asks for it from code |

The decision schema is the whole contract. What the fields mean is between
the plugin and the workflow that reads the decision. `hello/index.html` is
the smallest complete client; `plugins/list/view/index.html` is a
full one with drafts, read-only rendering and a previous-round overlay;
`review/index.html` is a workbench-style one that fills the viewport.
