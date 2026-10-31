---
title: Questions and sessions
description: "How you answer the questions an agent asks, the thread of everything one agent session sent you, and the statuses that show what the agent is doing."
---

An agent asks you short questions with `pinrail ask`, and reports what it is doing with `pinrail status`. Both belong to the agent's session: one run of the agent, such as a Claude Code conversation. Pinrail shows each session as a thread, with its questions, its reviews and its statuses in the order they happened.

![An agent session's thread: three questions waiting, the first answered with a note, and the box for a comment to the agent at the bottom.](screenshot:ask-thread "Questions waiting in a session's thread.")

## Answering questions

Questions wait in the inbox like reviews, marked with a question icon and *asks you*. Open them and they show in their session's thread, in the panel beside the inbox or as a page. The questions an agent asks together are one message, numbered *1/3*, *2/3* and so on, and are sent back together.

- **Pick a choice** with a click, or press its number, <kbd>1</kbd> to <kbd>9</kbd>. A yes-or-no question takes <kbd>1</kbd> for yes and <kbd>2</kbd> for no. A star marks the answer the agent recommends. Click a picked choice again to unpick it.
- **Pick several** where the question says *Select all that apply*.
- **Answer in your own words** with *Other…*, which opens a field under the choices. A question without choices has its field open from the start.
- **Add a note** to an answer with the button at the right of the question, or press <kbd>C</kbd>. The agent receives the note with the answer.

In a field, <kbd>Enter</kbd> saves what you wrote and <kbd>⇧Enter</kbd> starts a new line. In the answer to a question without choices, <kbd>Enter</kbd> goes on to the next question. A saved note or answer shows under the question, with a pencil to edit it and a bin to delete it when you point at it. <kbd>Esc</kbd> leaves a field, so that the keys pick choices again.

The keys answer the current question, which shows its keys on its choices. A key that picks a choice moves on to the next question that has no answer yet. <kbd>N</kbd> and <kbd>P</kbd> go to the next and the previous question, and a click makes a question the current one.

### Sending the answers

The box at the bottom of the thread holds a comment to the agent on all the questions at once. You can dictate it with the microphone in the macOS app.

- **Send** with <kbd>Enter</kbd> in the box, the arrow button, or <kbd>⌘↵</kbd> from anywhere in the thread. A question you left alone goes back as not answered, and the arrow button's tooltip says how many there are.
- **Decline** with the button in the box, or press <kbd>⌘D</kbd>, when the agent should not have asked. The agent is told to stop and is given your reason.

Once you have sent them, the answers show under their questions, with your notes and your comment. The thread stays open, so you can see what the agent does next.

## Sessions

Each session's thread lists what the agent sent from it, oldest first: its questions, its reviews, which open when you click them, and its statuses. The header's title is the stage of work the agent is on, when it named one, with the agent, the project and the branch under it. The header also shows how many questions and reviews the session holds, and what waits for you. The panel at the right lists what waits on you, and the session's details: the agent, the session id, the project, the branch, the folder the agent works in and its last activity.

The panel lists the session's starred reviews under *Starred*, newest first, and marks a starred review with a star wherever it is listed. It also lists the session's reviews that have ended, with their outcomes, folded under *Reviews* until you open it. Click one to open it.

To hide the panel at the right, or show it again, use the button at the right of the header, or press <kbd>⌘⇧B</kbd>. Drag its left edge to resize it. Pinrail remembers both.

A question's thread links to its session's page with *session* and the id in its header. A review sent from a session has a button in its header, on its page and in the side panel, that opens the session's thread.

![The Sessions page: agent sessions with what waits in each, what each holds, its project and its last activity.](screenshot:sessions "Sessions waiting for you come first.")

The **Sessions** page, in the sidebar, lists every session that asked you something, sent a review or reported a status: those waiting for you first, then the most recently active. Search for an agent, a session id or a title, and choose a project in the *Project* menu to list only its sessions. Click a session to open its thread in the side panel, beside the list, as the inbox opens a review. Press <kbd>J</kbd> and <kbd>K</kbd> to open the next and previous session, and <kbd>Esc</kbd> to close the panel. The panel hides the session's details at first: show them with the button in its header. *Open as a page* opens the session on its own page. When *Opening a review* is set to *As a page* in [Settings](/docs/using/settings/), sessions open on their own page too.

To archive a session you are done with, click the archive button at the end of its row, or press <kbd>E</kbd> on the focused row. You can also archive the open session with the button in its header, or with <kbd>E</kbd>. An archived session leaves the Sessions page and the sidebar, but nothing it sent is deleted: its questions and reviews stay in the inbox while they wait, and its thread still opens from a review's header and from the command palette. Turn on *Show archived*, beside the *Project* menu, to list the archived sessions with the others, and unarchive one with its button or <kbd>E</kbd>. When an archived session's agent sends a question, a review or a status, the session comes back by itself.

## Sessions in the sidebar

The sidebar lists every session that is not archived, under its repository, in the *Repositories* section. Click a repository's name to see its reviews in the inbox, and click the arrow beside it to collapse or expand it. Click a session to open its thread.

The agent's icon shows what the session is doing. A pulsing blue dot means the agent reported a status in the last five minutes. A green check means its last status said it had finished. An amber count at the end of the row says how many questions and reviews wait in the session, and they are listed under it, each opening where you answer or decide it. A review that came from no session is listed under its repository. Hover over a session to see its latest status, the agent and the branch.

The button at the right of the section's heading opens a menu:

- **Grouping** groups the sessions by repository, by status, by when they were last updated, or not at all. The heading takes the grouping's name.
- **Ordering** lists what waits first, then the working, quiet and finished sessions, or the most recently updated first.
- **Show details** adds a second line under each session. **Details** chooses what it shows: the last activity, the latest status, the branch and the agent.
- **Show reviews** lists what waits under its session, or only counts it.
- **Filters** list only some agents, repositories or states, or add the archived sessions. **Reset** clears them.
- **Collapse all groups** collapses every group, and **Expand all groups** opens them again.

Pinrail keeps these choices in its settings.

To make the sidebar wider or narrower, drag its right edge, or focus the edge and press <kbd>←</kbd> or <kbd>→</kbd>. Double-click the edge to restore the usual width. Pinrail remembers the width on this computer.

## What the agent is doing

An agent that runs `pinrail status` reports what it is doing, in one line, such as *Running the integration tests*. Each status shows in the thread at the moment it was sent, between the questions and reviews. Several in a row show as one, the newest, with the earlier ones behind *N earlier*.

When the agent starts a stage of its work, the thread shows *Started* with the stage, and the stage becomes the session's title, in the thread, on the Sessions page and in the sidebar. When the agent says it has finished, the thread shows *Finished* with what it finished with. An agent that gets more work in the same session starts a new stage, and the session takes its title. The panel at the right lists the session's stages, oldest first: each with how long it took: until the agent finished it, until the next stage started for one it left unfinished, or until now for the one it works on. Hover over a stage to see when it started. Click a stage to go to where it started in the thread.

![A session's thread: statuses before and after an answered question, and the agent saying it has finished.](screenshot:session-status "The agent's statuses around a question you answered, up to the end of its work.")

The newest status also shows under the session's id on the Sessions page, and a session the agent finished shows *Finished* beside its title once nothing waits for you. A status sent after *Finished* shows the agent working again.

Agents report their statuses when their Pinrail skill tells them to. To update the skill, choose *Update* next to each agent in *Settings › Agents*. See [Settings](/docs/using/settings/#agents), and [Reporting progress](/docs/agents/cli/#reporting-progress) for the command.

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
