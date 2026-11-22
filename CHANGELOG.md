# Changelog

All notable changes to the Pinrail app are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Changes are grouped by kind. An _Unreleased_ section is added while changes
wait for a release, and takes the version's name when it is tagged.

The `pinrail` command ships inside the app and carries the app's version.

## [Unreleased]

### Added

- Resume a session in another folder when its own was moved or deleted,
  with **Choose another folder…**.
- Ask what to do with the running agents when you quit Pinrail: leave them
  running, or stop them first. **Quitting with agents running** in Settings
  › General sets the answer once for all.
- Add **Add and start** to the quick add. It adds the work item and starts
  an agent on it, and ⌥Enter does the same.
- Add a Notifications section to Settings. It holds the system notification
  settings, an inbox switch and a system notification switch for each kind
  of notification, quiet hours, and the muted projects and repositories.
- The inbox is one list of notifications, with the tabs **All**, **Waiting
  on you**, **Unread** and **History**. **All** shows what waits on you
  first: reviews, questions and agents that wait for a permission or an
  answer. Below it, **Updates** lists what agents did. The **Kind**,
  **Project or repository** and **Plugin** filters each take several
  values, and **Show done** lists the notifications marked done.
- Every row of the inbox has the same layout. An icon shows what the row
  is about, such as a work item, a session or a review. A badge on the icon
  shows what happened, such as added, commented, done or failed.
- Each row of the inbox says what changed since you last looked, such as
  "claude-code added this work item with 10 sub-items", "3 new comments",
  or the first line of an agent's last reply.
- Opening a work item or a session from the inbox shows it in the side
  panel beside the list, as a review already opens, so you stay in the
  inbox. **J** and **K** move the panel through every row.
- Mute a work item's or a session's notifications with the bell in its
  header, or from its row in the inbox with **M**. The bell unmutes them.
- The inbox lists updates under the reviews that wait: a work item an
  agent added, commented on or closed. Mark an update read or unread with
  **U**, mark it done with **E**, and mark every update read with
  **Shift+U** or **Mark all as read**.
- The sidebar's **Inbox** entry shows how many updates are unread, after
  the number of reviews that wait for you.
- Choose in Settings › Work items › Inbox which work item notifications
  the inbox adds: an agent's new work items, its comments, the items it
  closes or claims, and its other changes.
- A button beside the **Parent** of a work item opens the parent item.
- Antigravity CLI reports its status to Pinrail. Turn on **Report status**
  for it in Settings › Agents, and its sessions show their turns, prompts
  and replies, and whether the agent is working or finished.
- Start and resume Antigravity CLI sessions from Pinrail, and send them
  messages, which are typed into the agent's terminal.
- Grok CLI reports its status to Pinrail. Turn on **Report status** for it
  in Settings › Agents, and its sessions show their turns, prompts and
  replies, and whether the agent is working, waits for a permission or an
  answer, or is finished.
- Start and resume Grok CLI sessions from Pinrail, and send them messages,
  which are typed into the agent's terminal.
- Copilot CLI reports its status to Pinrail. Turn on **Report status** for
  it in Settings › Agents, and its sessions show their turns, prompts and
  replies, and whether the agent is working, waits for a permission or an
  answer, or is finished.
- Start and resume Copilot CLI sessions from Pinrail, and send them
  messages, which are typed into the agent's terminal.
- Pi reports its status to Pinrail. Turn on **Report status** for it in
  Settings › Agents, which installs a Pinrail extension in Pi's extensions
  folder, and its sessions show their turns, prompts and replies, and
  whether the agent is working, waits for an answer, or is finished.
- Start and resume Pi sessions from Pinrail, and send them messages, which
  are typed into the agent's terminal.
- Cursor's CLI reports its status to Pinrail. Turn on **Report status** for
  it in Settings › Agents, and its sessions show their turns, prompts and
  replies, and whether the agent is working, waits for a permission, or is
  finished. A permission shows while you are not looking at its terminal.
- Start and resume Cursor's CLI sessions from Pinrail, and send them
  messages, which are typed into the agent's terminal.
- OpenCode reports its status to Pinrail. Turn on **Report status** for it
  in Settings › Agents, which adds a Pinrail plugin to OpenCode's plugin
  list, and its sessions show their turns, prompts and replies, and whether
  the agent is working, waits for a permission or an answer, or is
  finished.
- Start and resume OpenCode sessions from Pinrail, and send them messages,
  which are typed into the agent's terminal.
- Start an agent on a work item from the item's panel. The new session
  gets the item's title and description as its task, and claims the item.
- A session that Pinrail runs has a **Split view** beside Summary and
  Terminal, with the summary above the terminal. Drag the divider between
  them to resize the summary; the size is kept for every session. Settings ›
  Terminal can open sessions in it.
- Delete a session from its menu in the sidebar or in its header. Pinrail
  stops its agent if it runs it, withdraws the questions and reviews that
  still wait, and gives up the work items it holds; decided reviews stay in
  History.
- Every session has a name of its own, an adjective and a chemical element
  such as **Careful Calcium**. A session with no stage yet shows it, and
  its header shows the element's tile, with a card about the element on
  hover. Click a session's name in its header to rename it. **Show the
  periodic table** in the command palette shows which elements your
  sessions have found. Click an element to keep its card open. The first
  session to find a rare element announces it.
