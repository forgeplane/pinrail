---
title: The inbox
description: "Where reviews wait for you, how you decide them, the history of everything decided, and the keys that move you through it."
---

The inbox is where your agents' reviews wait. When you open a review, its plugin's view shows the work. You decide in the view and hand the decision over, and the agent continues. Reviews that have ended move to the inbox's History tab.

Questions an agent asks with `pinrail ask` wait in the inbox too, marked with a question icon and *asks you*. They open in the thread of the agent's session. See [Questions and sessions](/docs/using/sessions/).

![The inbox: eleven pending reviews grouped by project, each with counts of what it asks, and the repositories with their sessions in the sidebar.](screenshot:inbox "Eleven reviews waiting, across eight projects.")

## The inbox

The inbox has three tabs. *Pending* lists every review that waits, newest first. *New rounds* lists only the waiting reviews that revise an earlier one. *History* lists the reviews that have ended.

On the *Pending* tab, each row shows the title, the plugin, where it came from and who asked, and a summary of what the review asks. A paperclip icon shows that a review includes attachments, and how many. Hover over it to see their total size.

The summary is a set of counts, such as *1 blocker · 2 major · 1 nit* for a code review or *3 drafts* for emails, colored by importance. Each plugin decides what it counts, and a plugin that counts nothing shows no summary. The same counts appear in the notification for a new review and at the top of the review while it waits.

The filters above the tabs apply to every tab, and keep their values when you switch tabs.

