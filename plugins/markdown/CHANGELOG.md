# Changelog

This plugin is released with the Pinrail app, which carries it among its
official plugins. Each version names the release of the app that first
carries it.

## [1.2.0] - Unreleased

### Added

- A Changes view, for a new version of a document: the source line by
  line against the previous round's, with the changed words marked, the
  section above each change, and the unchanged lines folded. The previous
  round's comments appear at their place, each marked Changed or
  Unchanged by whether its lines changed.

### Changed

- A saved comment has edit and delete buttons at the right of its header,
  shown while the pointer is over it or the focus is in it. A click on the
  comment's text edits it, as well as showing what it is about.
  They replace the Edit and Delete links under the comment.
- The comment field looks like the change map's: one box with the text,
  the microphone and the cancel and add buttons at its end. Once the
  text wraps, the buttons move to a row under it, with the keys that add
  the comment and start a new line.
- When the previous round has comments, the comments panel shows this
  round's and the previous round's in two tabs, which stay at the top as
  the list scrolls. Every comment has the same bar at its left, and a
  long quote shows two lines and an ellipsis.

## [1.1.0] - Pinrail 0.3.0

### Added

- A comment can be dictated in the macOS app, from a microphone in the
  comment field's corner.

### Changed

- The view fits a narrow frame, such as the inbox's side panel. Below
  1100 pixels the outline opens as a drawer over the document, and below
  760 pixels the comments do too, from a button in the header that shows
  their count.

## [1.0.0] - Pinrail 0.1.0

The first version. It shows a Markdown document with its outline,
rendered with its Mermaid diagrams or as its source with line numbers,
with comments on a section, a diagram or a selected passage.