- Settings › Appearance chooses how tables look in messages, notes and
  work item descriptions: bordered, with lines between rows, or striped.
  **Table width** makes them fit their content or fill the full width.
- The Work items page searches the full text of the work items, best
  match first, and shows where each one matched. The button at the left
  of the search field, or ⌥T, switches to searching the titles only.
- `pinrail items search` finds the work items, open and closed, that match
  its words in their title, description or comments, best match first.
  Several phrasings of one question find more.
- `pinrail items add` lists the three work items most like the new one,
  and the Pinrail skill tells agents to search before they add an item.
- Typing `#` in the message box under a session lists the work items, as
  it does in a work item's comment or description.
- **Count sub-items at every level**, in Settings › Work items, makes a
  parent's progress on the Work items page count every work item under
  it, not only its own sub-items. A work item's panel and `pinrail items`
  show both counts when they differ.
- Each session in the sidebar has a menu, from the **⋯** button on its row
  or a right-click: archive it, mark it read or unread, copy its id, and
  stop or resume its agent when it runs in Pinrail.
- The terminal shows Nerd Font icons, such as those in prompts and file
  listings, without a patched font: Pinrail carries the symbols-only Nerd
  Font and loads it once a screen shows an icon.
- Settings has a **Terminal** section for the terminal of an agent that
  runs in Pinrail: its renderer, Option as Meta, the screen reader,
  clipboard writes, and the view a session opens in.
- **Show next item** in the Work items page's **Display options** marks
  each repository's next item with **Next**: the one `pinrail items next`
  gives an agent.
- The Work items page has **Fold all** beside the search field, which
  folds every parent, and **Unfold all** once they are folded.
- On the Work items page, a done or cancelled item starts with its
  sub-items folded. **Fold done parents** in Settings › Work items turns
  this off.
- The work items search finds an item by its number or key, such as
  `42`, `#42` or `PIN-42`, as does `pinrail items list --search`.
- Settings has a **Labs** section for features still being built. Its
  first switch, **Run agents in Pinrail**, starts the terminal host that
  will run agents for Pinrail and keep them running after the app quits.
