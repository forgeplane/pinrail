---
title: Work items
description: "Record the tasks you and your agents work on, with sub-items and dependencies, and let agents take the next one that is ready."
---

A work item is a task recorded in Pinrail, such as a feature, a bug or a step of a plan. You add work items in the app, and agents add, take and close them with `pinrail items`. Each item belongs to a repository, and can have sub-items and depend on other items. Pinrail works out which items are ready, so that an agent can take the next one without asking you.

## Keys

Each repository numbers its work items under a prefix of a few capital letters taken from its name, such as `PIN` for `pinrail`. An item's key is the prefix and the number, such as `PIN-42`. Inside the repository's git checkout, an agent can also name an item by its number alone.

To change a repository's prefix, choose **Change key prefix…** in the **…** menu of its row on the [Projects](/docs/using/projects/) page. The items take the new prefix, and their old keys still find them, so links and commit messages that name an old key keep working.

A key in a description, a comment, a note or a message, such as `PIN-42`, links to its item, wherever Pinrail shows the text.

## Status

An item is in one of four states:

- **Open:** nobody is working on it yet.
- **In progress:** a session, or you, has claimed it.
- **Done:** its work is finished.
- **Cancelled:** it will not be done. Cancelling an item also cancels its sub-items that are still open or in progress. Pinrail asks first when an item has any.

An open item is **ready** when every item it depends on is done and it has no open sub-item. An item waiting for another is **blocked**. A parent item waits for its sub-items in the same way. A parent's row shows how many of its sub-items are closed, with a ring that fills as they close, and how many are in progress. When its last open sub-item is done or cancelled, and at least one of them is done, the parent is done too, unless someone works on it or something of it waits for you. Turn this off in Settings › Work items to close parents yourself. Pinrail refuses a dependency or a parent that would make items wait for each other in a circle.

## The Work items page

Open **Work items** in the sidebar. The count beside it is the number of items that are open or in progress.

![The Work items page: a plan nested under its item, with one step done, one in progress and two waiting for it, and the step in progress open in the panel beside the list.](screenshot:work-items "Work items, with one open in the panel.")

- **Add an item** with the field at the top of the page, or press <kbd>C</kbd>. Type its title, and choose its repository and, if you like, its parent. Press <kbd>Enter</kbd> to add it. From any other page, press <kbd>⌘⇧C</kbd>, or choose *Add a work item* in the command palette, to add one in a dialog without leaving the page. The dialog starts on the repository of the session or the review you are looking at, and a message names the item once it is added. <kbd>⌘Enter</kbd> adds it and keeps the dialog open for the next one.
- **Filter** the list with the **Filter** menu at the right of the header: by status, by a project or a repository, and by who created the item. *Open* shows the items that are open or in progress. **Waiting on you** shows the items with a review or a question that waits for you. Each filter that is on shows as a chip under the header: click its × to remove it, or **Clear filters** to remove them all. Search titles and descriptions with the search field, or press <kbd>/</kbd> to go to it, and press <kbd>Esc</kbd> to leave it and use the keys on the results.
- **Display** the list as **Display options** says. *Grouping* groups the items by project or by repository, or shows them as one list. *Ordering* lists them oldest or newest first, recently updated first, by status, or by title, and orders sub-items under their parent the same way. *Sub-items* shows them nested under their parent, or each as a row of its own.
- **Make an item a sub-item** by dragging it onto another item. An item dropped on an item of another repository stays in its own repository and keeps its key.
- **Move an item to another repository** by dragging it onto another group's heading, which is outlined while you drag. Grouped by project, a project with several repositories asks which one. The item takes the next number there, and its old key still finds it.
- **Move** through the list with <kbd>J</kbd> and <kbd>K</kbd>, and open the selected item with <kbd>Enter</kbd>.

On the selected item, <kbd>S</kbd> claims it for you or releases it, <kbd>D</kbd> marks it done or opens it again, <kbd>X</kbd> cancels it or opens it again, and <kbd>H</kbd> puts it on hold or takes it off hold.

