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

The panel also lists the session's reviews that have ended, with their outcomes, folded under *Reviews* until you open it. Click one to open it.

To hide the panel at the right, or show it again, use the button at the right of the header, or press <kbd>⌘⇧B</kbd>. Drag its left edge to resize it. Pinrail remembers both.

A question's thread links to its session's page with *session* and the id in its header.

![The Sessions page: agent sessions with what waits in each, what each holds, its project and its last activity.](screenshot:sessions "Sessions waiting for you come first.")

The **Sessions** page, in the sidebar, lists every session that asked you something, sent a review or reported a status: those waiting for you first, then the most recently active. Search for an agent, a session id or a title. The sidebar's *Sessions* section lists the sessions waiting for you, with how many questions and reviews wait in each.

## What the agent is doing

An agent that runs `pinrail status` reports what it is doing, in one line, such as *Running the integration tests*. Each status shows in the thread at the moment it was sent, between the questions and reviews. Several in a row show as one, the newest, with the earlier ones behind *N earlier*.

When the agent starts a stage of its work, the thread shows *Started* with the stage, and the stage becomes the session's title, in the thread, on the Sessions page and in the sidebar. When the agent says it has finished, the thread shows *Finished* with what it finished with. An agent that gets more work in the same session starts a new stage, and the session takes its title. The panel at the right lists the session's stages, oldest first: each with when it started, how long it took once the agent finished it, and *In progress* for the one the agent works on now. Click a stage to go to where it started in the thread.

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