- A session whose agent runs in Pinrail has a **Summary** view and a
  **Terminal** view, which shows the agent's terminal and takes your
  typing. Ctrl+` or Cmd+Shift+T (Ctrl+Shift+T on Linux) switches between
  them, and each session remembers the view you used last.
- Pinrail can show what each Claude Code session is doing. Turn on
  **Report status** for Claude Code in Settings › Agents, and Pinrail adds
  its hooks to Claude Code's settings, leaving your own settings as they
  are.
- Each session shows whether it needs you, finished a turn, is working,
  is idle or has stopped. The mark appears on the session in the sidebar
  and on the Sessions page, and a chip beside the session's title says
  what it waits on or how long it has worked.
- The sidebar's **Priority** sort lists the sessions that need you first,
  and the Sessions page has a **Needs you only** filter.
- The inbox has an **Agents** section above the reviews. It lists the
  sessions that wait for you in their terminal, such as for a permission,
  and those that finished a turn you have not seen.
- A finished session counts as seen once you have had it open for a
  second. **Mark as unread** in the session header shows it as finished
  again.
- A stopped Claude Code or Codex session can be resumed in Pinrail with
  **Resume** in the session header, including a session you started in
  your own terminal.
- A Claude Code session still running in your own terminal can be opened in
  Pinrail with **Open in Pinrail**, after a warning that it starts a second
  Claude Code on the session.
- You can write to an agent from its session's thread. A message to an agent
  Pinrail runs is typed into its terminal as your prompt once the agent is
  idle, and the thread shows when it arrived. Any other agent receives the
  message the next time it reports a status.
- After the computer restarts, the Sessions page says which agents the
  restart stopped, with **Resume all** to resume them one by one.
- A session's summary shows a question the agent asks in its terminal, as
  it shows a permission, with **Open the terminal** to answer it.
- Clicking an image attached to a note or a question opens it in a pop-up.
  The arrow keys move between the images.

### Changed

- Show a sidebar session's full title in its tooltip, with its state,
  what it is doing or waits for, its agent and branch, and what waits on
  you. The element tile is gone from it.
- Mark questions that were withdrawn, declined or expired with a badge
  in a session's thread, and show the reason in full colour.
- Hide a session's details at first in a narrow window, so the terminal
  keeps its width. The details button, or ⌘⇧B, shows them over the
  thread, and Esc hides them again.
- Show an ended run in a session's Terminal view as one message, with
  how and when it ended and a **Resume** button, above its last screen
  while Pinrail keeps it.
- Make the divider of a session's split view easier to find. Its handle
  is larger, and a line across it shows on hover and while you drag it.
- Keep an agent's text readable in a light theme. Colours that nearly
  vanish on the background, such as a white bullet, are darkened just
  enough to read.
- Show the state of a session's terminal with a coloured dot in the bar
  below it: viewing, typing, reconnecting or disconnected.
- Move a session's generated name from its header to the details panel.
  In a narrow window, the header hides the start time and shows Stop and
  Resume as icons, so the title keeps its room.
- Rearrange the quick add for narrow widths. **Add** and a new **Add more**
  button sit beside **Cancel** on the title line. The repository, the
  parent and **Add and start** share the row under the description, and
  the key hints stay on one line.
- Archiving a session stops its agent when Pinrail runs it.
- System notifications follow the inbox: a kind of notification turned off
  in the inbox, a muted project or repository, or a muted work item or
  session is never announced.
- When an agent Pinrail runs goes on in a new session, as after
  `/clear`, its open session page follows the terminal there, and the
  session it left links to the new one, also from its **Terminal** view.
- Starting an agent from Pinrail opens its session at once, also for
  Codex and Antigravity CLI, which report their own session id later.
  The session's details show that id as **Agent session**, and **Copy
  session id** copies it.
- Starting Codex or Antigravity CLI from Pinrail needs **Report status**
  on, since the agent tells Pinrail its session only through its hooks.
  The **New session** page says so and turns it on with one click.
- A session that Pinrail runs takes its status from its terminal when its
  agent reports there, as Claude Code does. It shows Working, Needs you
  with the prompt's text, and Finished even with **Report status** off.
- A session whose agent reports no state, through hooks or its terminal,
  no longer shows Working after a status or Stopped after its last stage.
  A status names the work, not whether the agent still works.
- Messages the agent has not received yet wait at the top of the message
  box, a line each, where you can delete them. A message joins the session's
  thread when the agent receives it, so it no longer sits above what the
  agent did after you sent it.
- A session's header has the same two rows for every session: the title
  and its state, then where the agent works, since when, and the work
  items it holds. Archive, Mark as unread and Copy session id are in the
  **⋯** menu beside the title, the same menu as the session's row in the
  sidebar. The counts of reviews and questions are in the session's
  details.
- The Pinrail skill tells agents to fence a diff that has a line of three
  backticks with four, so that the diff shows whole in a question. Update
  the Pinrail skill in Settings › Agents for the new advice.
- An agent can no longer claim a work item while an item it depends on
  is open. You can still claim it in the app.
- Ordered by status with sub-items shown, the work items list puts a
  parent first when any item under it is in progress.
- A session you open again shows its thread at once, where you left it,
  or at its newest message if you were there.
- When you come back to the inbox, Sessions, Work items or Projects, the
  page keeps its open side panel and its scroll position.
- The Terminal view of a session that runs, or ran, in another terminal
  explains what opening or resuming it in Pinrail does, in the middle of
  the view.
- The Work items page remembers which parents you folded or unfolded,
  across page changes and restarts.
- A new project starts with the colour fewest projects have, so projects
  look different while unused colours remain.
- A session's thread shows what the agent is doing on a line above the
  message box: that it is working, whenever its hooks report so, or that
  it waits for your permission.

### Security

- Only the Pinrail app can now act as you. Sending an agent a message,
  changing settings, projects or themes, and any decision, discard or
  work-item change made as you need a token that only the app's window
  holds, so another program on your computer can no longer do these in
  your name. The `pinrail` command still works as before.

### Fixed

- Keep the Work items list in place when a parent folds or unfolds
  beside an open item. It jumped back to the top.
- Open the inbox filtered to a project or repository from the sidebar,
  the Projects page and the command palette. These links opened the whole
  inbox.
- Fix ⌘D, which declines questions or discards a review, with a review
  open beside a session's thread. From the thread it now declines the
  thread's questions, and from the review's panel it discards the review.
- Count everything that waits on you in the inbox's **Waiting on you**
  heading, with a link to the tab when the list shows only the first 200.
- Count in the inbox's tabs what the filters and the search keep, and show
  in its title how much of the whole that is, such as "1 of 2".
- Keep a notification's title readable in a narrow inbox beside an open
  review.
- Stop notifying about sessions the sidebar leaves out, those known only
  from an agent's hooks and archived ones. An agent that waits for you
  still notifies you, and its session shows in the sidebar again.
- Show in a session's summary when an agent running in its own terminal
  waits for a permission, an answer or a sign-in, and where to answer it.
- Offer only a repository's checkouts as its folders when starting an
  agent. A folder that a `pinrail` command ran from outside a checkout no
  longer becomes the repository's default folder.
- `pinrail items add --file` refuses a breakdown when one of its items has
  a field it does not know, such as `description` instead of `body`. The
  error names each field and the item's ref, and no item is added.
- An agent started on a work item now gets the item's comments in its
  first prompt. Each attached file is named in the prompt, with the
  `pinrail items show` command that saves it.
- The inbox's side panel keeps the same width whether it shows a work
  item, a session or a review.
- The inbox's title, search box, **Mark all as read** and **Filter** stay
  on one line in a narrow inbox, where **Mark all as read** shows only its
  icon. The inbox's tabs no longer wrap.
- The side panel covers the list when the two do not fit side by side,
  also when a page opens with the panel already open.
- A session page follows its agent to the new session when the agent
  starts a new conversation in Pinrail's terminal, such as after `/clear`.
- A question opened from the inbox shows at once in the side panel, and
  its session's thread scrolls to it, also in a long session.
- A session's thread keeps an agent's earlier reply when the agent
  replies again in the same turn, as it does when a background job ends.
- ⌘Enter in the quick add dialog on the new session page adds the work
  item without starting the session.
- ⌘⇧C opens the quick add while a review has the focus. The Navigate
  menu also has **Add Work Item…**.
- A table in a message or note no longer splits short words, such as an
  id or a time, across lines to make room for a long column.
- A message typed into a session that Pinrail runs no longer shows as not
  confirmed when its agent's hooks are off. The agent starting work
  confirms it, and the next waiting message is typed when the turn ends.
- A session started from Pinrail is listed under its repository and branch
  from the start, also when its agent does not report status.
- Messages you write to a session while it is not running are typed into
  its terminal once you resume it in Pinrail.
- With a session open, the sidebar marks only the session's own row, not
  Sessions as well.
- A filter's search field keeps the focus when you open its menu by
  pointing at it and then click it.
- A message typed in a session's message box stays there when you open
  another page or session and come back.
- A session's terminal no longer touches the left edge of its view.
- A diff in a question's context no longer starts with an empty file
  when it opens with a blank line.
- `pinrail status` says the status was sent when no stage is open, so
  agents no longer send a note twice. Update the Pinrail skill in
  Settings › Agents for the matching advice.
- **Closed sub-items** on the Work items page also shows the closed items
  under a closed sub-item, so a done sub-item with its own sub-items can
  be unfolded.
- Crash reports from the window keep each stack frame's function, file
  name, line and column. The address before the file name is still
  removed.
- In a narrow window, a work item's repository no longer runs under its
  badges. The repository is shortened first, and an item blocked by
  several others shows the first and how many more.
- An agent's reply in a session thread no longer shows **Show more** and
  a faded last line when it fits, after the window was made wider.
- Opening or closing the side panel on the Work items, Sessions or inbox
  page no longer scrolls the item you clicked out of view.
- `pinrail status` and the other session commands now find the session of
  Cursor Agent.
- A long session's thread now opens at its newest message. It used to
  stop short of the end, or scroll back up once the older messages
  loaded.
- The breadcrumb on a session's page now names that session after a quick
  switch between sessions, not the one shown before.
- A question opened from the inbox or a link now shows in its session's
  thread at once and stays in view. It used to show alone first, and in a
  long session it ended up out of view.
- Opening a work item or a review from a session's details keeps the
  thread where you were reading. The narrower thread used to move what
  was in view.
- An agent's reply now shows in a session's thread where it was written,
  after what the agent did during its turn, and a message you typed into
  the agent's terminal is no longer followed by an empty gap.
- The search fields of the inbox and of the Sessions, Work items and
  Projects pages keep text entered while the page updates. The text was
  sometimes dropped.
- A plugin view that hands over a decision is told only that it was
  submitted. It was sometimes sent the review again as read-only first.
- Esc and the arrow keys now always work in the What's new dialog. A key
  pressed while the app was updating was sometimes ignored.
- A key pressed while the app is updating is no longer ignored in a
  review, a menu, a session's questions or the shortcut field in Settings.
  Esc sometimes left the review panel or a menu open.

## [0.6.0]

### Added

- Anonymous usage data and crash reports. Pinrail records which features
  are used and how often, never what you review, and sends a report when
  it crashes, with file paths and quoted text removed. Both start once you
  answer the notice in the inbox, and you can turn either off in Settings ›
  Privacy, or with `DO_NOT_TRACK`. Show what is sent shows exactly what
  leaves your computer. See the new Usage data and crash reports page in
  the docs.
- Work items. Record tasks in Pinrail, with sub-items and dependencies,
  on the new Work items page. Each repository numbers its items under a
  key prefix, such as `PIN-42`. Agents add, take and close them with
  `pinrail items`. The reviews and questions an agent sends belong to
  the item it holds, or to the items it names with `--item`; an agent
  that holds several names them, or says with `--no-item` that it asks
  about none. A review can belong to several items, which its header lets
  you change.
- Open a work item in a panel beside the list, with its sub-items,
  reviews, comments and files. Drag an item onto another to make it a
  sub-item, or to another project or repository. Put items on hold, queue
  them for agents to take first, and attach files to them.
- Add a work item from any page with ⌘⇧C. Typing `#` in an item's text
  suggests other items to link.
