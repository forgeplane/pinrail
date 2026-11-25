---
title: Questions and sessions
description: "Start an agent, follow its session in the summary and the terminal, write to it, answer its questions, and stop, resume and archive sessions."
---

A session is one run of an agent, such as a Claude Code conversation. This page covers what you do with sessions in the app: starting an agent, following it in its summary and its terminal, writing to it, answering its questions, and stopping, resuming and archiving it. [Sessions](/docs/concepts/sessions/), under Concepts, explains what a session is, where its agent runs and what its states mean.

## Starting an agent

Click **New session** in the sidebar, or press <kbd>⌘N</kbd>.

![The New session page. It asks what Claude Code should work on in the folder acme-api, with the agent and the folder above a message box that holds the task.](screenshot:new-session "The New session page, with an agent, a folder and the first message.")

- **Agent:** the agents on your computer that Pinrail can start. The page opens with the one you used most in the last 30 days. [Supported agents](/docs/agents/supported/) lists them.
- **Folder:** where the agent works. Choose one of the folders your agents worked in, filter the list, or type a path.
- **First message:** the task, which becomes the agent's first prompt. Type `#` to name a work item. Leave it empty to start the agent without a task.
- **Arguments:** extra arguments for the agent, such as `--model opus`.

Press the arrow to start. The agent's session opens at once, in its terminal unless you changed **Open an agent you start in** in [Settings › Terminal](/docs/using/settings/#terminal).

Codex and Antigravity CLI tell Pinrail their session only through its hooks. When one of them is not connected yet, the page says so and offers **Connect**, which turns the reports on.

You can also start an agent on a work item, from the item's panel or with **Add and start** in the quick add. See [Work items](/docs/using/work-items/#an-items-panel).

## The Summary and Terminal views

