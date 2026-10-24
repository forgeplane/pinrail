# Changelog

All notable changes to the Pinrail app are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Changes are grouped by kind. An *Unreleased* section is added while changes
wait for a release, and takes the version's name when it is tagged.

The `pinrail` command ships inside the app and carries the app's version.

## [0.4.1]

### Added

- The markdown plugin has a Changes view for a new version of a document.
  It shows what changed since the previous round, with the changed words
  marked and the unchanged lines folded. Your previous comments appear
  at their place, marked by whether their lines changed, so you can
  check how they were addressed without reading the document again.

### Changed

- The official plugins share one comment field: a box with the text,
  the microphone and the add button at its end. Once the text wraps,
  the buttons move to a row under it, with the keys that add the
  comment and start a new line.
- A saved comment in the official plugins has edit and delete buttons
  at the right of its header, shown while the pointer is over it or the
  focus is in it.
- Comment fields in plugin views that use the SDK's comment field have
  spell checking, autocorrection and automatic capitals off.
- In the markdown plugin, the comments panel shows this round's and the
  previous round's comments in two tabs.

### Fixed

- Going back from the side panel now closes it after two reviews were
  clicked in quick succession, in the inbox and in the history.
- Fix plugin views that compare with the previous round, such as the
  image plugin's comparison, in the app. They now receive the previous
  round with its payload.

### Security

- Updated tokio, whose `AsyncFd` constructors accepted a file descriptor
  the caller might not own.

## [0.4.0]

### Added