- A session's work items open beside its thread, and the thread shows
  the changes the agent made to them and its comments on them.
- Open the Work items page from the menu bar icon's menu.
- `pinrail items show` saves the item's files to a temporary folder and
  prints the path of each.

- Projects. Group repositories into a project on the new Projects page,
  by dragging them under it. A project has a name, an optional
  description, and an avatar with a colour and an emoji or up to three
  letters. Agents need no change: a review or a session names its
  repository, and shows under the project that repository is in.
- Group the inbox, the sidebar's sessions and the Sessions page by
  project. A repository in no project is a group of its own. The Sessions
  page can also group its sessions by repository.
- Filter the inbox, the history, the Sessions page and the sidebar's
  sessions by a project or a repository, from a menu that you can type
  in.
- Go to the projects, create one, or show a project's reviews from the
  command palette.
- Delete a review, with every round of it, from the **…** menu of its row
  in the history, from its header or from the command palette. A round
  that still waits is discarded first, so the agent stops.
- Delete a repository from the Projects page. Its reviews, questions and
  sessions are deleted, and what still waits is discarded first, so the
  agents waiting on it stop.
- Press `/` to search the Projects page and the official plugins in
  Settings, as in the inbox and the Sessions page.
- Collapse all on the Projects page folds every group.
- The sidebar shows a Clear filters button while a filter hides
  sessions. Its Agent and Status filters show each agent's icon and each
  status's mark.
