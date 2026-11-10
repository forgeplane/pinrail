# Changelog

This plugin is released with the Pinrail app, which carries it among its
official plugins. Each version names the release of the app that first
carries it.

## [1.1.2] - Unreleased

### Fixed

- Text typed in the comment field of one part, and not yet added, no
  longer stays in the field when you select another part.
- After you hand the review over, the view shows it read-only. The
  verdict, the comment fields and the reviewed toggles stayed editable.
- A comment typed and not yet added, on a part, a line or a screen,
  goes with the decision when you hand the review over. It was left out,
  and with no other comment the review was approved.

## [1.1.1] - Pinrail 0.5.0

### Fixed

- The count on a tab uses the theme's colour for text on filled buttons,
  so it stays readable in a theme with a light accent colour.

## [1.1.0] - Pinrail 0.4.1

### Changed

- A saved comment has edit and delete buttons at the right of its header,
  shown while the pointer is over it or the focus is in it. A click on the
  comment's text still edits it.
- The comment field is drawn by the SDK's stylesheet, which the other
  official plugins share. It looks and works as before.

## [1.0.0] - Pinrail 0.4.0

The first version. It shows a code change as a diagram of its parts and
their interactions, with each part's files in a diff drawer, screenshots
before and after, and optional sequence and state diagrams, with comments
on parts, interactions, lines, areas of a screenshot and diagram
entities. Its skill tells agents how to build a map from a change, with a
script that lists the change's files from git.
