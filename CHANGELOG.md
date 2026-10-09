# Changelog

All notable changes to the Pinrail app are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Changes are grouped by kind. An *Unreleased* section is added while changes
wait for a release, and takes the version's name when it is tagged.

The `pinrail` command ships inside the app and carries the app's version.

## Unreleased

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

[0.1.2]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.2
[0.1.1]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.1
[0.1.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.0