- A page keeps its filters when you leave it and come back.

- Find text in a session's thread with ⌘F, as in a review's view.
- `pinrail ask --suggest-reply` suggests the reply the agent expects. The
  empty comment box shows it, and Tab takes it.
- `pinrail ask --attach` sends a file with a question, such as a
  screenshot or a log. The question lists it, and its context can show an
  image in place.
- ⌘1 to ⌘4 go to the Inbox, Sessions, Work items and Projects pages.
- A fenced `diff` block in a question's context, a note or a work item
  shows as a diff view, one file at a time, with the old and new line
  numbers and the added and deleted lines marked. Buttons step through
  the files of a diff that has several, and a long file scrolls inside
  the view.

### Changed

- Sessions and questions are out of Labs and always on. Agents can ask
  you questions with `pinrail ask` and report their progress with
  `pinrail status`, and the Sessions page and the sidebar's sessions
  show their threads. Update the Pinrail skill in Settings › Agents, so
  your agents know these commands.
- Reads from the database no longer wait for a write or for another slow
  read, so the app and the CLI answer at once while a large list loads.
- The app and plugin views have the system's own scrollbars. On macOS
  they are thin, drawn over the content and shown only while it
  scrolls, as your scroll bar setting says, so a list's width no longer
  changes when it starts to scroll.
- The keyboard shortcuts dialog lists the keys in sections, those of the
  page you are on first. Show all shortcuts lists the others, and a search
  looks through every section.
- The inbox's layout is kept in the settings, so every window shows the
  same one.
- Where the inbox, the history and the Sessions page said "project" for
  a repository, they now say "repository".
- The history's star moved into the **…** menu at the end of each row. A
  starred round still shows its star.
- The inbox's Pending and New rounds tabs count what the filters show,
  and its title reads "3 of 11" while a filter is set.
- J and K move the focus to the row they highlight, so a screen reader
  reads it, and wrap around at the ends of every list. Closing the side
  panel puts the focus back where it was.
- A submenu stays open while the pointer moves toward it across other
  rows.
- Search and filter fields no longer complete, correct or capitalise what
  you type.
- The list plugin's warning about undecided items says how many of them
  have a note, which is not sent.

### Fixed

- The Sessions page and the history say why when their list cannot be
  loaded. They showed an empty list.
- Esc pressed the moment a review opens beside the list closes it. On a
  busy computer, the key could be lost while the review was loading.
- In a narrow window, a review or a session opened beside the list covers
  the whole list. It covered only part of it, with the list showing
  beside it.
- Choosing a filter or a display option closes the Filter or Display
  options menu, in the inbox, the Sessions page and the Work items page.
- The text of a session's thread, such as statuses, notes, messages and
  questions, can be selected and copied.
- The app shows every change made while it was disconnected, such as
  while the computer slept. After more than 1000 changes, the inbox,
  sessions and counts could miss some until the app was reloaded.
- A page that fails to load now shows the error and a Reload button,
  and the sidebar and the other pages stay usable. An error on one page
  used to leave the window blank.
- A theme file saved twice in quick succession now shows its last
  colours. An earlier version could stay.
- `pinrail` stops quietly when the program reading its output exits,
  such as `head`, as other command-line tools do.
- `pinrail ask --data` refuses a question that is not an object with an
  error, rather than stopping with an internal error.
- `pinrail status` and `pinrail open` answer in JSON with `--json` or
  `PINRAIL_JSON=1`, and `pinrail status` filters terminal control
  sequences out of the person's messages, as the other commands do.
- `pinrail wait`, `pinrail ask` and `pinrail submit --wait` refuse a
  `PINRAIL_TIMEOUT` that is not a whole number of seconds, such as `10m`.
  Such a value meant no time limit.
- A long reason for a withdrawn review now wraps at the width of the
  review cards in a session's thread.
- At the large interface size, menus, selects and tooltips open next to
  their button rather than off to the side.
- The app shows a setting changed from the command line or another
  window while it is saving a change of its own.
- On a session's page, what you click in the thread stays in place when
  the side panel opens or closes beside it.
- A note that shows `attachment:` links inside code, as an example, is no
  longer refused for files it does not carry.
- A note or work item whose `attachment:` link has a malformed escape,
  such as `%E0`, no longer blanks the app's window.
- Settings no longer tells AppImage users to install the CLI again after
  updating Pinrail. The app updates the installed copy when it starts.
- A value in `settings.json` that the app does not accept, such as a typo
  or a value from a newer Pinrail, stays in the file when the app saves
  another setting. The app still uses the default in its place.
- `history.keep_days` takes at most 36500 days. A much larger value set
  through the API or in `settings.json` stopped the app from deleting
  old history and expired reviews until it restarted.
- Esc pressed as soon as What's new opens closes it.
- Moving quickly from one review to another after a plugin update no
  longer brings back the review you left.
- **Restart to update** can be tried again after the install fails, such
  as when Pinrail runs from its disk image. It used to answer that no
  update was downloaded until the app was restarted.
- A block of code in a question's explanation, such as a diff, wraps its
  long lines inside the question, set apart as in a note.
- With a review open beside a session's thread, ⌘Enter, ⌘D and the
  answer keys act on the review, unless the focus is in the thread. They
  also sent or declined the session's waiting questions.