- A plugin can ship a skill of its own in a `skill/` folder, which
  Pinrail writes into each connected agent's `pinrail` skill under
  `plugins/<name>/`, and the `pinrail` skill names the plugins that have
  one. Installing, updating or removing a plugin rewrites the connected
  agents' skills. Every plugin's skill is off until you turn it on in the
  plugin's details in Settings › Plugins, where you can also open the
  skill's folder to read it, or with `pinrail plugins install
  --allow-skill`. A plugin's row shows whether its skill is on.
- `pinrail plugins skill <name>` prints a plugin's skill, or a file it
  names, for an agent without the `pinrail` skill installed, and
  `pinrail plugins` says which plugins have one.
- The change map plugin, `change-map`, among the official plugins. It
  shows a code change, such as a pull request, as a diagram of the parts
  it touches and how they interact, with each part's diff, screenshots
  before and after or recordings of the interface, and optional sequence
  and state diagrams. Comments go
  on any of them. Its skill tells your agents how to build a map from a
  change. Install it in Settings › Plugins.
- The history opens a review in a panel beside the list, as the inbox
  does. J and K step through the history from it. *Settings › General ›
  Opening a review* applies to both.
- *Settings › Agents › Update skills automatically* updates the skill of
  each connected agent when Pinrail starts, after an update of Pinrail
  changed it. It is off by default.
- Support for Hermes Agent. Settings › Agents installs the `pinrail`
  skill to `~/.hermes/skills`, and reviews submitted from Hermes Agent
  show its name and icon.
- Plugins can copy to the clipboard. A plugin that declares the
  `clipboard` capability can copy the text or images of a review, and the
  app shows a notice for each copy.

### Fixed

- The inbox beside the side panel fits its width. A narrow list hides
  each review's plugin and the sort order, and a review's details no
  longer run into its plugin's name.
- In the inbox, the outline of the review under the pointer no longer
  runs over the review open beside it.
- Discarding the review open in the side panel opens the next one, as a
  decision does, whether from the review or from its row in the inbox.
- The keyboard shortcuts dialog lists the keys of the plugin open in the
  side panel, as it does for a review open on its own page.
- Dictation fills text fields in plugin views built with React, so a
  button that waits for text, such as Add comment, is enabled as you
  dictate.

### Security

- Updated rustls-webpki and webpki-roots, which could trust certificates
  from the distrusted emSign authorities, and markdown-it, which could
  take quadratic time to parse some Markdown.

## [0.3.0]

### Added

- Support for Amp, Augment CLI (Auggie), Cline, GitHub Copilot CLI,
  Goose, Kiro CLI and Pi. Settings › Agents installs the `pinrail` skill
  in each agent's skill folder, and reviews submitted from these agents
  show the agent's name and icon.
- What's new: the first time a new release starts, Pinrail shows a few
  cards about what changed, one at a time, each with a screenshot and a
  button that opens the change. A release without notes shows nothing,
  and a fresh install starts counting from the release it installed. The
  newest notes open again from Settings › About, Help › What's New in
  Pinrail and ⌘K.
- Pinrail records whether a decision or a discard was made in the app,
  with `pinrail decide` or `pinrail discard`, or by another program
  through the API, and in which agent session. The review page, the
  history, `pinrail show` and the decision an agent receives say so when
  it was not made in the app. The review page warns when the agent
  session that submitted a review also decided it.
- The history shows who withdrew or discarded a review and how, with the
  reason they gave in a tooltip. Pinrail records the agent that withdrew
  a review, as it does for a discard.
- In the macOS app, the note to the agent can be dictated: the
  microphone in the note box turns speech into text as you speak, in
  your system's language. macOS asks once for access to the microphone
  and to speech recognition.

### Changed

- Settings › Agents groups the agents into connected, not connected and
  not found, and sorts each group by name. The agents not found are
  folded away. The setup wizard also lists the agents it found by name.
- Pinrail installs its skill to OpenCode's and Grok CLI's skill folders,
  `~/.config/opencode/skills` and `~/.grok/skills`, even when Claude Code
  or Cursor already has it.
- An app update now updates the installed official plugins to the
  versions it carries, when Pinrail starts. Until now, Settings › Plugins
  offered each update and waited for you to apply it. Reviews keep the
  version they were submitted to.
- A plugin can no longer declare J or K as shortcuts. They are the
  app's keys for moving through the inbox, and `pinrail plugins check`
  refuses a manifest that declares them. The official plugins move
  between items with N and P instead.
- The inbox opens a review in a panel beside the list. The panel opens
  the review as a page when the view needs more room, closes with its
  button or Esc, and moves on to the next review after a decision. J
  and K step through the list from it, while the view has the keyboard
  too. *Settings › General › Opening a review* switches back to opening
  reviews as a page.
- Esc closes the inbox's panel from inside the review's view too. When
  the view has something of its own open, such as a comment, the first
  Esc closes that and the next closes the panel. A plugin that uses Esc
  no longer stops it from closing the panel.
- J and K go round at the ends of the inbox: J on the last review goes
  to the first, and K on the first to the last, in the list and in the
  panel.
- The official plugins fit a narrow frame, such as the inbox's side
  panel. Their sidebars, such as the code review's file tree and the
  Markdown review's outline and comments, open as drawers over the
  review, and their headers and controls rearrange to leave the content
  the width. The image review's notes fold away, and the code review has
  arrows for the next and previous proposal.
- Only a decision or a discard made in the app is recorded as yours. One
  made with `pinrail decide` or `pinrail discard` is recorded as made by
  the agent the command runs under, if any, and one made through the API
  by the name the program gives, if any.

### Fixed

- On Linux, an update of the AppImage reliably updates the copy of the
  `pinrail` command it installed. The check that the copy is the
  `pinrail` command could fail when the file had just been written.

## [0.2.0]

### Added

- ⌘F (Ctrl+F on Linux) opens a find bar on a review that searches the
  text of the plugin's view and highlights every match. Enter and
  Shift+Enter move between the matches, and Esc closes the bar. It works
  in every view that uses the plugin SDK, with no change to the plugin.
- The code review view highlights the syntax of the diff, of suggested
  changes and of code blocks, in the language of each file. If you
  installed code review, update it to 1.1.0 in *Settings › Plugins*.

### Changed

- ⌘D (Ctrl+D on Linux) discards a review, from its row in the inbox or
  from the review itself, including while the plugin's view has the
  keyboard. It replaces D, which worked only in the inbox. A plugin that
  declares ⌘D as one of its shortcuts no longer receives it.

### Fixed

- A review created while the app was loading the inbox no longer
  disappears from the inbox until the app is reloaded.

## [0.1.3]

### Fixed

- A review's notification closes when you open the review in the window
  or it stops waiting. On Linux, notifications stayed in GNOME's list, and
  Ubuntu Dock kept counting them on the app's icon.
- On Linux, the menu bar icon is white, so it can be seen on GNOME's dark
  top bar.

## [0.1.2]

### Changed

- `pinrail plugins new --playwright` takes the plugin SDK from npm, as
  `pinrail-sdk`, rather than from the tarball on the SDK's GitHub release.

### Fixed

- On macOS 26, the button that shows and hides the sidebar no longer
  touches the window's close, minimize and zoom buttons.
- The button that shows and hides the sidebar stays at the top when the
  sidebar scrolls.

## [0.1.1]

### Changed

- A review's *Discard*, *Copy decision as markdown* and *Maximize* buttons
  are at the right of the review's header, beside its status, rather than
  in the window's top bar.

### Fixed

- In a window too narrow for the sidebar, the page now takes the whole
  width, and the window's buttons no longer cover the top bar on macOS.

## [0.1.0]

Initial version of Pinrail.

[0.4.1]: https://github.com/forgeplane/pinrail/releases/tag/v0.4.1
[0.4.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.4.0
[0.3.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.3.0
[0.2.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.2.0
[0.1.3]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.3
[0.1.2]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.2
[0.1.1]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.1
[0.1.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.0