Put an item on hold to keep it without starting it. No agent takes it, nor any of its sub-items: `pinrail items next` passes over them, and an agent that tries to claim one is told to ask you. An item on hold still blocks the items that depend on it. It is listed with the open items, dimmed; choose *On hold* in the status filter to list only the items on hold. Putting an item on hold releases it and each of its sub-items that is in progress, and each agent working on one is told to stop the next time it runs `pinrail status`. When there is such work, Pinrail lists it and asks you first. Only you put an item on hold or take it off.

Queue the items you want done first, in the order you want them. Press <kbd>Q</kbd> on an item, or click *Queue* in its panel, to add it to the end of the queue, and press <kbd>Q</kbd> again to take it out. Each queued item shows the queue icon and its place in its row: press <kbd>⇧↑</kbd> and <kbd>⇧↓</kbd> on a queued item to move it up or down the queue. `pinrail items next` offers the first ready item of the queue in the agent's repository before any other, after the sub-items of what the agent already holds. An item stays queued while an agent works on it, and leaves the queue when it is done, cancelled or put on hold. There is one queue across all your repositories, and only you change it.

## An item's panel

An item opens in a panel beside the list. The panel shows:

- the item's title and description, which you can edit in place, or with <kbd>T</kbd> and <kbd>E</kbd>;
- its status, its repository, its parent and its link, which you change from their controls;
- a graph of what the item depends on and what it blocks, and its sub-items;
- its reviews and questions, and the sessions that worked on it;
- its comments and activity: every change, with who made it and when.

To attach an image or a file to the description or to a comment, paste it, drop it on the field, or use the paperclip. Pinrail uploads it and links it where the cursor is, and shows an image in place. The files of the description are listed under it: point at one and click its × to remove it, with its link.

Add a comment in the field at the end of the panel, and post it with ⌘Enter or the arrow. Enter starts a new line, and in the macOS app you can dictate the comment with the microphone. When a session holds the item, your comment reaches the agent the next time it runs `pinrail status`, like a message.

The buttons in the panel's header copy the item as Markdown, open its link, and delete it. Deleting an item also deletes its sub-items, and cannot be undone. Its reviews stay in Pinrail.

## Work items elsewhere in the app

- **Reviews:** a review that belongs to work items shows each item's key and title in its inbox row and in its header. In the header, the × on an item takes the review, with all its rounds, out of it, and **Add to a work item…** adds it to another.
- **Sessions:** a session's header names the item it is working on, and its details list every item it worked on. Its thread shows the comments its agent wrote on work items, each with the item's key and title, among its statuses and reviews, and a short line each time the agent added an item, started or stopped working on one, or changed its status.
- **Command palette:** <kbd>⌘K</kbd> finds a work item by its key or its title.

## How agents use work items

An agent with the `pinrail` skill knows the commands. When you ask it to work through the items, it takes the next ready item of its repository, reads it, does the work, and marks it done with a comment that says what changed and where.

```sh
pinrail items next --claim
pinrail items show PIN-42
pinrail items done PIN-42 --comment "Loader in core/src/themes.rs, commit 3f2a1c."
```

`pinrail items next` chooses among the ready items of the current repository. A sub-item of an item the session already holds comes first, then the first ready item of your queue. After that, the item that unblocks the most other items comes first, then the oldest. There is no priority field: to choose what comes next, queue it.

While a session holds items, the reviews and questions it sends belong to all of them, unless the agent names the ones they are for with `--item`. A review can belong to several items, and every round of it belongs to the same ones. In the review's header, the × on an item's chip takes the review out of that item, and **Add to a work item…** adds it to another. An item can be marked done only when each of its sub-items is done or cancelled, by you or by an agent. An agent must also wait until no review or question of the item waits for you.

An agent that plans a larger piece of work can record the plan as a breakdown: a JSON file of items with their parents and dependencies, which `pinrail items add --file` adds in one request. Pinrail adds all of the items or none of them.

The [CLI reference](/docs/reference/cli/) lists every `pinrail items` command and option.
