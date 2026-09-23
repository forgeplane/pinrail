---
title: The inbox
description: "Where reviews wait for you, how you decide them, the history of everything decided, and the keys that move you through it."
---

The inbox is where your agents' reviews wait. Open one, decide it in the view its plugin draws, hand it over, and the agent carries on. Everything decided moves to your history.

![The inbox: eight pending reviews grouped by project, with the waiting list and the projects in the sidebar.](screenshot:inbox "Eight reviews waiting, across six projects.")

## The inbox

The inbox lists every **pending** review, newest first. Each row shows the title, the plugin, where it came from and who asked, and a summary of what is inside.

- **Filter by project.** The sidebar lists the projects your reviews come from. Pick one to see only its reviews.
- **Search.** Press `/` and type. Every word must appear somewhere in the title, the payload, the plugin, the requester, the project or who decided.
- **Jump to the oldest.** The sidebar keeps the oldest waiting reviews one click away on every screen, and `⌥↓` and `⌥↑` step through what is waiting.

## Deciding a review

Open a review with `Enter` or a click. The plugin's view fills the screen: a list, a diff, a set of drafts, a form.

1. **Decide in the view.** Accept, reject, edit, comment: whatever the plugin offers. Your work is saved as a draft as you go, so you can leave and come back.
2. **Add a note to the agent**, if you want to, in the box below the view.
3. **Hand over** with the button, or `⌘↵`. The button says what will happen, such as *Hand over 3 of 5*.

If the whole review is wrong, **discard** it with `D` or the *Discard* button instead: the agent is told to stop, with your reason. See [Deciding and discarding](/docs/concepts/reviews/#deciding-and-discarding).

![The discard dialog, with a reason for the agent typed in.](screenshot:discard "Discarding: nothing is decided, and the agent is told to stop, with your reason.")

:::tip[Focus on the view]
`⌘⇧M` maximises the plugin's view and hides everything else. Press it again to restore.
:::

## Rounds

When an agent revises a review you asked it to change, the new round opens with your previous verdicts beside each item. Step between the rounds of one review with `[` and `]`.

## History

Every review that has ended is in your **history**: decided, discarded, withdrawn or expired. Open one and it renders exactly as it was, in the view it was decided in, read-only.

![History: ended reviews with their outcome, project and when they were recorded.](screenshot:history "Decided, changes requested, discarded and withdrawn, newest first.")

Search it the same way as the inbox, and filter by project, plugin or outcome. Open it with `⌘⇧H`.

How long history is kept is up to you: see *Keep reviews for* in [Settings](/docs/using/settings/#data).

## Search everything

`⌘K` opens a palette that searches every review, pending or decided, and every action in the app: go to a screen, open settings, switch the theme.

![The command palette: pending reviews, recent decisions and plugins, ready to search.](screenshot:palette "Everything in one place: what waits, what was decided, the plugins.")

## Keys

Press `?` anywhere in the app to see these. On Windows and Linux, `⌘` is `Ctrl`.

![The keyboard shortcuts dialog.](screenshot:shortcuts)

| Keys | Does |
|---|---|
| `J` / `K` | Next / previous review |
| `Enter` | Open the focused review |
| `⌥↓` / `⌥↑` | Next / previous waiting review |
| `D` | Discard the focused review |
| `/` | Search the inbox or the history |
| `⌘K` | Search everything |
| `⌘I` | Inbox |
| `⌘⇧H` / `⌘⇧P` | History / Plugins |
| `⌘,` | Settings |
| `T` | Switch theme |
| `⌘B` | Show or hide the sidebar |
| `⌘[` / `⌘]` | Back / forward |
| `⌘↵` | Hand over to the agent |
| `⌘⇧M` | Maximise / restore the view |
| `[` / `]` | Previous / next round |
| `?` | Keyboard shortcuts |

A plugin can add keys of its own. They are listed under the plugin in the `?` dialog while one of its reviews is open.

### From anywhere

`⌥⇧W` brings Wicket forward from any app and opens the oldest waiting review. You can change the shortcut, and have it open the inbox instead, in *Settings › Shortcuts*.