- ⌥↑ and ⌥↓ move the caret in a text field, and do nothing with a dialog
  open, rather than switching to another waiting review.
- Esc in the search field of the inbox, the Sessions page or the
  Projects page leaves the field and keeps the search, so the page's keys
  act on the results.
- Find in a plugin's view now finds text across the space between two
  styled words, such as **bold** followed by _italic_.
- A long session's thread opens at its newest message at once, and stays
  there as its images load.
- The Enter that confirms text typed with an input method, such as
  Japanese or Chinese, no longer submits the field, such as the command
  palette or a search.
- When an agent asked several sets of questions, Send now also sends the
  other sets whose questions are all answered. Answers picked in another
  set were left unsent.
- The app connects within a moment of starting. It showed Reconnecting
  for a few seconds when its window loaded before its server.
- A session's thread shows what its agent sent while the app was still
  connecting, such as a comment written as the app started. It showed it
  only once something else reloaded the thread.
- Two sessions in one repository that ask the same question each get
  their own question. The second session's thread showed nothing, and the
  answer went to the first.
- After the app reconnects, a session's thread shows the messages sent
  to it while the app was disconnected.
- Enter on a list row opens it once, so Back closes the side panel at
  the first press.
- A menu near the bottom of the window opens above its button, and the
  sidebar's submenus stay inside the window.
- In a narrow session thread, such as beside an open review, names and
  times no longer break over several lines.
- The chat box no longer hides the end of a session thread's scrollbar.
- The change-map and code review plugins send a comment that was typed
  but not added, and the code review plugin a rejection's reason that
  was not saved. A review could go back approved without them.
- The change-map plugin shows the review as read-only once it is sent,
  and clears the comment field when another part is selected.
- The code review plugin no longer shows the `diff --git` and `index`
  lines of a `git diff` as part of the diff.
- The image plugin keeps the regions drawn in a draft when the review is
  sent before its images have loaded.
- The markdown plugin limits a comment to the lengths the app accepts.
  A very long comment or quote made a decision the app refused.

### Security

- Updated rustix, whose extended attribute functions could mark unwritten
  memory as initialized when given an empty buffer (RUSTSEC-2026-0486).
- Updated rusqlite, whose bundled SQLite could return uninitialized
  memory when it opened a database with a crafted `-wal` file
  (CVE-2026-119375).
- A review's preview page in the browser asks before it opens a link
  that the plugin's view requests. It opened any web or email address,
  which could carry the review's content to another site.
- Updated the markdown plugin's Mermaid, DOMPurify, KaTeX and markdown-it
  for advisories, a critical one in DOMPurify among them.
- The code review plugin shows the change's link only for an `http` or
  `https` address. A `javascript:` link could run script in the view.

## [0.5.0]

### Added

- Themes. Add a theme as a JSON file in the themes folder, and choose a
  theme for dark and for light in Settings or in the command palette.
  Plugin views follow the theme.
- Add the Nord, Dracula, Catppuccin Mocha, Catppuccin Latte and Solarized
  Light themes.
- Settings › Labs, with features still in the works, each off until you
  turn it on. Questions and sessions are the first: while they are off,
  the sidebar lists the reviews that wait and the projects, the app
  refuses `pinrail ask` and `pinrail status`, and the skill leaves them
  out.
- Add a button to copy the session id in a session's header.
- The Help menu links to the documentation and the blog.
- `pinrail status` prints the current stage, or says that no stage is open.
- `pinrail ask` asks you one or more questions and prints your answers.
  Each question can offer choices, ask for yes or no, or take an answer
  in words, and you answer in the app.
- Questions open in a thread of the agent's session, beside its earlier
  questions and reviews. Pick a choice or answer in your own words, add a
  note to an answer, and comment on all the questions at once.
  A revised review stays where it was first sent and names its round,
  and a line marks where each later round was sent.
- A session's thread shows a line when the agent withdraws a review, with its
  reason. The line leads to the review.
- Links in the statuses an agent reports open in your browser, and links
  to this computer in a session's thread open too, such as a dev server.
- A Sessions page lists the agent sessions that asked you questions or
  sent reviews, those waiting for you first.
- The sidebar lists every session that is not archived under its
  repository, with what waits under the session that sent it. A mark on the
  agent's icon shows a session that works or has finished, and a menu
  groups, orders and filters the sessions.
- Drag the sidebar's edge to resize it. Pinrail remembers the width.
- Hide the session's details beside its thread with the header's button
  or ⌘⇧B, and drag their edge to resize them.
- The markdown plugin can hide its comments with the button at the right
  of its header, so the document takes their width.
- Questions in the inbox are marked apart from reviews.
- A question's text can use inline Markdown, such as `code` and **bold**,
  which the thread shows formatted.
- `pinrail status` lets an agent say what it is doing. Its statuses show
  in the session's thread between its questions and reviews, and the
  newest shows on the Sessions page.
  `pinrail status --start` names the stage of work the agent starts,
  which becomes the session's title. The session details list its latest
  8 stages and fold the earlier ones.
  `pinrail status --note` sends a note: a message in Markdown, with files
  such as a chart or a log, that shows in the thread and asks nothing.
- Code in a note or a question's context is highlighted when it names its
  language.
- The session details show the project, the branch and the folder the
  agent works in. The CLI now sends the folder with every review.
