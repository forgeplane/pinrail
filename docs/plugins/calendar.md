---
title: Calendar
description: "Times to arrange around what is already booked: one suggested slot picked per item, another time asked for, or the item declined."
sidebar:
  badge:
    text: Optional
    variant: default
---

<div class="pr-badges"><span class="pr-badge optional">Optional</span><span class="pr-badge plain">plugin: calendar</span></div>

The calendar plugin is for deciding when things happen. The agent has found candidate times for a few appointments, meetings or interviews; you see them laid over what is already in your calendar, and pick one time for each. Picking a time hides every other suggestion it would clash with, and clearing it brings them back. Anything that does not fit can go back to the agent for another time, or be declined.

![The calendar plugin in the week view: a doctor appointment chosen on Monday, the other suggestions around existing meetings, and dinner sent back for another time with a note.](screenshot:calendar-week "The week: a time chosen for the doctor, and dinner sent back for another time with a note.")
![The same plan in the day view, Tuesday at full width, with the week's days above it and how many suggestions each has.](screenshot:calendar-day "One day at full width, with the week above it and how many suggestions each day has.")
![The same plan as a list: every suggested time by item, the chosen one checked.](screenshot:calendar-list "Every suggested time, by item, with the chosen one checked.")

## When to use it

- **A personal assistant's week**: a doctor's appointment, a game of tennis and a dinner, fitted around work.
- **Interviews**: several candidates who share one interview panel, so choosing one slot rules out the others at that time.
- **Anything booked on someone's behalf**, where the time is theirs to choose.

## Install

```sh
pinrail plugins install github.com/forgeplane/pinrail/plugins/calendar
```

## What you see

- **The plan**: each item to arrange, how many times are still possible, and what you chose.
- **Your calendar and the suggestions together.** What is already booked is fixed and striped; each item's suggestions are in its own colour, the agent's best fit marked.
- **Three views.** *Day* shows one day at full width, with the week's days above it and how many suggestions each has. *Week* shows up to seven days and pages through a longer range. *List* shows every suggested time by item.
- **Conflicts that step aside.** Choosing a time hides the suggestions that would clash with it, and clearing it brings them back.
- **Another time, or decline.** An item none of whose times work goes back to the agent for another, with a note on when would suit. An item you do not want at all is declined, with a reason if you like.

Keys: <kbd>d</kbd>, <kbd>w</kbd> and <kbd>l</kbd> switch between the day, week and list views; <kbd>j</kbd> and <kbd>k</kbd> move to the next and previous day or week; <kbd>t</kbd> goes back to the first day. *Settings › Plugins › Calendar* chooses the view a calendar opens in.

To see it before any agent asks with it, send its sample: `pinrail submit calendar --sample`, or **Send a sample** on its row in *Settings › Plugins*.

## Asking from your agent

```md title="AGENTS.md"
## Before booking anything

When you have found times for appointments or meetings, do not book them.
Submit them to Pinrail as a `calendar` review and wait:

1. Write the payload: the timezone, the first day and how many days to show,
   what is already booked, and each item with its candidate times.
2. Run: `pinrail submit calendar --title "<what these are>" --data slots.json --wait --format markdown`
3. Book the `selections`, checking each is still free first.
4. For `deferred` items, find other times, using the note, and submit a new
   round with `--revises <id>`. Do not book anything for `declined` items.
5. If the command exits 5, stop.
```

## What the agent sends

```json title="slots.json"
{
  "timezone": "Europe/Athens",
  "start_date": "2026-09-21",
  "days": 5,
  "blocked": [
    { "id": "standup", "title": "Team stand-up", "start": "2026-09-21T09:00:00+03:00", "end": "2026-09-21T10:00:00+03:00" }
  ],
  "items": [
    {
      "id": "doctor",
      "title": "Doctor appointment",
      "subtitle": "Annual check-up · 1 hour",
      "icon": "stethoscope",
      "options": [
        { "id": "doctor-mon", "start": "2026-09-21T10:00:00+03:00", "end": "2026-09-21T11:00:00+03:00", "location": "Kolonaki", "detail": "€60", "recommended": true },
        { "id": "doctor-thu", "start": "2026-09-24T11:00:00+03:00", "end": "2026-09-24T12:00:00+03:00", "location": "Kolonaki", "detail": "€60" }
      ]
    }
  ]
}
```

- **Times carry their offset**, and everything is shown in the payload's `timezone`, whatever the reader's own.
- **`days`** is 1 to 14. A single day opens in the day view; anything longer opens in the week.
- **Conflicts are exact**: overlapping intervals clash, touching ones do not. Travel time belongs in the suggested intervals or in `blocked`.

## What comes back

```json
{
  "verdict": "revise",
  "timezone": "Europe/Athens",
  "selections": [
    { "item_id": "doctor", "option_id": "doctor-mon", "start": "2026-09-21T10:00:00+03:00", "end": "2026-09-21T11:00:00+03:00" }
  ],
  "deferred": ["dinner"],
  "declined": ["tennis"],
  "notes": [
    { "item_id": "dinner", "note": "Friday evening works better" },
    { "item_id": "tennis", "note": "Not this week" }
  ]
}
```

| Field | Meaning |
|---|---|
| `verdict` | `approve` when every item has a time or was declined; `revise` when at least one needs another time. |
| `selections` | The chosen times, copied from the payload's options. Book these. |
| `deferred` | Items that need another time. Nothing is booked for them yet. |
| `declined` | Items not to schedule at all. |
| `notes` | What the person said about an item sent back or declined. |

With `--format markdown`, the agent reads the chosen times by item with the day, hours and place, then what needs another time and what was declined, each with its note.

## Reference

The plugin's manifest, and the schemas a payload and a decision are checked against, read from the plugin's own files.

![The Calendar plugin's contract](contract:calendar)
