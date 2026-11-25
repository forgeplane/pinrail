---
title: The inbox
description: "The list of your notifications: what waits on you, the updates from your agents, how to read each row, and the keys that move you through it."
---

The inbox is the list of your notifications. It shows first what waits on you, which is reviews, questions and agents that wait for a permission or an answer, and below it the updates from your agents, such as a work item an agent added or a turn it finished. You open a row to decide a review, answer a question or read what changed.

![The inbox on its All tab, filtered to one repository. Under Waiting on you are two agents that wait in their terminals, a question, a review and a new round of a review. Under Updates are a note, a finished agent and a failed turn.](screenshot:inbox-all "The All tab: what waits on you, then the updates.")

## The tabs

| Tab | What it lists |
|---|---|
| **All** | What waits on you, under **Waiting on you**, then the **Updates**, newest first. |
| **Waiting on you** | Only what waits: reviews, new rounds, questions, and agents that wait for a permission or an answer in their terminal. |
| **Unread** | The rows you have not read yet. |
| **History** | The reviews that have ended. See [History](#history). |

The numbers beside **Waiting on you** and **Unread** count what the filters and the search keep. When a filter is on, the page's title says how much of the whole that is, such as *5 of 30*. On the **All** tab, **Waiting on you** shows the first 200 rows, with a link to the tab for the rest.

In the sidebar, the **Inbox** entry shows two numbers: how many notifications wait on you, then how many updates are unread.

## Reading a row

Every row has the same layout: an unread dot, an icon with a badge, the title, a line that says what changed, the kind of the row, and how long ago it changed.

![The Updates of an inbox: a note, two finished agents, a failed turn, a withdrawn review, and work items that an agent added, commented on, cancelled, marked done and claimed. Each row has an icon with a badge and a line that says what changed.](screenshot:inbox-updates "Updates, each with an icon, a badge and what changed.")

**The icon** shows what the row is about.

| Icon | The row is about |
|:-:|---|
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Work item" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-4px"><path d="M13 5h8"/><path d="M13 12h8"/><path d="M13 19h8"/><path d="m3 17 2 2 4-4"/><path d="m3 7 2 2 4-4"/></svg> | A work item. Its key follows the title. |
| The agent's icon | A session. The title is the session's title. |
| The plugin's icon | A review, shown with its plugin's icon. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Question" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-4px"><path d="M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><path d="M12 17h.01"/></svg> | A question that an agent asked. |

**The badge** on the icon shows what happened. A new review has no badge.

| Badge | Name | Colour | What happened |
|:-:|---|---|---|
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Added" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#5965cb"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14"/><path d="M12 5v14"/></g></svg> | Added | blue | An agent added the work item, or a sub-item of it. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Claimed" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#5965cb"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z"/></g></svg> | Claimed | blue | An agent claimed the work item. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Commented" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#65738e"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z"/></g></svg> | Commented | grey-blue | An agent commented on the work item. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Done" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#28795c"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></g></svg> | Done | green | An agent marked the work item done. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Cancelled" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#7b7d87"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></g></svg> | Cancelled | grey | An agent cancelled the work item. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Changed" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#65738e"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/><path d="m15 5 4 4"/></g></svg> | Changed | grey-blue | An agent made another change to the work item. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Finished" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#28795c"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></g></svg> | Finished | green | The agent finished its turn. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Failed" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#bc4655"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></g></svg> | Failed | red | The agent's turn failed. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Stopped" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#7b7d87"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/></g></svg> | Stopped | grey | The agent stopped. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Waits for a permission" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#99651b"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></g></svg> | Waits for a permission | amber | The agent waits for a permission in its terminal. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Waits for an answer" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#99651b"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><path d="M12 17h.01"/></g></svg> | Waits for an answer | amber | The agent waits for your answer in its terminal. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Note" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#65738e"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/><path d="M14 2v5a1 1 0 0 0 1 1h5"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/></g></svg> | Note | grey-blue | The agent sent you a note. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Asks you" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#5965cb"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><path d="M12 17h.01"/></g></svg> | Asks you | blue | An agent asked you a question. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="New round" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#5965cb"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/></g></svg> | New round | blue | An agent sent a new round of a review. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Withdrawn" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#7b7d87"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></g></svg> | Withdrawn | grey | The agent withdrew the review. |
| <svg width="18" height="18" viewBox="0 0 24 24" role="img" aria-label="Expired" style="vertical-align:-4px"><circle cx="12" cy="12" r="12" fill="#7b7d87"/><g transform="translate(5.2 5.2) scale(0.57)" fill="none" stroke="#fff" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></g></svg> | Expired | grey | The review expired before you decided. |

**The ring** after a work item's title shows the item's status.

| Ring | Status |
|:-:|---|
| <svg width="16" height="16" viewBox="0 0 16 16" role="img" aria-label="Open" style="vertical-align:-3px;color:#7b7d87"><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.6"/></svg> | Open |
| <svg width="16" height="16" viewBox="0 0 16 16" role="img" aria-label="In progress" style="vertical-align:-3px;color:#5965cb"><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M8 4a4 4 0 0 1 0 8z" fill="currentColor"/></svg> | In progress |
| <svg width="16" height="16" viewBox="0 0 16 16" role="img" aria-label="On hold" style="vertical-align:-3px;color:#99651b"><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-dasharray="2.4 1.9"/><path d="M6.6 5.8v4.4M9.4 5.8v4.4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg> | On hold |
| <svg width="16" height="16" viewBox="0 0 16 16" role="img" aria-label="Done" style="vertical-align:-3px;color:#28795c"><circle cx="8" cy="8" r="7" fill="currentColor"/><path d="m5 8.2 2 2 4-4.2" fill="none" stroke="#fff" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg> | Done |
| <svg width="16" height="16" viewBox="0 0 16 16" role="img" aria-label="Cancelled" style="vertical-align:-3px;color:#7b7d87"><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="m5.8 5.8 4.4 4.4M10.2 5.8l-4.4 4.4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg> | Cancelled |

**The dot** at the start of a row shows that you have not read it, and its title is bold. Opening a row marks it read.

**The second line** says what changed since you last looked, and who did it, such as *claude-code added this work item with 10 sub-items*, *3 new comments*, or the first line of an agent's last reply. When several things changed, it names the latest and adds how many more there are. The repository follows it. A review's line also names the work items it belongs to.

## Working through the inbox

- **Open a row** with a click or <kbd>Enter</kbd>. A review, a work item or a session opens in the side panel beside the list, so you stay in the inbox. <kbd>J</kbd> and <kbd>K</kbd> move to the next and previous row, and move the panel with them. An agent that waits in its terminal opens on its terminal.
- **Mark a row read or unread** with <kbd>U</kbd>, or with the button at the end of the row.
- **Mark an update done** with <kbd>E</kbd>, or with the tick at the end of the row. A done update leaves the list. What waits on you cannot be marked done: it leaves when you decide or answer it.
- **Mark everything read** with **Mark all as read** or <kbd>⇧U</kbd>. It marks the rows that the list shows, so it follows the filters and the search.
- **Discard a review** with <kbd>⌘D</kbd> on its row. See [Deciding a review](#deciding-a-review).

Pinrail also marks read updates as done by itself, a week after their last change. **Mark read updates as done** in [Settings › General](/docs/using/settings/#inbox) changes when: on your return to the inbox, after a day, or after a week.

## Filtering and searching

The **Filter** button at the right of the header opens the filters. They keep their values when you switch tabs or go to another page and come back. Each filter that is on shows as a chip under the tabs: click its × to remove it, or **Clear filters** to remove them all.

- **Kind** lists only some kinds of notification, such as reviews, questions, new work items, comments or finished agents. It takes several values.
- **Project or repository** lists the notifications of some projects or repositories. Clicking a repository or a project in the sidebar sets it too.
- **Plugin** lists the reviews of some plugins.
- **Starred** lists the starred reviews. See [Stars](#stars).
- **Show done** adds the updates that are marked done.

To search, press <kbd>/</kbd> and type. The inbox keeps the rows whose title, plugin, requester, project, workflow or reference contains the text. The content of a review is not searched. <kbd>Esc</kbd> leaves the search field and keeps the search, so <kbd>J</kbd> and <kbd>K</kbd> move through the results.

To step through what waits from any page, press <kbd>⌥↓</kbd> and <kbd>⌥↑</kbd>. The sidebar also lists what waits under the session that sent it. See [Sessions in the sidebar](/docs/using/sessions/#sessions-in-the-sidebar).

## Muting

To stop the notifications of one work item or one session, click the bell in its header. The bell unmutes them. To mute a whole project or repository, or to turn a kind of notification off, see [Notifications](/docs/using/notifications/). What waits on you always reaches the inbox.

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

To search the history, press <kbd>/</kbd> on the *History* tab and type one or more words. A review matches when each word appears in its title, plugin, requester, project, workflow or reference, or in the name of the person who decided it. As in the inbox, the content of a review is not searched. You can also filter by project, repository, plugin or outcome. The **Filter** menu has *Outcome* on the *History* tab only.

To choose how long history is kept, see *Keep reviews for* in [Settings](/docs/using/settings/#data).

### Deleting a review

To delete a review, choose **Delete review…** from the **…** menu at the end of its row on the *History* tab, or click the trash button in the review's header. The command palette has **Delete this review…** while a review is open. Every round of the review is deleted, with its decisions and comments, and this cannot be undone. If a round still waits, it is discarded first, so the agent waiting on it is told to stop.

## Stars

Star a review to find it again later. Click the star in the review's header, on its page or in the side panel, or press <kbd>⌘⇧S</kbd>. In the inbox, the star sits at the end of each row: it shows on a starred review, and on the others when the pointer is over the row. Click it again to remove the star. On the *History* tab, a starred round shows its star, and the **…** menu at the end of the row has **Star** and **Remove star**. Questions cannot be starred.

A star belongs to the round you starred. A review is shown as starred when any of its rounds is.

To list the starred reviews, turn on **Starred** in the inbox's **Filter** menu. On the **All** and **Waiting on you** tabs, it lists the waiting reviews with a starred round. On the *History* tab, it lists only the rounds you starred. A starred review is deleted with the rest of your history after the time set in *Keep reviews for*.

## Search everything

<kbd>⌘K</kbd> opens a palette that searches every review, pending or decided, the stages agents started in their sessions, and every action in the app, such as going to a screen, opening settings or switching the theme. Choose a stage to open its session's thread where the stage started.

![The command palette: pending reviews, recent decisions and plugins, ready to search.](screenshot:palette "The command palette lists pending reviews, recent decisions and plugins.")

## Keys

Press <kbd>?</kbd> anywhere in the app to see the keys, in sections: those of the page you are on first, then the general ones. **Show all shortcuts** lists every section, and a search looks through all of them. On Linux, use <kbd>Ctrl</kbd> for <kbd>⌘</kbd> and <kbd>Alt</kbd> for <kbd>⌥</kbd>.

![The keyboard shortcuts dialog.](screenshot:shortcuts)

| Keys | Action |
|---|---|
| <kbd>J</kbd> / <kbd>K</kbd> | Next / previous row |
| <kbd>Enter</kbd> | Open the focused row |
| <kbd>U</kbd> | Mark the row read or unread |
| <kbd>E</kbd> | Mark the update done, or not done |
| <kbd>⇧U</kbd> | Mark all as read |
| <kbd>⌥↓</kbd> / <kbd>⌥↑</kbd> | Next / previous notification that waits |
| <kbd>Esc</kbd> | Close the review panel |
| <kbd>⌘D</kbd> | Discard the focused or open review |
| <kbd>/</kbd> | Search the page |
| <kbd>⌘K</kbd> | Search everything |
| <kbd>⌘P</kbd> | Go to a session |
| <kbd>⌘1</kbd> or <kbd>⌘I</kbd> | Inbox |
| <kbd>⌘2</kbd> / <kbd>⌘3</kbd> / <kbd>⌘4</kbd> | Sessions / Work items / Projects |
| <kbd>⌘⇧H</kbd> / <kbd>⌘⇧P</kbd> | The inbox's History tab / Plugins |
| <kbd>⌘,</kbd> | Settings |
| <kbd>⌘⇧L</kbd> | Switch theme |
| <kbd>⌘B</kbd> | Show or hide the sidebar |
| <kbd>⌘[</kbd> / <kbd>⌘]</kbd> | Back / forward |
| <kbd>⌘↵</kbd> | Hand over to the agent |
| <kbd>⌘⇧S</kbd> | Star or remove the star of the review |
| <kbd>E</kbd> | On the Sessions page: archive or unarchive the session |
| <kbd>⌘⇧M</kbd> | Maximize / restore the view |
| <kbd>⌘F</kbd> | Find in the view |
| <kbd>[</kbd> / <kbd>]</kbd> | Previous / next round |
| <kbd>?</kbd> | Keyboard shortcuts |

A plugin can add keys of its own. They are listed under the plugin in the <kbd>?</kbd> dialog while one of its reviews is open.

### From anywhere

<kbd>⌥⇧W</kbd> brings Pinrail forward from any app and opens the oldest waiting review. You can change the shortcut, and have it open the inbox instead, in *Settings › Shortcuts*.
