---
title: The inbox
description: "Where reviews wait for you, how you decide them, the history of everything decided, and the keys that move you through it."
---

The inbox is where your agents' reviews wait. When you open a review, its plugin's view shows the work. You decide in the view and hand the decision over, and the agent continues. Reviews that have ended move to your history.

![The inbox: eleven pending reviews grouped by project, each with counts of what it asks, and the waiting list and the projects in the sidebar.](screenshot:inbox "Eleven reviews waiting, across eight projects.")

## The inbox

The inbox lists every **pending** review, newest first. Each row shows the title, the plugin, where it came from and who asked, and a summary of what the review asks. A paperclip icon shows that a review includes attachments, and how many. Hover over it to see their total size.

The summary is a set of counts, such as *1 blocker · 2 major · 1 nit* for a code review or *3 drafts* for emails, colored by importance. Each plugin decides what it counts, and a plugin that counts nothing shows no summary. The same counts appear in the notification for a new review and at the top of the review while it waits.

- **Filter by project or plugin.** The sidebar lists the projects your reviews come from. Pick one, or choose a project or a plugin from the menus above the list, to see only those reviews.
- **Show new rounds.** *New rounds* shows only the reviews that revise an earlier one.
- **Search.** Press <kbd>/</kbd> and type. The inbox shows the reviews whose title, plugin, requester, project, workflow or reference contains the text you typed. The content of a review is not searched.
- **Jump to the oldest.** The sidebar keeps the oldest waiting reviews one click away on every screen, and <kbd>⌥↓</kbd> and <kbd>⌥↑</kbd> step through what is waiting.

## Deciding a review

Open a review with <kbd>Enter</kbd> or a click. The plugin's view fills the screen. Depending on the plugin, it shows a list, a diff, draft emails or a form.

1. **Decide in the view.** Use the controls that the plugin provides, for example to accept, reject, edit or comment. Your work is saved as a draft as you go, so you can open another review and come back to it.
2. **Add a note to the agent**, if you want to, in the box below the view.
3. **Hand over** with the button, or <kbd>⌘↵</kbd>. The button says what will happen, such as *Hand over 3 of 5*. Pinrail then returns to the inbox, where the next review is waiting.

:::caution[Drafts last only while Pinrail runs]
Quitting, restarting or updating Pinrail loses your drafts and any note to the agent that you have not handed over yet. Hand over a review before you quit the app.
:::

A review that includes attachments lists them above the view. Click *N files* to list each attachment with its name, size and type, and choose *Save…* to save one.

If the whole review is wrong, **discard** it with the *Discard* button at the right of the review's header, or with <kbd>D</kbd> on it in the inbox list. The agent is told to stop and is given your reason. From the review's header, Pinrail then returns to the inbox. See [Deciding and discarding](/docs/concepts/reviews/#deciding-and-discarding).

![The discard dialog, with a reason for the agent typed in.](screenshot:discard "When you discard a review, nothing is decided, and the agent is told to stop and is given your reason.")

:::tip[Focus on the view]
<kbd>⌘⇧M</kbd>, or the *Maximize* button at the right of the review's header, maximizes the plugin's view and hides everything else. Press <kbd>⌘⇧M</kbd> or <kbd>Esc</kbd> to restore it. The button beside it copies the decision as Markdown, the way the agent receives it.
:::

## Rounds

When an agent revises a review you asked it to change, the new round opens with your previous verdicts beside each item. Step between the rounds of one review with <kbd>[</kbd> and <kbd>]</kbd>.

## History

Every review that has ended is in your **history**: decided, discarded, withdrawn or expired. Open one and it renders read-only, with the release of the plugin it was submitted to.

Each decided review shows what was decided, as its plugin counts it: for example *2 accepted · 1 rejected*, or a verdict such as *approved* in place of *decided*. The review's header shows the same, and so does the decision the agent receives.

![History: ended reviews with their outcome and what was decided, their project, and when they were recorded.](screenshot:history "Reviews that were decided, discarded or withdrawn, newest first.")

To search your history, press <kbd>/</kbd> and type one or more words. A review matches when each word appears in its title, plugin, requester, project, workflow or reference, or in the name of the person who decided it. As in the inbox, the content of a review is not searched. You can also filter by project, plugin or outcome. Open it with <kbd>⌘⇧H</kbd>.

To choose how long history is kept, see *Keep reviews for* in [Settings](/docs/using/settings/#data).

## Search everything

<kbd>⌘K</kbd> opens a palette that searches every review, pending or decided, and every action in the app, such as going to a screen, opening settings or switching the theme.

![The command palette: pending reviews, recent decisions and plugins, ready to search.](screenshot:palette "The command palette lists pending reviews, recent decisions and plugins.")

## Keys

Press <kbd>?</kbd> anywhere in the app to see these. On Linux, use <kbd>Ctrl</kbd> for <kbd>⌘</kbd> and <kbd>Alt</kbd> for <kbd>⌥</kbd>.

![The keyboard shortcuts dialog.](screenshot:shortcuts)

| Keys | Action |
|---|---|
| <kbd>J</kbd> / <kbd>K</kbd> | Next / previous review |
| <kbd>Enter</kbd> | Open the focused review |
| <kbd>⌥↓</kbd> / <kbd>⌥↑</kbd> | Next / previous waiting review |
| <kbd>D</kbd> | Discard the focused review |
| <kbd>/</kbd> | Search the inbox or the history |
| <kbd>⌘K</kbd> | Search everything |
| <kbd>⌘I</kbd> | Inbox |
| <kbd>⌘⇧H</kbd> / <kbd>⌘⇧P</kbd> | History / Plugins |
| <kbd>⌘,</kbd> | Settings |
| <kbd>⌘⇧L</kbd> | Switch theme |
| <kbd>⌘B</kbd> | Show or hide the sidebar |
| <kbd>⌘[</kbd> / <kbd>⌘]</kbd> | Back / forward |
| <kbd>⌘↵</kbd> | Hand over to the agent |
| <kbd>⌘⇧M</kbd> | Maximize / restore the view |
| <kbd>[</kbd> / <kbd>]</kbd> | Previous / next round |
| <kbd>?</kbd> | Keyboard shortcuts |

A plugin can add keys of its own. They are listed under the plugin in the <kbd>?</kbd> dialog while one of its reviews is open.

### From anywhere

<kbd>⌥⇧W</kbd> brings Pinrail forward from any app and opens the oldest waiting review. You can change the shortcut, and have it open the inbox instead, in *Settings › Shortcuts*.
