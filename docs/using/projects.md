---
title: Projects
description: "Group repositories into projects, so that the inbox and the sidebar show the work of several repositories together."
---

A project is a set of repositories that you group under one name, such as an app, its infrastructure and its plugins. Grouped by project, the inbox and the sidebar show the reviews and the sessions of all its repositories together. A repository belongs to at most one project.

Agents know nothing of projects. A review or a session names its repository, and Pinrail shows it under the project that repository is in. Adding a repository to a project also moves its earlier reviews and sessions there.

![The Projects page: two projects with their repositories, what waits in each and when it was updated, and the repositories in no project below them.](screenshot:projects "Each project lists its repositories.")

## The Projects page

Open **Projects** in the sidebar. The page lists the projects by name. Each project shows what waits in it, its sessions and its latest activity, with its repositories under it. **In no project** lists every other repository that sent a review or a session.

- **Create a project** with **New project**. Give it a name and, if you like, a description, a colour and an icon: an emoji, or up to three letters. Without an icon, the project's avatar shows the first letter of its name.
- **Add a repository** by dragging it under a project. Drag it under **In no project** to take it out again. The **…** button at the end of a repository's row also moves it, or starts a new project with it.
- **Change a project** with **Edit…** in the **…** menu of its heading. **Delete project…** removes the project and nothing else: its repositories are in no project again, and their reviews and sessions stay as they are.
- **Fold a project** with the arrow before its name, and **filter** the page by a project's or a repository's name.

From a project's **…** menu, **Open in the inbox** shows the reviews of all its repositories, and **Open its sessions** shows their sessions. A repository's **…** menu opens that repository in the inbox, when something waits in it, in the Sessions page or in the history.

## Grouping by project

- **Inbox:** the buttons at the right of the tabs group the waiting reviews by project, by repository, or show them in one list. Grouped by project, a repository in no project is a group of its own, and in a project with several repositories each review names its repository.
- **Sidebar:** the button at the right of the sessions heading opens its menu. Under **Grouping**, choose **Project**.
- **Sessions page:** the buttons at the right of its header group the sessions by project, by repository, or show them in one list.
- **Filters:** the **Project or repository** menu in the inbox, the history and the Sessions page lists the projects first. Choosing a project shows the reviews or sessions of all its repositories. Type in the menu to find a project or a repository by name.

The command palette, <kbd>⌘K</kbd>, has **Go to projects**, **New project…**, and **Show** followed by a project's name, which opens the inbox filtered to that project.
