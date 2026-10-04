/* global CalendarCore -- calendar-core.js, loaded before this file */
(function () {
  "use strict";
  const C = CalendarCore,
    esc = Pinrail.escape,
    icon = (name) => Pinrail.icon(name, { size: 17 });
  const app = document.getElementById("app");
  const colors = ["violet", "blue", "amber", "rose", "teal", "indigo"];
  /* The view: `day` one day at full width, `week` up to seven days, `list`
     every option by item. `cursor` is the first day shown, as an index into
     the payload's days; a week pages seven days at a time from the start. */
  const VIEWS = ["day", "week", "list"];
  let payload,
    state,
    error = "",
    mode = "week",
    cursor = 0,
    preferred = "auto",
    expanded = null,
    previous = null;
  const plugin = Pinrail.connect({
    onInit({ review, draft, previous: old, settings }) {
      error = "";
      previous = old;
      expanded = null;
      if (typeof settings?.view === "string") preferred = settings.view;
      try {
        payload = C.validate(review.payload);
        state = C.restore(payload, review.decision?.data || draft);
        // a draft keeps its view; "calendar" is what drafts called the week
        const saved = draft?.mode === "calendar" ? "week" : draft?.mode;
        mode = VIEWS.includes(saved)
          ? saved
          : VIEWS.includes(preferred)
            ? preferred
            : payload.days === 1
              ? "day"
              : "week";
        cursor = mode === "day" ? firstBusyDay() : 0;
      } catch (e) {
        payload = null;
        error = e.message;
      }
      render();
    },
    onSettings(settings) {
      if (typeof settings?.view === "string") preferred = settings.view;
    },
    onSubmitted() {
      error = "";
      state = C.restore(payload, plugin.review.decision?.data);
      render();
    },
    // the app's hand-over: the decision, or nothing while it is incomplete
    onCollect() {
      if (!payload || plugin.readonly) return;
      try {
        return C.decision(payload, state);
      } catch (e) {
        error = e.message;
        render();
        document.getElementById("feedback")?.focus();
      }
    },
    onViolations(errors) {
      error = errors.map((e) => `${e.path || "Decision"}: ${e.message}`).join(" ");
      render();
    },
  });
  const fmt = (value, options) =>
    new Intl.DateTimeFormat("en-GB", { timeZone: payload.timezone, ...options }).format(new Date(value));
  const time = (value) => fmt(value, { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
  const date = (value) => fmt(value, { weekday: "short", day: "numeric", month: "short" });
  const range = (o) => `${time(o.start)}–${time(o.end)}`;
  const fullTime = (o) =>
    `${date(o.start)}, ${time(o.start)} – ${C.dateKey(o.start, payload.timezone) !== C.dateKey(o.end, payload.timezone) ? date(o.end) + ", " : ""}${time(o.end)}`;
  const chosen = (item) => item.options.find((o) => o.id === state.selected[item.id]);
  const color = (item) => colors[payload.items.indexOf(item) % colors.length];
  const deferred = (item) => state.deferred.includes(item.id);
  const declined = (item) => state.declined.includes(item.id);
  // an item handed back without a time: another time asked for, or declined
  const settled = (item) => deferred(item) || declined(item);
  const NOTE_HINT = {
    deferred: "When would suit? e.g. any evening after 18:00, or not this week",
    declined: "Why not? (optional)",
  };
  const label = (item, o) => `${item.title}, ${fullTime(o)}${o.location ? ", " + o.location : ""}`;
  const dayCount = () => payload.days;
  const dayAt = (i) => C.addDays(payload.start_date, i);
  /* the days on screen: one, a page of seven, or all of them in the list */
  function visibleDays() {
    if (mode === "day") return [dayAt(cursor)];
    const from = Math.floor(cursor / 7) * 7;
    return Array.from({ length: Math.min(7, dayCount() - from) }, (_, i) => dayAt(from + i));
  }
  // the first day with something to choose, so a day view opens where the work is
  function firstBusyDay() {
    for (let i = 0; i < dayCount(); i++) {
      const day = dayAt(i);
      if (payload.items.some((item) => item.options.some((o) => C.segments(o, day, payload.timezone)))) return i;
    }
    return 0;
  }
  const suggestionsOn = (day) =>
    payload.items.reduce(
      (n, item) => n + C.available(payload, state, item).filter((o) => C.segments(o, day, payload.timezone)).length,
      0,
    );
  function setView(next) {
    if (!VIEWS.includes(next) || next === mode) return;
    // the week keeps the day in view, and the day opens on the week's first
    if (next === "day" && mode === "week") cursor = Math.floor(cursor / 7) * 7;
    mode = next;
    if (!plugin.readonly) plugin.draft({ ...state, mode });
    plugin.setSetting?.("view", mode);
    render();
  }
  function step(by) {
    if (mode === "list") return;
    const size = mode === "day" ? 1 : 7;
    const next = mode === "day" ? cursor + by : Math.floor(cursor / 7) * 7 + by * size;
    if (next < 0 || next >= dayCount()) return;
    cursor = next;
    render();
  }
  function goFirst() {
    cursor = 0;
    render();
  }
  function save(message) {
    error = "";
    plugin.draft({ ...state, mode });
    render();
    document.getElementById("announcement").textContent = message;
  }
  function optionButton(item, option, compact = false) {
    const selected = state.selected[item.id] === option.id;
    return `<button class="option ${selected ? "is-selected" : ""}" data-action="choose" data-item="${esc(item.id)}" data-option="${esc(option.id)}" aria-label="${esc(label(item, option))}" aria-pressed="${selected}" ${plugin.readonly ? "disabled" : ""}>
      <span class="option-check">${selected ? icon("check") : ""}</span><span class="option-text"><strong>${esc(date(option.start))}</strong><span>${esc(range(option))}${!compact && option.location ? ` · ${esc(option.location)}` : ""}</span>${option.detail && !compact ? `<small>${esc(option.detail)}</small>` : ""}</span>${option.recommended ? '<span class="recommended">Best fit</span>' : ""}</button>`;
  }
  function card(item) {
    const selected = chosen(item),
      left = C.available(payload, state, item),
      isDeferred = deferred(item),
      isDeclined = declined(item);
    const hidden = item.options.length - left.length;
    const open = expanded === item.id;
    const how = isDeferred ? "deferred" : isDeclined ? "declined" : null;
    const note = state.notes[item.id] || "";
    return `<article class="activity ${color(item)} ${selected ? "has-selection" : ""} ${how ? "is-" + how : ""}" data-activity="${esc(item.id)}">
      <button class="activity-heading" data-action="expand" data-item="${esc(item.id)}" aria-expanded="${open}" aria-controls="options-${esc(item.id)}">
        <span class="activity-icon">${icon(item.icon || "calendar")}</span><span><strong>${esc(item.title)}</strong><span class="activity-sub">${selected ? esc(fullTime(selected)) : isDeferred ? "Another time asked for" : isDeclined ? "Declined" : `${left.length} available ${left.length === 1 ? "option" : "options"}`}</span></span><span class="activity-status">${selected ? icon("check") : isDeferred ? icon("calendar-sync") : isDeclined ? icon("calendar-x") : icon(open ? "chevron-up" : "chevron-down")}</span>
      </button>
      ${item.subtitle ? `<p class="activity-description">${esc(item.subtitle)}</p>` : ""}
      ${selected?.location ? `<div class="chosen-location">${icon("map-pin")} ${esc(selected.location)}</div>` : ""}
      ${selected?.detail ? `<p class="selected-detail">${esc(selected.detail)}</p>` : ""}
      ${hidden ? `<p class="hidden-count">${icon("circle-minus")} ${hidden} conflicting ${hidden === 1 ? "time hidden" : "times hidden"}</p>` : ""}
      ${!left.length && !how ? '<p class="no-options">No compatible times. Clear another selection, ask for another time, or decline it.</p>' : ""}
      ${
        how
          ? `<div class="handback">${
              plugin.readonly
                ? note
                  ? `<p class="handback-note">${esc(note)}</p>`
                  : ""
                : `<label class="sr-only" for="note-${esc(item.id)}">${how === "deferred" ? "When would suit" : "Why not"}</label><textarea id="note-${esc(item.id)}" data-note="${esc(item.id)}" rows="2" placeholder="${esc(NOTE_HINT[how])}">${esc(note)}</textarea>`
            }</div>`
          : ""
      }
      <div id="options-${esc(item.id)}" class="card-options" ${open ? "" : "hidden"}>${left.map((o) => optionButton(item, o, true)).join("")}</div>
      ${
        plugin.readonly
          ? ""
          : `<div class="card-actions">${
              how
                ? `<button data-action="undo-settle" data-item="${esc(item.id)}">${icon("undo-2")} Choose a time instead</button>`
                : `${selected ? `<button data-action="clear" data-item="${esc(item.id)}">${icon("undo-2")} Clear choice</button>` : `<button data-action="defer" data-item="${esc(item.id)}" title="None of these times work: ask the agent to find another">${icon("calendar-sync")} Another time</button>`}<button data-action="decline" data-item="${esc(item.id)}" class="decline" title="Do not schedule this at all">${icon("calendar-x")} Decline</button>${selected ? `<button data-action="expand" data-item="${esc(item.id)}">${open ? "Hide" : "Change"}</button>` : ""}`
            }</div>`
      }
    </article>`;
  }
  function calendar() {
    const days = visibleDays();
    const raw = payload.blocked.map((b) => ({ ...b, blocked: true }));
    payload.items.forEach((item) => {
      const options =
        plugin.readonly || settled(item) ? (chosen(item) ? [chosen(item)] : []) : C.available(payload, state, item);
      options.forEach((option) =>
        raw.push({ ...option, item, title: item.title, selected: option.id === state.selected[item.id] }),
      );
    });
    // Keep the same hours on every day and page, so the grid does not jump
    // while choosing or moving between days.
    const all = [...payload.blocked, ...payload.items.flatMap((i) => i.options)];
    const everyDay = Array.from({ length: dayCount() }, (_, i) => dayAt(i));
    const allSegments = everyDay.flatMap((day) => all.map((o) => C.segments(o, day, payload.timezone)).filter(Boolean));
    const startHour = Math.min(8, ...allSegments.map((s) => Math.floor(s.from / 60)));
    const endHour = Math.min(24, Math.max(20, ...allSegments.map((s) => Math.ceil(s.to / 60))));
    const hourHeight = 58,
      height = (endHour - startHour) * hourHeight;
    const hours = Array.from({ length: endHour - startHour }, (_, i) => startHour + i);
    return `<div class="calendar-scroll ${mode === "day" ? "is-day" : ""}" id="calendar-scroll" tabindex="0" aria-label="Calendar. Scroll to see all times and days."><div class="calendar-grid" style="--days:${days.length};--grid-height:${height}px;--hour-height:${hourHeight}px">
      <div class="day-head time-head">${icon("clock-3")}</div>${days.map((day) => `<div class="day-head ${mode === "day" ? "is-single" : ""}"><span>${esc(new Intl.DateTimeFormat("en-GB", { weekday: mode === "day" ? "long" : "short", timeZone: "UTC" }).format(new Date(day + "T12:00Z")))}</span><strong>${Number(day.slice(-2))}</strong>${mode === "day" ? `<em>${esc(new Intl.DateTimeFormat("en-GB", { month: "long", timeZone: "UTC" }).format(new Date(day + "T12:00Z")))}</em>` : ""}</div>`).join("")}
      <div class="time-rail">${hours.map((h) => `<span style="top:${(h - startHour) * hourHeight}px">${String(h).padStart(2, "0")}:00</span>`).join("")}</div>
      ${days
        .map((day) => {
          const events = C.lanes(raw.map((o) => C.segments(o, day, payload.timezone)).filter(Boolean));
          return `<section class="day-column" aria-label="${esc(day)}">${events
            .map((e) => {
              const style = `top:${(e.from / 60 - startHour) * hourHeight + 2}px;height:${Math.max(22, ((e.to - e.from) / 60) * hourHeight - 4)}px;left:calc(${(e.lane / e.lanes) * 100}% + 3px);width:calc(${100 / e.lanes}% - 6px)`;
              if (e.blocked)
                return `<div class="event blocked" style="${style}" title="${esc(`${e.title} · ${fullTime(e)}`)}"><strong>${icon("lock-keyhole")} ${esc(e.title)}</strong><span>${esc(range(e))}</span></div>`;
              return `<button class="event suggestion ${color(e.item)} ${e.selected ? "selected" : ""} ${chosen(e.item) && !e.selected ? "alternative" : ""}" style="${style}" data-action="choose" data-item="${esc(e.item.id)}" data-option="${esc(e.id)}" aria-label="${esc(label(e.item, e))}" aria-pressed="${e.selected}" title="${esc(`${label(e.item, e)}${e.detail ? " · " + e.detail : ""}`)}" ${plugin.readonly ? "disabled" : ""}>
            <strong>${e.selected ? icon("check") : icon(e.item.icon || "calendar")} ${esc(e.title)}</strong><span>${esc(range(e))}</span>${e.location ? `<small>${esc(e.location)}</small>` : ""}${e.recommended && !e.selected ? '<i class="best-dot" aria-label="Recommended"></i>' : ""}
          </button>`;
            })
            .join("")}</section>`;
        })
        .join("")}</div></div>`;
  }
  function list() {
    return `<div class="list-scroll">${payload.items
      .map((item) => {
        const options = plugin.readonly ? (chosen(item) ? [chosen(item)] : []) : C.available(payload, state, item);
        return `<section class="list-group ${color(item)}"><div class="list-heading"><span class="activity-icon">${icon(item.icon || "calendar")}</span><div><h3>${esc(item.title)}</h3><p>${esc(item.subtitle || "Choose one of the available times")}</p></div></div>${deferred(item) ? '<p class="pinrail-dim">Another time asked for.</p>' : declined(item) ? '<p class="pinrail-dim">Declined: not to be scheduled.</p>' : options.length ? options.map((o) => optionButton(item, o)).join("") : '<p class="pinrail-dim">No compatible options remain.</p>'}</section>`;
      })
      .join(
        "",
      )}<section class="blocked-list"><h3>${icon("lock-keyhole")} Already in your calendar</h3>${payload.blocked.length ? payload.blocked.map((b) => `<p><strong>${esc(b.title)}</strong><span>${esc(fullTime(b))}</span></p>`).join("") : '<p class="pinrail-dim">No blocked time in this window.</p>'}</section></div>`;
  }
  function render() {
    const scroller = document.getElementById("calendar-scroll"),
      oldScroll = scroller ? [scroller.scrollLeft, scroller.scrollTop] : null;
    const side = document.querySelector(".sidebar"),
      sideScroll = side?.scrollTop || 0;
    const active = document.activeElement?.dataset;
    const focus = active?.action ? { ...active } : null;
    if (!payload) {
      app.innerHTML = `<div class="fatal" role="alert">${icon("calendar-x")}<h1>This calendar needs another look</h1><p>${esc(error)}</p></div>`;
      plugin.handOverLabel("Calendar data needs correction");
      return;
    }
    const selected = C.selectedSlots(payload, state),
      total = payload.items.length;
    const unresolved = total - selected.length - state.deferred.length - state.declined.length;
    const handedBack = [
      state.deferred.length ? `${state.deferred.length} for another time` : "",
      state.declined.length ? `${state.declined.length} declined` : "",
    ]
      .filter(Boolean)
      .join(" · ");
    const hidden = payload.items.reduce((n, i) => n + i.options.length - C.available(payload, state, i).length, 0);
    const duration = Math.round(
      selected.reduce((sum, s) => sum + (Date.parse(s.end) - Date.parse(s.start)) / 60000, 0),
    );
    const shown = mode === "list" ? [payload.start_date, dayAt(dayCount() - 1)] : visibleDays();
    const firstShown = shown[0],
      lastShown = shown[shown.length - 1];
    const displayMonth = new Intl.DateTimeFormat("en-GB", { month: "long", year: "numeric", timeZone: "UTC" }).format(
      new Date(firstShown + "T12:00Z"),
    );
    const short = (d) =>
      `${Number(d.slice(8))} ${new Intl.DateTimeFormat("en", { month: "short", timeZone: "UTC" }).format(new Date(d + "T12:00Z"))}`;
    const spanLabel =
      firstShown === lastShown
        ? new Intl.DateTimeFormat("en-GB", { weekday: "long", day: "numeric", month: "long", timeZone: "UTC" }).format(
            new Date(firstShown + "T12:00Z"),
          )
        : `${short(firstShown)} – ${short(lastShown)}`;
    const pages = mode === "day" ? dayCount() : Math.ceil(dayCount() / 7);
    const page = mode === "day" ? cursor : Math.floor(cursor / 7);
    const paging = mode !== "list" && pages > 1;
    const nav = paging
      ? `<div class="pager" role="group" aria-label="${mode === "day" ? "Days" : "Weeks"}">
        <button data-action="prev" aria-label="Previous ${mode}" title="Previous ${mode} (k)" ${page === 0 ? "disabled" : ""}>${Pinrail.icon("chevron-left", { size: 15 })}</button>
        <button data-action="first" class="pager-first" title="Back to the first day (t)" ${page === 0 ? "disabled" : ""}>First day</button>
        <button data-action="next" aria-label="Next ${mode}" title="Next ${mode} (j)" ${page === pages - 1 ? "disabled" : ""}>${Pinrail.icon("chevron-right", { size: 15 })}</button>
      </div>`
      : "";
    const weekFrom = Math.floor(cursor / 7) * 7,
      weekDays = Math.min(7, dayCount() - weekFrom);
    const strip =
      mode === "day" && dayCount() > 1
        ? `<div class="day-strip" role="group" aria-label="Days this week">${Array.from(
            { length: weekDays },
            (_, k) => {
              const i = weekFrom + k,
                day = dayAt(i),
                n = suggestionsOn(day);
              return `<button data-action="goto" data-day="${i}" class="${i === cursor ? "is-current" : ""}" aria-pressed="${i === cursor}" aria-label="${esc(new Intl.DateTimeFormat("en-GB", { weekday: "long", day: "numeric", month: "long", timeZone: "UTC" }).format(new Date(day + "T12:00Z")))}${n ? `, ${n} ${n === 1 ? "suggestion" : "suggestions"}` : ""}"><span>${esc(new Intl.DateTimeFormat("en-GB", { weekday: "short", timeZone: "UTC" }).format(new Date(day + "T12:00Z")))}</span><strong>${Number(day.slice(-2))}</strong>${n ? `<i>${n}</i>` : ""}</button>`;
            },
          ).join("")}</div>`
        : "";
    app.innerHTML = `<header class="pinrail-header calendar-header"><h1 class="pinrail-title">${esc(plugin.review.title || "Calendar")}</h1><span class="header-progress">${plugin.readonly ? `Read-only · ${esc(plugin.review.status || "closed")}` : `<b>${selected.length}</b> of ${total} selected${handedBack ? ` · ${handedBack}` : ""}`}</span></header>
      <div class="workbench"><aside class="sidebar" aria-label="Activities to arrange"><div class="sidebar-heading"><h2>The plan</h2><span>${total} ${total === 1 ? "item" : "items"}</span></div>
        ${payload.items.map(card).join("")}
        ${!total ? '<div class="pinrail-empty">Nothing to schedule in this review.</div>' : ""}
        <div class="selection-summary"><div><span>${selected.length}<small> / ${total}</small></span>${icon(unresolved === 0 ? "circle-check" : "mouse-pointer-2")}</div><strong>${unresolved ? "A little room for your judgment." : selected.length ? "Your plan is ready." : "Back to the agent."}</strong><p>${selected.length ? `${duration >= 60 ? Math.floor(duration / 60) + "h " : ""}${duration % 60 ? (duration % 60) + "m " : ""}planned. ` : ""}${handedBack ? `${handedBack}. ` : ""}${unresolved ? "Pick a time for each item, ask for another, or decline it." : "Use Pinrail’s hand-over to send your choices."}</p><div class="progress" aria-hidden="true"><span style="width:${total ? ((total - unresolved) / total) * 100 : 100}%"></span></div></div>
        ${selected.length && !plugin.readonly ? `<button class="reset" data-action="reset">${icon("rotate-ccw")} Clear all choices</button>` : ""}
        ${previous ? '<p class="revision-note">Revised proposal · choices apply to this round only.</p>' : ""}
      </aside><section class="schedule" aria-label="Suggested schedule"><div class="calendar-toolbar"><div><h2>${esc(displayMonth)}</h2><span>${esc(spanLabel)} <span class="timezone">· ${esc(payload.timezone)}</span></span></div><div class="toolbar-controls">${nav}<div class="view-switch" role="group" aria-label="Schedule view"><button data-action="view" data-view="day" aria-label="Day" title="Day (d)" aria-pressed="${mode === "day"}">${icon("calendar")}<span>Day</span></button><button data-action="view" data-view="week" aria-label="Week" title="Week (w)" aria-pressed="${mode === "week"}">${icon("calendar-days")}<span>Week</span></button><button data-action="view" data-view="list" aria-label="List" title="List (l)" aria-pressed="${mode === "list"}">${icon("list")}<span>List</span></button></div></div></div>
        ${strip}
        <div class="legend"><span><i class="legend-blocked"></i>Already busy</span><span><i class="legend-option"></i>Suggested</span><span><i class="legend-selected">✓</i>Your choice</span><span class="conflict-indicator">${hidden ? `${hidden} conflicting ${hidden === 1 ? "time" : "times"} hidden` : "Conflicts disappear as you choose"}</span></div>
        ${error ? `<div id="feedback" class="feedback" role="alert" tabindex="-1">${icon("circle-alert")}<span>${esc(error)}</span></div>` : ""}
        ${mode === "list" ? list() : calendar()}
        <footer class="calendar-footer">${icon("info")}<span>${plugin.readonly ? "The choices in this review are read-only." : "Click a suggestion to choose it. Click again to clear. Nothing is booked until you hand over."}</span></footer>
      </section></div>`;
    if (oldScroll && document.getElementById("calendar-scroll")) {
      const el = document.getElementById("calendar-scroll");
      el.scrollLeft = oldScroll[0];
      el.scrollTop = oldScroll[1];
    }
    document.querySelector(".sidebar").scrollTop = sideScroll;
    if (focus) {
      const target = [...app.querySelectorAll("button[data-action]")].find(
        (el) =>
          el.dataset.action === focus.action && el.dataset.item === focus.item && el.dataset.option === focus.option,
      );
      target?.focus({ preventScroll: true });
    }
    plugin.handOverLabel(
      plugin.readonly
        ? "Decision recorded"
        : unresolved
          ? `Choose ${unresolved} more ${unresolved === 1 ? "item" : "items"}`
          : handedBack
            ? `Hand over · ${selected.length} selected, ${handedBack}`
            : `Hand over ${selected.length} ${selected.length === 1 ? "selection" : "selections"}`,
    );
  }
  app.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-action]");
    if (!button || !payload) return;
    const { action, item: itemId, option: optionId } = button.dataset;
    if (action === "view") {
      setView(button.dataset.view);
      return;
    }
    if (action === "prev") {
      step(-1);
      return;
    }
    if (action === "next") {
      step(1);
      return;
    }
    if (action === "first") {
      goFirst();
      return;
    }
    if (action === "goto") {
      cursor = Number(button.dataset.day);
      render();
      return;
    }
    if (action === "expand") {
      expanded = expanded === itemId ? null : itemId;
      render();
      return;
    }
    if (plugin.readonly) return;
    const item = payload.items.find((i) => i.id === itemId);
    if (action === "choose" && C.choose(payload, state, itemId, optionId))
      save(
        `${item.title}: ${state.selected[itemId] ? "time selected" : "selection cleared"}. Conflicting suggestions updated.`,
      );
    else if (action === "clear") {
      delete state.selected[itemId];
      save(`${item.title} cleared. Available suggestions restored.`);
    } else if (action === "defer") {
      C.settle(state, itemId, "deferred");
      save(`${item.title}: ${deferred(item) ? "another time asked for" : "ready to choose"}.`);
      setTimeout(() => document.getElementById(`note-${itemId}`)?.focus(), 0);
    } else if (action === "decline") {
      C.settle(state, itemId, "declined");
      save(`${item.title}: ${declined(item) ? "declined, not to be scheduled" : "ready to choose"}.`);
      setTimeout(() => document.getElementById(`note-${itemId}`)?.focus(), 0);
    } else if (action === "undo-settle") {
      C.settle(state, itemId, deferred(item) ? "deferred" : "declined");
      save(`${item.title}: ready to choose.`);
    } else if (action === "reset") {
      state = C.restore(payload, null);
      save("All choices cleared. Available suggestions restored.");
    }
  });
  app.addEventListener("input", (event) => {
    const id = event.target.dataset?.note;
    if (!id || plugin.readonly) return;
    if (event.target.value.trim()) state.notes[id] = event.target.value;
    else delete state.notes[id];
    plugin.draft({ ...state, mode });
  });
  /* d, w and l for the views; j and k for the next and previous day or
     week; t back to the first day */
  document.addEventListener("keydown", (event) => {
    if (!payload || event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.target.closest && event.target.closest("textarea, input, [contenteditable]")) return;
    const key = event.key.toLowerCase();
    if (key === "d") setView("day");
    else if (key === "w") setView("week");
    else if (key === "l") setView("list");
    else if (key === "j") step(1);
    else if (key === "k") step(-1);
    else if (key === "t") goFirst();
    else return;
    event.preventDefault();
  });
})();
