---
title: Themes
description: "Choose a theme for dark and light, and add your own theme as a JSON file."
---

Pinrail has a dark and a light colour scheme, and each scheme has a theme. The theme applies to the app and to every plugin view. You can choose one of the bundled themes or add your own as a JSON file.

![Settings, Appearance: the colour scheme, a theme for dark and for light, and custom themes.](screenshot:themes "Each colour scheme has its own theme.")

## Choose a theme

Open <kbd>⌘,</kbd> › *Appearance*. *Colour scheme* selects *System*, *Dark* or *Light*, and *System* follows your computer's setting. *Dark theme* and *Light theme* select the theme for each scheme. <kbd>⌘⇧L</kbd> switches between the two schemes.

You can also press <kbd>⌘K</kbd> and type a theme's name, then choose *Switch to the … theme*.

Pinrail bundles these themes:

| Scheme | Themes |
|---|---|
| Dark | Pinrail Dark, Nord, Dracula, Catppuccin Mocha |
| Light | Pinrail Light, Catppuccin Latte, Solarized Light |

## Add your own theme

1. Open *Settings › Appearance* and choose *New theme…*. Pinrail copies the current theme to a file named `my-theme.json`, selects the new theme, and opens the file.
2. Change the `name` and the colours you want.
3. Save the file. The app and any open plugin view update within a second.

You can also add a file to the `themes` folder in your data directory yourself. *Open folder* in the same section opens that folder. The file name without `.json` is the theme's id. It can contain lowercase letters, digits and hyphens, and it cannot be `dark` or `light`.

If a theme has the same id as a bundled theme, your file replaces the bundled one.

## The theme file

```json
{
  "name": "Nord",
  "scheme": "dark",
  "colors": {
    "bg": "#2e3440",
    "text": "#eceff4",
    "accent": "#88c0d0"
  }
}
```

| Field | Required | Meaning |
|---|---|---|
| `name` | Yes | The name shown in Settings, up to 60 characters. |
| `scheme` | Yes | `"dark"` or `"light"`. It decides which setting lists the theme. |
| `colors` | No | The colours, by the keys listed below. |

We recommend that you set every key. A colour that you leave out comes from Pinrail Dark or Pinrail Light, according to the scheme, and it may not suit your other colours.

A colour is a hex colour such as `#88c0d0`, or an `rgb()`, `hsl()`, `oklch()` or `color-mix()` expression. Named colours, `var()` and `url()` are not accepted.

If a file has an error, such as invalid JSON or a bad colour, Settings shows the error under *Custom themes* and you cannot choose the theme. A setting that names it uses the built-in theme of that scheme until you fix the file. Pinrail ignores a colour key that it does not recognise.

An agent can write a theme file too. To check one, read `GET /api/v1/themes`, which lists every theme with its `error` and `warnings`.

## Colour keys

| Key | Where it is used |
|---|---|
| `bg` | The main background |
| `bg-panel` | The background of panels |
| `bg-raised` | Cards, fields and menus |
| `bg-hover` | A row or a button under the pointer |
| `sidebar` | The sidebar |
| `border` | Borders |
| `border-strong` | Stronger borders, and scrollbars |
| `text` | Text |
| `dim` | Secondary text |
| `faint` | The least prominent text |
| `accent` | Links, focus and what is selected |
| `accent-bg` | The background of what is selected |
| `button-bg` | The primary button |
| `button-text` | Text on filled buttons |
| `danger` | Errors, destructive actions and blocker findings |
| `warning` | Warnings and major findings |
| `caution` | Minor findings |
| `success` | Success |
| `neutral` | Neutral tones and nit findings |
| `tooltip-bg`, `tooltip-text` | Tooltips |
| `tooltip-key-bg`, `tooltip-key-text` | Shortcut keys in tooltips |
| `tooltip-danger-bg`, `tooltip-danger-text` | Tooltips for destructive actions |
| `diff-add-bg`, `diff-add-gutter` | Added lines in a diff, and their gutter |
| `diff-del-bg`, `diff-del-gutter` | Deleted lines in a diff, and their gutter |
| `code-keyword`, `code-string`, `code-number`, `code-comment`, `code-title`, `code-type`, `code-attr`, `code-symbol`, `code-meta` | Highlighted code |

A plugin view follows your theme if it is styled with the [SDK's tokens](/docs/building/design/#themes). A view with colours of its own looks as it does in the built-in theme of the scheme.
