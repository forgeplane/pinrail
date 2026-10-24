# Changelog

This plugin is released with the Pinrail app, which carries it among its
official plugins. Each version names the release of the app that first
carries it.

## [1.2.0] - Pinrail 0.4.1

### Changed

- A comment on an area can be edited: its pencil, or a click on it, zooms
  in on the area and opens its comment again. The image's note and each
  area's comment have edit and delete buttons, shown while the pointer is
  over them or the focus is in them, which replace Show and Remove.
- The fields for an image's note and for a comment on an area look like
  the change map's: one box with the text, the microphone and an add
  button at its end. Once the text wraps, the buttons move to a row
  under it, with the keys that add the text and start a new line.
- Enter saves an image's note, as it adds a comment on an area, and
  Shift+Enter starts a new line.

## [1.1.0] - Pinrail 0.3.0

### Added

- The round's notes fold away to a single line. The view remembers
  whether they are open, in the new `notes_open` setting.
- An image's note and a region's comment can be dictated in the macOS
  app, from a microphone in the note's corner and among the region
  comment's buttons.

### Changed

- The next and previous image are N and P, and a pin at the centre of
  the view is M. J and K now move through the inbox.
- The view fits a narrow frame, such as the inbox's side panel. Below
  820 pixels the whole view scrolls as one, so the image always has
  room. Below 520 pixels the zoom controls sit at the foot of the stage,
  clear of the stage's tools, and the buttons name their keys in their
  tooltips instead of beside their labels.

## [1.0.0] - Pinrail 0.1.0

The first version. It shows a round of generated images, each looked at
closely, with comments and boxed regions on the image itself, a
favourite, and images kept or dropped.
