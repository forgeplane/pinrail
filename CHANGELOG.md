# Changelog

All notable changes to the Pinrail app are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Changes are grouped by kind. An *Unreleased* section is added while changes
wait for a release, and takes the version's name when it is tagged.

The `pinrail` command ships inside the app and carries the app's version.

## [Unreleased]

### Added

- Work items. Record tasks in Pinrail, with sub-items and dependencies,
  on the new Work items page. Each repository numbers its items under a
  key prefix, such as `PIN-42`. Agents add, take and close them with
  `pinrail items`. The reviews and questions an agent sends belong to
  every item it holds, or to the items it names with `--item`. A review
  can belong to several items, which its header lets you change.

- Projects. Group repositories into a project on the new Projects page,
  by dragging them under it. A project has a name, an optional
  description, and an avatar with a colour and an emoji or up to three
  letters. Agents need no change: a review or a session names its
  repository, and shows under the project that repository is in.
- Group the inbox, the sidebar's sessions and the Sessions page by
  project. A repository in no project is a group of its own. The Sessions
  page can also group its sessions by repository.
- Filter the inbox, the history, the Sessions page and the sidebar's
  sessions by a project or a repository, from a menu that you can type
  in.
- Go to the projects, create one, or show a project's reviews from the
  command palette.
- Delete a review, with every round of it, from the **…** menu of its row
  in the history, from its header or from the command palette. A round
  that still waits is discarded first, so the agent stops.
- Delete a repository from the Projects page. Its reviews, questions and
  sessions are deleted, and what still waits is discarded first, so the
  agents waiting on it stop.
- Press `/` to search the Projects page and the official plugins in
  Settings, as in the inbox and the Sessions page.

- Find text in a session's thread with ⌘F, as in a review's view.
- `pinrail ask --suggest-reply` suggests the reply the agent expects. The
  empty comment box shows it, and Tab takes it.
- ⌘1 to ⌘4 go to the Inbox, Sessions, Work items and Projects pages.

### Changed

- The keyboard shortcuts dialog lists the keys in sections, those of the
  page you are on first. Show all shortcuts lists the others, and a search
  looks through every section.
- The inbox's layout is kept in the settings, so every window shows the
  same one.
- Where the inbox, the history and the Sessions page said "project" for
  a repository, they now say "repository".
- The history's star moved into the **…** menu at the end of each row. A
  starred round still shows its star.
- The inbox's Pending and New rounds tabs count what the filters show,
  and its title reads "3 of 11" while a filter is set.

### Fixed

- `pinrail` stops quietly when the program reading its output exits,
  such as `head`, as other command-line tools do.
- A long reason for a withdrawn review now wraps at the width of the
  review cards in a session's thread.
- At the large interface size, menus, selects and tooltips open next to
  their button rather than off to the side.
- The app shows a setting changed from the command line or another
  window while it is saving a change of its own.
- On a session's page, what you click in the thread stays in place when
  the side panel opens or closes beside it.
- A note that shows `attachment:` links inside code, as an example, is no
  longer refused for files it does not carry.
- Esc pressed as soon as What's new opens closes it.
- Esc in the search field of the inbox, the Sessions page or the
  Projects page leaves the field and keeps the search, so the page's keys
  act on the results.

## [0.5.0]

### Added

- Themes. Add a theme as a JSON file in the themes folder, and choose a
  theme for dark and for light in Settings or in the command palette.
  Plugin views follow the theme.
- Add the Nord, Dracula, Catppuccin Mocha, Catppuccin Latte and Solarized
  Light themes.
- Settings › Labs, with features still in the works, each off until you
  turn it on. Questions and sessions are the first: while they are off,
  the sidebar lists the reviews that wait and the projects, the app
  refuses `pinrail ask` and `pinrail status`, and the skill leaves them
  out.
- Add a button to copy the session id in a session's header.
- The Help menu links to the documentation and the blog.
- `pinrail status` prints the current stage, or says that no stage is open.
- `pinrail ask` asks you one or more questions and prints your answers.
  Each question can offer choices, ask for yes or no, or take an answer
  in words, and you answer in the app.
- Questions open in a thread of the agent's session, beside its earlier
  questions and reviews. Pick a choice or answer in your own words, add a
  note to an answer, and comment on all the questions at once.
  A revised review stays where it was first sent and names its round,
  and a line marks where each later round was sent.
- A session's thread shows a line when the agent withdraws a review, with its
  reason. The line leads to the review.
- Links in the statuses an agent reports open in your browser, and links
  to this computer in a session's thread open too, such as a dev server.
- A Sessions page lists the agent sessions that asked you questions or
  sent reviews, those waiting for you first.
- The sidebar lists every session that is not archived under its
  repository, with what waits under the session that sent it. A mark on the
  agent's icon shows a session that works or has finished, and a menu
  groups, orders and filters the sessions.
- Drag the sidebar's edge to resize it. Pinrail remembers the width.
- Hide the session's details beside its thread with the header's button
  or ⌘⇧B, and drag their edge to resize them.
- The markdown plugin can hide its comments with the button at the right
  of its header, so the document takes their width.
- Questions in the inbox are marked apart from reviews.
- A question's text can use inline Markdown, such as `code` and **bold**,
  which the thread shows formatted.
- `pinrail status` lets an agent say what it is doing. Its statuses show
  in the session's thread between its questions and reviews, and the
  newest shows on the Sessions page.
  `pinrail status --start` names the stage of work the agent starts,
  which becomes the session's title. The session details list its latest
  8 stages and fold the earlier ones.
  `pinrail status --note` sends a note: a message in Markdown, with files
  such as a chart or a log, that shows in the thread and asks nothing.
- Code in a note or a question's context is highlighted when it names its
  language.
- The session details show the project, the branch and the folder the
  agent works in. The CLI now sends the folder with every review.
- The Sessions page opens a session's thread in the side panel, as the
  inbox opens a review.
- The Sessions page filters the sessions by project.
- A review sent from a session links to the session's thread from its
  header. On the session's page, a review opens in a side panel beside
  the thread. A review opened as a page from the session names the
  session in its breadcrumb, and deciding it returns to the thread.
- Archive a session from its header, its row on the Sessions page or
  with E. Show archived lists archived sessions with the others, and new
  activity from the agent brings a session back.
- The command palette finds the stages agents started, and opens the
  session's thread where a stage started.
- Star a review to find it again later, from its header, with ⌘⇧S, or
  from its row in the inbox. The inbox's star filter lists the starred
  reviews.

### Changed

- The history moved to a History tab in the inbox, with the same rows and
  filters as the other tabs. ⌘⇧H opens the tab.
- The `pinrail` skill tells agents to ask their questions through Pinrail
  with `pinrail ask`, not in the chat, and to report their progress with
  `pinrail status`.

### Fixed

- Pressing ⌘B twice in quick succession hides the sidebar and shows it
  again, and the next start keeps it shown.
- A change made as the app starts, such as ⌘B, no longer resets the other
  settings, which could open the setup again.

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

[0.5.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.5.0
[0.4.1]: https://github.com/forgeplane/pinrail/releases/tag/v0.4.1
[0.4.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.4.0
[0.3.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.3.0
[0.2.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.2.0
[0.1.3]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.3
[0.1.2]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.2
[0.1.1]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.1
[0.1.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.0