- The Sessions page opens a session's thread in the side panel, as the
  inbox opens a review.
- The Sessions page filters the sessions by project.
- A review sent from a session links to the session's thread from its
  header. On the session's page, a review opens in a side panel beside
  the thread. A review opened as a page from the session names the
  session in its breadcrumb, and deciding it returns to the thread.
- Archive a session from its header, its row on the Sessions page or
  with E. Show archived lists archived sessions with the others, and new
  activity from the agent brings a session back.
- The command palette finds the stages agents started, and opens the
  session's thread where a stage started.
- Star a review to find it again later, from its header, with ⌘⇧S, or
  from its row in the inbox. The inbox's star filter lists the starred
  reviews.

### Changed

- The history moved to a History tab in the inbox, with the same rows and
  filters as the other tabs. ⌘⇧H opens the tab.
- The `pinrail` skill tells agents to ask their questions through Pinrail
  with `pinrail ask`, not in the chat, and to report their progress with
  `pinrail status`.

### Fixed

- Pressing ⌘B twice in quick succession hides the sidebar and shows it
  again, and the next start keeps it shown.
- A change made as the app starts, such as ⌘B, no longer resets the other
  settings, which could open the setup again.

## [0.4.1]

### Added

- The markdown plugin has a Changes view for a new version of a document.
  It shows what changed since the previous round, with the changed words
  marked and the unchanged lines folded. Your previous comments appear
  at their place, marked by whether their lines changed, so you can
  check how they were addressed without reading the document again.

### Changed

- The official plugins share one comment field: a box with the text,
  the microphone and the add button at its end. Once the text wraps,
  the buttons move to a row under it, with the keys that add the
  comment and start a new line.
- A saved comment in the official plugins has edit and delete buttons
  at the right of its header, shown while the pointer is over it or the
  focus is in it.
- Comment fields in plugin views that use the SDK's comment field have
  spell checking, autocorrection and automatic capitals off.
- In the markdown plugin, the comments panel shows this round's and the
  previous round's comments in two tabs.

### Fixed

- Going back from the side panel now closes it after two reviews were
  clicked in quick succession, in the inbox and in the history.
- Fix plugin views that compare with the previous round, such as the
  image plugin's comparison, in the app. They now receive the previous
  round with its payload.

### Security

- Updated tokio, whose `AsyncFd` constructors accepted a file descriptor
  the caller might not own.

## [0.4.0]

### Added

- A plugin can ship a skill of its own in a `skill/` folder, which
  Pinrail writes into each connected agent's `pinrail` skill under
  `plugins/<name>/`, and the `pinrail` skill names the plugins that have
  one. Installing, updating or removing a plugin rewrites the connected
  agents' skills. Every plugin's skill is off until you turn it on in the
  plugin's details in Settings › Plugins, where you can also open the
  skill's folder to read it, or with `pinrail plugins install
