# Changelog

All notable changes to Pinrail are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Changes are grouped by kind, and inside a kind by the area they touch:
Desktop app, CLI, Plugins, SDK. An *Unreleased* section is added while
changes wait for a release, and takes the version's name when it is tagged.

The `pinrail` command ships inside the app and carries the app's version.
The [`pinrail-plugin`](pinrail-plugin/CHANGELOG.md) package is versioned on
its own, and its major is the plugin protocol's.

## [0.1.0]

The first release of Pinrail.

### Added

#### Desktop app

- An inbox of the reviews that agents submit, grouped by project, and a history of every review that has ended.
- Each review is shown in its plugin's view, where you decide and hand the decision back to the agent.
- Rounds: an agent can submit a new version of a review, which is shown beside your earlier verdicts.
- System notifications, and an icon in the menu bar or the tray with the number of waiting reviews.
- Plugins installed from a folder, a Git repository or a GitHub release, and updated from the same source.
- Automatic updates for the macOS app and the AppImage.
- Packages for macOS 13 or later, and for Linux as an AppImage, a `.deb` or an `.rpm`.

#### CLI

- `pinrail submit` sends a review and waits for the decision. It prints the decision as Markdown for an agent or as JSON for a script, and ends with an exit code for each outcome.
- `pinrail plugins` and `pinrail plugins describe` show which plugins are installed and what each one expects.
- `pinrail docs` prints the briefs that explain Pinrail to an agent.
- `pinrail submit --attach` sends files with a review.

#### Plugins

- Built into the app: `list` and `feedback`.
- Optional, installed from this repository: `review`, `email`, `artifact`, `calendar`, `logo` and `model`.

[0.1.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.0