A session that Pinrail runs has three views, which the buttons at the right of its header switch between: **Summary**, **Terminal** and **Split view**. [The Summary and Terminal views](/docs/concepts/sessions/#the-summary-and-terminal-views) says what each one shows and when to use it.

![A session's Terminal view. The header shows the session's title, a chip that says the agent has worked for eight minutes, the Stop button and the three view buttons. Below it, the agent's terminal shows the files it read and changed and the tests it ran. The bar under the terminal says You are typing.](screenshot:session-terminal "A session's Terminal view, with its state in the bar under the terminal.")

- **Switch** between the summary and the terminal with <kbd>Ctrl+&#96;</kbd> or <kbd>⌘⇧T</kbd>. On Linux, the second shortcut is <kbd>Ctrl+⇧T</kbd>. Each session remembers the view you used last.
- **Resize the split view** by dragging the divider between the summary and the terminal. Pinrail keeps the size for every session.
- **Take control of the terminal** by clicking it or typing. One viewer at a time types into a terminal and sets its size.

The bar under the terminal shows its state with a coloured dot:

| The bar says | What it means |
|---|---|
| *Viewing · click or type to take control* | You see the terminal. Your keys do not go to the agent yet. |
| *You are typing* | Your keys go to the agent, and the terminal has the size of this view. |
| *Size set by another viewer · click to take control* | Another view, such as `pinrail attach`, controls the terminal. |
| *Reconnecting to the terminal…* | The view lost the terminal and is connecting again. |
| *Disconnected from the terminal.* | The view has no terminal. The agent is not affected. |

In the terminal, <kbd>⌘V</kbd> pastes what you copied. Files copied in Finder paste their paths, and a copied image goes to the agent as it does when you paste it in your own terminal. Dropping files on the terminal pastes their paths too.

When the agent ends, the Terminal view shows how and when it ended, with a **Resume** button, above the agent's last screen. When the agent goes on in a new session without leaving its terminal, as after `/clear`, the page follows the terminal to the new session.

A session in your own terminal has no terminal in Pinrail. Its Terminal view says so, and offers to resume it once it has stopped.

## Writing to the agent

Write to the agent in the message box under the session's thread, and send with <kbd>Enter</kbd>. [Messages to the agent](/docs/concepts/sessions/#messages-to-the-agent) explains when the agent receives a message.

- **Attach files** by pasting them, dropping them on the box, or with the paperclip. The agent receives each file's path after your text.
- **Link a work item** by typing `#` and part of its key or title.
- **Waiting messages** show at the top of the box, a line each, with a few words that say when each one goes to the agent. Click the × on a line to delete the message before the agent receives it.
- **Type now** shows on a message that has waited a minute for the agent to start. It types the message at once.

A message that you typed stays in the box when you open another page or session and come back.

## Answering questions

![An agent session's thread: three questions waiting, the first answered with a note, and the box for a comment to the agent at the bottom.](screenshot:ask-thread "Questions waiting in a session's thread.")

Questions wait in the inbox like reviews, marked with a question icon and *asks you*. Open them and they show in their session's thread, in the panel beside the inbox or as a page. The questions an agent asks together are one message, numbered *1/3*, *2/3* and so on, and are sent back together.

- **Pick a choice** with a click, or press its number, <kbd>1</kbd> to <kbd>9</kbd>. A yes-or-no question takes <kbd>1</kbd> for yes and <kbd>2</kbd> for no. A star marks the answer the agent recommends. Click a picked choice again to unpick it.
- **Pick several** where the question says *Select all that apply*.
- **Answer in your own words** with *Other…*, which opens a field under the choices. A question without choices has its field open from the start.
- **Add a note** to an answer with the button at the right of the question, or press <kbd>C</kbd>. The agent receives the note with the answer.

In a field, <kbd>Enter</kbd> saves what you wrote and <kbd>⇧Enter</kbd> starts a new line. In the answer to a question without choices, <kbd>Enter</kbd> goes on to the next question. A saved note or answer shows under the question, with a pencil to edit it and a bin to delete it when you point at it. <kbd>Esc</kbd> leaves a field, so that the keys pick choices again.

The keys answer the current question, which shows its keys on its choices. A key that picks a choice moves on to the next question that has no answer yet. <kbd>N</kbd> and <kbd>P</kbd> go to the next and the previous question, and a click makes a question the current one.

### Sending the answers

The box at the bottom of the thread holds a comment to the agent on all the questions at once. You can dictate it with the microphone in the macOS app. When the agent suggests a reply, the empty box shows it: press <kbd>Tab</kbd> to take it, and edit it or send it as it is.

- **Send** with <kbd>Enter</kbd> in the box, the arrow button, or <kbd>⌘↵</kbd> from anywhere in the thread. A question you left alone goes back as not answered, and the arrow button's tooltip says how many there are. When the agent asked several sets of questions, Send also sends every other set whose questions you have all answered.
- **Decline** with the button in the box, or press <kbd>⌘D</kbd>, when the agent should not have asked. The agent is told to stop and is given your reason.

Once you have sent them, the answers show under their questions, with your notes and your comment. The thread stays open, so you can see what the agent does next.

## A session's thread

Each session's thread lists what the agent sent from it, oldest first: its questions, its reviews, which open when you click them, and its statuses. A review the agent revised shows once, where its first round was sent, with its newest round and the round's number, such as *round 2 of 2*. Where the agent sent a later round, a *Revised* line names the review and says when the round waits for you. Click the line to scroll to the review. Where the agent withdrew a review, a line names the review and gives the agent's reason.

### A session's header and menu

The header has two rows. The first holds the session's title and a chip with its state, such as *Working · 8m* or *Needs you · 2 waiting*. The second says where the agent works, since when, and which work items the session holds. The title is the stage of work the agent is on. Until the agent names one, it is the session's own name, such as **Careful Calcium**.

![A session's header with its menu open. The menu lists Rename, Archive, Copy session id, Stop Claude Code and Delete.](screenshot:session-menu "A session's menu, opened with the ⋯ button beside its title.")

- **Rename the session** by clicking its name in the header, or with **Rename** in the menu.
- **Open the menu** with the **⋯** button beside the title. It archives or unarchives the session, marks a finished session as read or unread, copies the session's id, stops or resumes its agent when Pinrail runs it, and deletes the session. The same menu opens from the **⋯** button on the session's row in the sidebar, or with a right-click there.
- **Stop the agent** with **Stop** in the header, when Pinrail runs it.
- **Mute the session's notifications** with the bell. The bell unmutes them.

Deleting a session stops its agent, withdraws the questions and reviews that still wait, and gives up the work items it holds. It cannot be undone. See [Deleting](/docs/concepts/sessions/#deleting).

In a narrow window, the header leaves out the start time and shows **Stop** and **Resume** as icons, and the session's details start hidden.

### A session's details

The panel at the right of a session's thread sums up the session, so that you can see where it stands without reading the whole thread.

![A session's page: the thread with a question about a diff, a note with a chart and the agent saying it has finished, and beside it the session's details, with its work items, two stages and its decided reviews.](screenshot:session-details "A session's thread, and its details beside it.")

- **Waiting on you:** the questions and reviews of the session that wait for your answer.
- **Waiting on the agent:** messages for the agent, such as your comments on its work items, until the agent next runs `pinrail status` and receives them.
- **Work items:** the items the session holds or worked on, with their status. Click one to open it beside the thread.
- **Starred:** the session's starred reviews, newest first. A starred review shows a star wherever it is listed.
- **Stages:** each stage the agent started, oldest first, with how long it took. Point at a stage to see when it started and the reviews sent during it, and click it to go to its start in the thread.
- **Reviews:** the reviews of the session that have ended, with their outcomes, folded until you open it. Click one to open it.
- **Session:** the session's name, the agent, the agent's own id for the session (*Agent session*, which you can copy from the session's menu, as `claude --resume` takes it), the project, the branch, the folder the agent works in and the last activity.

Hide or show the panel with the button at the right of the header, or press <kbd>⌘⇧B</kbd>. Drag its left edge to resize it. Pinrail remembers both. In a narrow window the panel shows over the thread, and <kbd>Esc</kbd> hides it again.

A question's thread links to its session's page with *session* and the id in its header. A review sent from a session has a button in its header, on its page and in the side panel, that opens the session's thread.

On a session's page, a review you click opens in a side panel beside the thread, in place of the session's details, and its card in the thread is marked. Deciding or discarding the review closes the panel, and the thread stays where you scrolled. Press <kbd>Esc</kbd> or click the close button to close the panel, and drag its edge to resize it. *Open as a page* opens the review on its own page.

A work item you click in a session, in its header, its thread or its details, opens in the same side panel, where you can read and change it as on the **Work items** page. When *Opening a review* is set to *As a page* in [Settings](/docs/using/settings/), the item opens on the **Work items** page instead.

To find text in a session's thread, press <kbd>⌘F</kbd>. Type to highlight every match, press <kbd>Enter</kbd> or <kbd>⇧Enter</kbd> to go to the next or previous one, and press <kbd>Esc</kbd> to close the find bar. The search ignores case. When the focus is in a review open beside the thread, <kbd>⌘F</kbd> finds text in the review's view instead.

When *Opening a review* is set to *As a page* in [Settings](/docs/using/settings/), a review opens on its own page instead. When you open a review that way from its session's thread, or from under its session in the sidebar, the breadcrumb at the top reads *Sessions › <session> › <review>*. Click the session's name to return to its thread. Deciding or discarding the review also returns you to the thread.

![The Sessions page: agent sessions with what waits in each, what each holds, its project and its last activity.](screenshot:sessions "Sessions waiting for you come first.")

The **Sessions** page, in the sidebar, lists every session that asked you something, sent a review or reported a status: those waiting for you first, then the most recently active. Search for an agent, a session id or a title, and choose a project or a repository under *Project or repository* in the **Filter** menu to list only its sessions. **Needs you only**, in the same menu, lists the sessions that wait for you. **Display options** groups the sessions by project, by repository, or shows them in one list. Click a session to open its thread in the side panel, beside the list, as the inbox opens a review. Press <kbd>J</kbd> and <kbd>K</kbd> to open the next and previous session, and <kbd>Esc</kbd> to close the panel. The panel hides the session's details at first: show them with the button in its header. *Open as a page* opens the session on its own page. When *Opening a review* is set to *As a page* in [Settings](/docs/using/settings/), sessions open on their own page too.

To archive a session you are done with, click the archive button at the end of its row, or press <kbd>E</kbd> on the focused row. You can also archive the open session with **Archive** in its **⋯** menu, or with <kbd>E</kbd>. An archived session leaves the Sessions page and the sidebar, but nothing it sent is deleted: its questions and reviews stay in the inbox while they wait, and its thread still opens from a review's header and from the command palette. When Pinrail runs the session's agent, archiving the session stops the agent. You can resume the session later. Turn on *Show archived* in the **Filter** menu to list the archived sessions with the others, and unarchive one with its button or <kbd>E</kbd>. When an archived session's agent sends a question, a review or a status, the session comes back by itself.

### Resuming a session

To resume a stopped session in Pinrail, click **Resume** in its header. This also works for a session that you started in your own terminal. If the session's folder was moved or deleted, Pinrail says so and offers **Choose another folder…**. The session then resumes in the folder you choose, which becomes its folder from then on. Messages that you wrote while the agent was stopped are typed into its terminal once it runs again.

After the computer restarts, the Sessions page says which agents the restart stopped, with **Resume all**, which resumes them one by one.

A Claude Code session that still runs in your own terminal has **Open in Pinrail** in its header.

:::caution[Open in Pinrail starts a second Claude Code]
Opening a session that still runs elsewhere starts a second Claude Code on the same session, and both answer it. Close the other terminal first. Pinrail warns you before it opens the session.
:::

## Going to a session

Press <kbd>⌘P</kbd> to open the command palette on its **Sessions** filter. It lists the sessions that need you first, with what each one waits for, then the sessions that finished a turn since you last looked and those whose agent works. The other sessions are folded under **Earlier**: press <kbd>→</kbd> on that row to show them. The palette opens with the current session selected, and each further press of <kbd>⌘P</kbd> moves down a row. Press <kbd>Enter</kbd> to open the selected session, or type to search every session, archived ones included. The last row starts a new session in the current session's folder. On Linux, press <kbd>Ctrl+⇧O</kbd> instead of <kbd>Ctrl+P</kbd> in a terminal, which keeps <kbd>Ctrl+P</kbd> for the shell.

With nothing typed, <kbd>⌘K</kbd> also lists the sessions that need you, above the pending reviews.

## Sessions in the sidebar

The sidebar lists every session that is not archived, under its repository, in the *Repositories* section. Click a repository's name to see its reviews in the inbox, and click the arrow beside it to collapse or expand it. Click a session to open its thread.

The agent's icon shows what the session is doing, as the agent's hooks or, for an agent that runs in Pinrail, its terminal report it: working, needs you, finished, idle or stopped. A session whose agent reports neither shows only what waits in it, because a status names the work and does not say whether the agent is still working. An amber count at the end of the row says how many questions and reviews wait in the session, and they are listed under it, each opening where you answer or decide it. A review that came from no session is listed under its repository. Hover over a session to see its full title, its state and what it waits for, its agent and its branch.

The button at the right of the section's heading opens a menu:

- **Grouping** groups the sessions by project, by repository, by status, by when they were last updated, or not at all. The heading takes the grouping's name. Grouped by project, a repository in no project is a group of its own, and in a project with several repositories each session names its repository.
- **Ordering** chooses **Priority**, which lists the sessions that need you first, or the most recently updated first.
- **Show details** adds a second line under each session. **Details** chooses what it shows: the last activity, the latest status, the repository, the branch and the agent. The repository is not shown while the sessions are grouped by repository, because each group already names it.
- **Show waiting reviews** lists the questions and reviews waiting in each session under it, or only counts them.
- **Filters** list only some agents, projects, repositories or states. *Agent* shows each agent's icon, and *Status* the mark its sessions show. **Include archived** adds the archived sessions. **Reset** clears the filters. While a filter is on, a **Clear filters** button beside the menu's button clears them too.
- **Collapse all groups** collapses every group, and **Expand all groups** opens them again.

Pinrail keeps these choices in its settings.

To make the sidebar wider or narrower, drag its right edge, or focus the edge and press <kbd>←</kbd> or <kbd>→</kbd>. Double-click the edge to restore the usual width. Pinrail remembers the width on this computer.

## Element tiles and the periodic table

Every session is named after a chemical element. To see the elements, turn on **Element tiles** in [Settings › Appearance](/docs/using/settings/#appearance). A session's header then shows its element's tile, with a card about the element when you point at it, and the Sessions page can filter by element.

**Show the periodic table**, in the command palette, shows which elements your sessions have found. Click an element to keep its card open.

## What the agent is doing

An agent that runs `pinrail status` reports what it is doing, in one line, such as *Running the integration tests*. Each status shows in the thread at the moment it was sent, between the questions and reviews. Several in a row show as one, the newest, with the earlier ones behind *N earlier*.

When the agent starts a stage of its work, the thread shows *Started* with the stage, and the stage becomes the session's title, in the thread, on the Sessions page and in the sidebar. When the agent says it has finished, the thread shows *Finished* with what it finished with. An agent that gets more work in the same session starts a new stage, and the session takes its title. The panel at the right lists the session's stages, oldest first: each with how long it took: until the agent finished it, until the next stage started for one it left unfinished, or until now for the one it works on. Hover over a stage to see when it started. Click a stage to go to where it started in the thread. When a session has more than 8 stages, the panel shows the latest 8. Click *N earlier stages* above them to show the rest.

![A session's thread: statuses before and after an answered question, and the agent saying it has finished.](screenshot:session-status "The agent's statuses around a question you answered, up to the end of its work.")

The newest status also shows under the session's id on the Sessions page, and a session the agent finished shows *Finished* beside its title once nothing waits for you. A status sent after *Finished* shows the agent working again.

## Notes

An agent can also send a note: a message in Markdown that tells you something and asks nothing, such as a summary of a stage, a decision it took on its own, or what an investigation found. A note shows in the thread as a message under the agent's icon, with its headings, lists, tables, links and code. Code in a language the note names, such as `rust`, is highlighted. A long note is folded: click *Show more* to read it all.

A note can carry files, such as a chart, a screenshot or a log. An image the note refers to shows in place, and every file is listed at the bottom of the note with its size. Click the save button beside a file, or a link to it in the note, to save a copy where you choose.

A note is information only. It does not wait in the inbox, and it is not the latest status that the Sessions page and the sidebar show. A note after *Finished* leaves the session finished.

## Keys

| Keys | Action |
|---|---|
| <kbd>1</kbd> to <kbd>9</kbd> | Pick a choice; <kbd>1</kbd> and <kbd>2</kbd> answer yes or no |
| <kbd>N</kbd> / <kbd>P</kbd> | Next / previous question |
| <kbd>C</kbd> | Add a note to the answer |
| <kbd>Enter</kbd> | Save a note or an answer; send, in the comment box |
| <kbd>⇧Enter</kbd> | A new line |
| <kbd>Esc</kbd> | Leave the field |
| <kbd>⌘↵</kbd> | Send the answers |
| <kbd>⌘D</kbd> | Decline the questions |
| <kbd>⌘⇧B</kbd> | Show or hide the session's details |
| <kbd>⌘P</kbd> | Go to a session |
| <kbd>⌘N</kbd> | Start a new session |
| <kbd>Ctrl+&#96;</kbd> or <kbd>⌘⇧T</kbd> | Switch between the summary and the terminal |
| <kbd>⌘F</kbd> | Find in the thread |