--allow-skill`. A plugin's row shows whether its skill is on.
- `pinrail plugins skill <name>` prints a plugin's skill, or a file it
  names, for an agent without the `pinrail` skill installed, and
  `pinrail plugins` says which plugins have one.
- The change map plugin, `change-map`, among the official plugins. It
  shows a code change, such as a pull request, as a diagram of the parts
  it touches and how they interact, with each part's diff, screenshots
  before and after or recordings of the interface, and optional sequence
  and state diagrams. Comments go
  on any of them. Its skill tells your agents how to build a map from a
  change. Install it in Settings › Plugins.
- The history opens a review in a panel beside the list, as the inbox
  does. J and K step through the history from it. _Settings › General ›
  Opening a review_ applies to both.
- _Settings › Agents › Update skills automatically_ updates the skill of
  each connected agent when Pinrail starts, after an update of Pinrail
  changed it. It is off by default.
- Support for Hermes Agent. Settings › Agents installs the `pinrail`
  skill to `~/.hermes/skills`, and reviews submitted from Hermes Agent
  show its name and icon.
- Plugins can copy to the clipboard. A plugin that declares the
  `clipboard` capability can copy the text or images of a review, and the
  app shows a notice for each copy.

### Fixed

- The inbox beside the side panel fits its width. A narrow list hides
  each review's plugin and the sort order, and a review's details no
  longer run into its plugin's name.
- In the inbox, the outline of the review under the pointer no longer
  runs over the review open beside it.
- Discarding the review open in the side panel opens the next one, as a
  decision does, whether from the review or from its row in the inbox.
- The keyboard shortcuts dialog lists the keys of the plugin open in the
  side panel, as it does for a review open on its own page.
- Dictation fills text fields in plugin views built with React, so a
  button that waits for text, such as Add comment, is enabled as you
  dictate.

### Security

- Updated rustls-webpki and webpki-roots, which could trust certificates
  from the distrusted emSign authorities, and markdown-it, which could
  take quadratic time to parse some Markdown.

## [0.3.0]

### Added

- Support for Amp, Augment CLI (Auggie), Cline, GitHub Copilot CLI,
  Goose, Kiro CLI and Pi. Settings › Agents installs the `pinrail` skill
  in each agent's skill folder, and reviews submitted from these agents
  show the agent's name and icon.
- What's new: the first time a new release starts, Pinrail shows a few
  cards about what changed, one at a time, each with a screenshot and a
  button that opens the change. A release without notes shows nothing,
  and a fresh install starts counting from the release it installed. The
  newest notes open again from Settings › About, Help › What's New in
  Pinrail and ⌘K.
- Pinrail records whether a decision or a discard was made in the app,
  with `pinrail decide` or `pinrail discard`, or by another program
  through the API, and in which agent session. The review page, the
  history, `pinrail show` and the decision an agent receives say so when
  it was not made in the app. The review page warns when the agent
  session that submitted a review also decided it.
- The history shows who withdrew or discarded a review and how, with the
  reason they gave in a tooltip. Pinrail records the agent that withdrew
  a review, as it does for a discard.
- In the macOS app, the note to the agent can be dictated: the
  microphone in the note box turns speech into text as you speak, in
  your system's language. macOS asks once for access to the microphone
  and to speech recognition.

### Changed

- Settings › Agents groups the agents into connected, not connected and
  not found, and sorts each group by name. The agents not found are
  folded away. The setup wizard also lists the agents it found by name.
- Pinrail installs its skill to OpenCode's and Grok CLI's skill folders,
  `~/.config/opencode/skills` and `~/.grok/skills`, even when Claude Code
  or Cursor already has it.
- An app update now updates the installed official plugins to the
  versions it carries, when Pinrail starts. Until now, Settings › Plugins
  offered each update and waited for you to apply it. Reviews keep the
  version they were submitted to.
- A plugin can no longer declare J or K as shortcuts. They are the
  app's keys for moving through the inbox, and `pinrail plugins check`
  refuses a manifest that declares them. The official plugins move
  between items with N and P instead.
- The inbox opens a review in a panel beside the list. The panel opens
  the review as a page when the view needs more room, closes with its
  button or Esc, and moves on to the next review after a decision. J
  and K step through the list from it, while the view has the keyboard
  too. _Settings › General › Opening a review_ switches back to opening
  reviews as a page.
- Esc closes the inbox's panel from inside the review's view too. When
  the view has something of its own open, such as a comment, the first
  Esc closes that and the next closes the panel. A plugin that uses Esc
  no longer stops it from closing the panel.
- J and K go round at the ends of the inbox: J on the last review goes
  to the first, and K on the first to the last, in the list and in the
  panel.
- The official plugins fit a narrow frame, such as the inbox's side
  panel. Their sidebars, such as the code review's file tree and the
  Markdown review's outline and comments, open as drawers over the
  review, and their headers and controls rearrange to leave the content
  the width. The image review's notes fold away, and the code review has
  arrows for the next and previous proposal.
- Only a decision or a discard made in the app is recorded as yours. One
  made with `pinrail decide` or `pinrail discard` is recorded as made by
  the agent the command runs under, if any, and one made through the API
  by the name the program gives, if any.

### Fixed

- On Linux, an update of the AppImage reliably updates the copy of the
  `pinrail` command it installed. The check that the copy is the
  `pinrail` command could fail when the file had just been written.

## [0.2.0]

### Added

- ⌘F (Ctrl+F on Linux) opens a find bar on a review that searches the
  text of the plugin's view and highlights every match. Enter and
  Shift+Enter move between the matches, and Esc closes the bar. It works
  in every view that uses the plugin SDK, with no change to the plugin.
- The code review view highlights the syntax of the diff, of suggested
  changes and of code blocks, in the language of each file. If you
  installed code review, update it to 1.1.0 in _Settings › Plugins_.

### Changed

- ⌘D (Ctrl+D on Linux) discards a review, from its row in the inbox or
  from the review itself, including while the plugin's view has the
  keyboard. It replaces D, which worked only in the inbox. A plugin that
  declares ⌘D as one of its shortcuts no longer receives it.

### Fixed

- A review created while the app was loading the inbox no longer
  disappears from the inbox until the app is reloaded.

## [0.1.3]

### Fixed

- A review's notification closes when you open the review in the window
  or it stops waiting. On Linux, notifications stayed in GNOME's list, and
  Ubuntu Dock kept counting them on the app's icon.
- On Linux, the menu bar icon is white, so it can be seen on GNOME's dark
  top bar.

## [0.1.2]

### Changed

- `pinrail plugins new --playwright` takes the plugin SDK from npm, as
  `pinrail-sdk`, rather than from the tarball on the SDK's GitHub release.

### Fixed

- On macOS 26, the button that shows and hides the sidebar no longer
  touches the window's close, minimize and zoom buttons.
- The button that shows and hides the sidebar stays at the top when the
  sidebar scrolls.

## [0.1.1]

### Changed

- A review's _Discard_, _Copy decision as markdown_ and _Maximize_ buttons
  are at the right of the review's header, beside its status, rather than
  in the window's top bar.

### Fixed

- In a window too narrow for the sidebar, the page now takes the whole
  width, and the window's buttons no longer cover the top bar on macOS.

## [0.1.0]

Initial version of Pinrail.

[0.6.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.6.0
[0.5.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.5.0
[0.4.1]: https://github.com/forgeplane/pinrail/releases/tag/v0.4.1
[0.4.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.4.0
[0.3.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.3.0
[0.2.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.2.0
[0.1.3]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.3
[0.1.2]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.2
[0.1.1]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.1
[0.1.0]: https://github.com/forgeplane/pinrail/releases/tag/v0.1.0