- **Filter by project or plugin.** The sidebar lists your repositories. Click one, or choose a project or a plugin from the menus above the list, to see only those reviews.
- **Show the starred reviews.** Turn on the star filter beside the menus. See [Stars](#stars).
- **Search.** Press <kbd>/</kbd> and type. The inbox shows the reviews whose title, plugin, requester, project, workflow or reference contains the text you typed. The content of a review is not searched.
- **Jump to what waits.** The sidebar lists what waits under the session that sent it, one click away on every screen. See [Sessions in the sidebar](/docs/using/sessions/#sessions-in-the-sidebar). <kbd>⌥↓</kbd> and <kbd>⌥↑</kbd> step through what is waiting, oldest first.

## Deciding a review

Open a review with <kbd>Enter</kbd> or a click. It opens in a panel beside the inbox, which stays in view. Depending on the plugin, the view shows a list, a diff, draft emails or a form.

- **Open as a page** with the button at the right of the panel's header, when the view needs more room. The page has a button that shows the review in the panel again.
- **Close the panel** with its *Close* button, or press <kbd>Esc</kbd>, from the view too. When the view has something of its own open, such as a comment, the first <kbd>Esc</kbd> closes that, and the next closes the panel.
- **Go to the next or previous review** with <kbd>J</kbd> and <kbd>K</kbd>, in the order the inbox lists them. They work while you work in the view too, unless you are typing in a field, and go round at the ends of the list: <kbd>J</kbd> on the last review goes to the first. <kbd>⌥↓</kbd> and <kbd>⌥↑</kbd> do the same.
- **Resize the panel** by dragging its left edge. Pinrail remembers the width.

The *History* tab opens a review in the same panel. To open reviews as a page of their own instead, set *Settings › General › Opening a review* to *As a page*.

1. **Decide in the view.** Use the controls that the plugin provides, for example to accept, reject, edit or comment. Your work is saved as a draft as you go, so you can open another review and come back to it.
2. **Add a note to the agent**, if you want to, in the box below the view. In the macOS app you can dictate it: click the microphone, speak, and click it again or press <kbd>Esc</kbd> to stop. The first time, macOS asks for access to the microphone and to speech recognition.
3. **Hand over** with the button, or <kbd>⌘↵</kbd>. The button says what will happen, such as *Hand over 3 of 5*. In the panel, Pinrail then opens the next review in the list, and closes the panel after the last one. On a review's page, Pinrail returns to the inbox.

:::caution[Drafts last only while Pinrail runs]
Quitting, restarting or updating Pinrail loses your drafts and any note to the agent that you have not handed over yet. Hand over a review before you quit the app.
:::

A review that includes attachments lists them above the view. Click *N files* to list each attachment with its name, size and type, and choose *Save…* to save one.

If the whole review is wrong, **discard** it with the *Discard* button at the right of the review's header, or press <kbd>⌘D</kbd> on the review or on its row in the inbox list. The agent is told to stop and is given your reason. From the review's header, Pinrail then returns to the inbox. See [Deciding and discarding](/docs/concepts/reviews/#deciding-and-discarding).

![The discard dialog, with a reason for the agent typed in.](screenshot:discard "When you discard a review, nothing is decided, and the agent is told to stop and is given your reason.")

:::tip[Focus on the view]
<kbd>⌘⇧M</kbd>, or the *Maximize* button at the right of the review's header, maximizes the plugin's view and hides everything else. Press <kbd>⌘⇧M</kbd> or <kbd>Esc</kbd> to restore it. The button beside it copies the decision as Markdown, the way the agent receives it.

<kbd>⌘F</kbd> finds text in the plugin's view, as find in page does in a browser. Type to highlight every match, press <kbd>Enter</kbd> or <kbd>⇧Enter</kbd> to go to the next or previous one, and press <kbd>Esc</kbd> to close the find bar. The search ignores case and covers the text the view shows, but not text inside form fields, in images or in parts the view has collapsed.
:::

## Rounds

When an agent revises a review you asked it to change, the new round opens with your previous verdicts beside each item. Step between the rounds of one review with <kbd>[</kbd> and <kbd>]</kbd>.

## History

Every review that has ended is on the inbox's **History** tab: decided, discarded, withdrawn or expired. Each row shows the outcome, who ended the review and when. Open one and it renders read-only, with the release of the plugin it was submitted to. Open the tab with <kbd>⌘⇧H</kbd>.

Each decided review shows what was decided, as its plugin counts it: for example *2 accepted · 1 rejected*, or a verdict such as *approved* in place of *decided*. The review's header shows the same, and so does the decision the agent receives.

![The History tab: ended reviews with their outcome and what was decided, their project, and when they ended.](screenshot:history "Reviews that were decided, discarded or withdrawn, newest first.")

To search the history, press <kbd>/</kbd> on the *History* tab and type one or more words. A review matches when each word appears in its title, plugin, requester, project, workflow or reference, or in the name of the person who decided it. As in the inbox, the content of a review is not searched. You can also filter by project, plugin or outcome. The *Outcome* menu shows on the *History* tab only.

To choose how long history is kept, see *Keep reviews for* in [Settings](/docs/using/settings/#data).

## Stars

Star a review to find it again later. Click the star in the review's header, on its page or in the side panel, or press <kbd>⌘⇧S</kbd>. In the inbox, the star sits at the end of each row: it shows on a starred review, and on the others when the pointer is over the row. Click it again to remove the star. Questions cannot be starred.

A star belongs to the round you starred. A review is shown as starred when any of its rounds is.

To list the starred reviews, turn on the star filter in the inbox, beside the project and plugin filters. On the *Pending* and *New rounds* tabs, it lists the waiting reviews with a starred round. On the *History* tab, it lists only the rounds you starred. A starred review is deleted with the rest of your history after the time set in *Keep reviews for*.

## Search everything

<kbd>⌘K</kbd> opens a palette that searches every review, pending or decided, the stages agents started in their sessions, and every action in the app, such as going to a screen, opening settings or switching the theme. Choose a stage to open its session's thread where the stage started.

![The command palette: pending reviews, recent decisions and plugins, ready to search.](screenshot:palette "The command palette lists pending reviews, recent decisions and plugins.")

## Keys

Press <kbd>?</kbd> anywhere in the app to see these. On Linux, use <kbd>Ctrl</kbd> for <kbd>⌘</kbd> and <kbd>Alt</kbd> for <kbd>⌥</kbd>.

![The keyboard shortcuts dialog.](screenshot:shortcuts)

| Keys | Action |
|---|---|
| <kbd>J</kbd> / <kbd>K</kbd> | Next / previous review |
| <kbd>Enter</kbd> | Open the focused review |
| <kbd>⌥↓</kbd> / <kbd>⌥↑</kbd> | Next / previous waiting review |
| <kbd>Esc</kbd> | Close the review panel |
| <kbd>⌘D</kbd> | Discard the focused or open review |
| <kbd>/</kbd> | Search the inbox |
| <kbd>⌘K</kbd> | Search everything |
| <kbd>⌘I</kbd> | Inbox |
| <kbd>⌘⇧H</kbd> / <kbd>⌘⇧P</kbd> | The inbox's History tab / Plugins |
| <kbd>⌘,</kbd> | Settings |
| <kbd>⌘⇧L</kbd> | Switch theme |
| <kbd>⌘B</kbd> | Show or hide the sidebar |
| <kbd>⌘[</kbd> / <kbd>⌘]</kbd> | Back / forward |
| <kbd>⌘↵</kbd> | Hand over to the agent |
| <kbd>⌘⇧S</kbd> | Star or remove the star of the review |
| <kbd>E</kbd> | Archive or unarchive the session |
| <kbd>⌘⇧M</kbd> | Maximize / restore the view |
| <kbd>⌘F</kbd> | Find in the view |
| <kbd>[</kbd> / <kbd>]</kbd> | Previous / next round |
| <kbd>?</kbd> | Keyboard shortcuts |

A plugin can add keys of its own. They are listed under the plugin in the <kbd>?</kbd> dialog while one of its reviews is open.

### From anywhere

<kbd>⌥⇧W</kbd> brings Pinrail forward from any app and opens the oldest waiting review. You can change the shortcut, and have it open the inbox instead, in *Settings › Shortcuts*.
