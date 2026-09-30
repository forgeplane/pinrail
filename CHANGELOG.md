# Changelog

All notable changes to the Pinrail app are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Changes are grouped by kind. An *Unreleased* section is added while changes
wait for a release, and takes the version's name when it is tagged.

The `pinrail` command ships inside the app and carries the app's version.

## [Unreleased]

### Added

- Send feedback to the Pinrail team from the app, with **Help › Send
  Feedback…** or the command palette. A report has a reply address, a
  subject and a message, and can include up to five attached files, which
  you can choose, drop or paste.

### Fixed

- The AppImage starts on newer Linux systems, such as Arch and Fedora 44,
  where it aborted with "Could not create default EGL display".
- Dragging the empty space of a review's top bar moves the window, as it
  does on every other screen.

## [0.1.0]

Initial version of Pinrail.

[Unreleased]: https://github.com/forgeplane/pinrail/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.0
