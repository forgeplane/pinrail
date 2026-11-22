---
title: Notifications
description: "When Pinrail tells you a review is waiting, and how to pause, quiet or narrow it."
---

While an agent waits for your decision, Pinrail tells you that a review has arrived. It shows a system notification and updates the count in the menu bar. Click the notification to open the review.

## The menu bar

The menu bar icon shows how many reviews are waiting, and its menu lists them, oldest first. From it you can:

- open a waiting review, the oldest one, the inbox, or the Work items page;
- pause notifications for 15 minutes, an hour, or until tomorrow.

Closing Pinrail's window keeps it in the menu bar, so reviews keep arriving. You can hide the icon in *Settings › General*, and Pinrail still runs.

On Linux, the icon appears in the system tray. Some desktops, such as GNOME, show tray icons only with an extension, for example *AppIndicator and KStatusNotifierItem Support*. Without one, set *Closing the window* to *Quit*, or keep the window open.

## Quieting notifications

| To | Do this |
|---|---|
| Stop them for a while | **Pause**, from the menu bar or *Settings › Notifications*. Reviews still arrive and are counted. |
| Stop them for one kind | Turn off its switch in the *System* column of *Settings › Notifications*. |
| Stop them for one plugin | Turn off **Notify** on the plugin's row in *Settings › Plugins*. Its reviews are still counted. |
| Stop them for a project or repository | **Mute** it in *Settings › Notifications*. Only what waits for you reaches the inbox, and nothing of it is announced. |
| Keep them silent | Turn off **Sound** in *Settings › Notifications*. |
| Stop them entirely | Turn off **System notifications** in *Settings › Notifications*. |
| Stop them at night | Turn on **Quiet hours** in *Settings › Notifications* and set the times. The times are local, and the range may span midnight. |

A review that arrives during quiet hours is counted but never announced, not even when the quiet hours end. The [Settings reference](/docs/reference/settings/) describes every key in `settings.json`.

The menu bar count always shows what is waiting, whatever you have paused or muted.

## When notifications don't appear

Pinrail asks your system for permission to show notifications when you turn them on, in the setup or in *Settings › Notifications*. If notifications are already on and your system has not been asked yet, Pinrail asks when the first review arrives.

**On macOS**, open *System Settings › Notifications › Pinrail* and check that notifications are allowed, with the alert style you want. A Focus mode also holds notifications back until it ends. *Settings › Notifications* in Pinrail shows what macOS reports, with a link to its settings when notifications are blocked.

**On Linux**, notifications go through your desktop's notification service. Check its do-not-disturb setting.
