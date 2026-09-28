/* Shared scheduling rules; no browser, network, or storage dependencies. */
(function (root) {
  "use strict";
  const instant = (value) => Date.parse(value);
  const overlaps = (a, b) => instant(a.start) < instant(b.end) && instant(b.start) < instant(a.end);
  const parts = (value, timezone) =>
    Object.fromEntries(
      new Intl.DateTimeFormat("en-GB", {
        timeZone: timezone,
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        hourCycle: "h23",
      })
        .formatToParts(new Date(value))
        .map((p) => [p.type, p.value]),
    );
  const dateKey = (value, timezone) => {
    const p = parts(value, timezone);
    return `${p.year}-${p.month}-${p.day}`;
  };
  const minutes = (value, timezone) => {
    const p = parts(value, timezone);
    return +p.hour * 60 + +p.minute;
  };
  const addDays = (day, n) => new Date(Date.parse(`${day}T12:00:00Z`) + n * 86400000).toISOString().slice(0, 10);
  const validDate = (value) =>
    typeof value === "string" &&
    /^\d{4}-\d{2}-\d{2}$/.test(value) &&
    Number.isFinite(Date.parse(`${value}T12:00:00Z`)) &&
    addDays(value, 0) === value;
  const validTime = (value) =>
    typeof value === "string" &&
    /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(value) &&
    validDate(value.slice(0, 10)) &&
    Number.isFinite(instant(value));
  function validate(payload) {
    if (!payload || typeof payload !== "object") throw new Error("The agent must provide a calendar payload.");
    try {
      new Intl.DateTimeFormat("en", { timeZone: payload.timezone }).format();
    } catch {
      throw new Error("The calendar timezone is invalid.");
    }
    if (
      !payload.timezone ||
      !validDate(payload.start_date) ||
      !Number.isInteger(payload.days) ||
      payload.days < 1 ||
      payload.days > 14
    )
      throw new Error("Provide a timezone, a valid start date, and between 1 and 14 days.");
    if (!Array.isArray(payload.blocked) || !Array.isArray(payload.items))
      throw new Error("Provide blocked calendar events and items to schedule.");
    const ids = new Set();
    const checkId = (id) => {
      if (typeof id !== "string" || !/^[a-zA-Z0-9][a-zA-Z0-9_-]*$/.test(id) || ids.has(id))
        throw new Error("Every item, option and calendar event needs a unique ID.");
      ids.add(id);
    };
    const checkSlot = (slot) => {
      checkId(slot.id);
      if (!validTime(slot.start) || !validTime(slot.end) || instant(slot.start) >= instant(slot.end))
        throw new Error("Every time slot needs an end after its start and an explicit UTC offset.");
    };
    const lastDay = addDays(payload.start_date, payload.days - 1);
    payload.blocked.forEach((event) => {
      checkSlot(event);
      if (typeof event.title !== "string") throw new Error("Calendar events need titles.");
    });
    payload.items.forEach((item) => {
      checkId(item.id);
      if (typeof item.title !== "string" || !Array.isArray(item.options))
        throw new Error("Every activity needs a title and an options list.");
      item.options.forEach((option) => {
        checkSlot(option);
        if (
          dateKey(option.start, payload.timezone) < payload.start_date ||
          dateKey(instant(option.end) - 1, payload.timezone) > lastDay
        )
          throw new Error("All suggested times must fall within the displayed calendar range.");
      });
    });
    return payload;
  }
  function selectedSlots(payload, state) {
    return payload.items.flatMap((item) => {
      const option = item.options.find((o) => o.id === state.selected[item.id]);
      return option ? [{ ...option, item_id: item.id }] : [];
    });
  }
  function conflicts(payload, state, itemId, option) {
    return [
      ...payload.blocked.filter((b) => overlaps(b, option)),
      ...selectedSlots(payload, state).filter((s) => s.item_id !== itemId && overlaps(s, option)),
    ];
  }
  function available(payload, state, item) {
    return item.options.filter((option) => conflicts(payload, state, item.id, option).length === 0);
  }
  /* Per item: a chosen option, or `deferred` (find another time), or
     `declined` (do not schedule it), with an optional note for either. A draft
     keeps notes as an object; a decision as a list. */
  function restore(payload, raw) {
    const state = { selected: Object.create(null), deferred: [], declined: [], notes: Object.create(null) };
    if (!raw) return state;
    const choices = raw.selected || Object.fromEntries((raw.selections || []).map((s) => [s.item_id, s.option_id]));
    const notes = Array.isArray(raw.notes)
      ? Object.fromEntries(raw.notes.map((n) => [n.item_id, n.note]))
      : raw.notes || {};
    for (const item of payload.items) {
      const option = item.options.find((o) => o.id === choices[item.id]);
      if (option && conflicts(payload, state, item.id, option).length === 0) state.selected[item.id] = option.id;
      else if (Array.isArray(raw.declined) && raw.declined.includes(item.id)) state.declined.push(item.id);
      else if (Array.isArray(raw.deferred) && raw.deferred.includes(item.id)) state.deferred.push(item.id);
      if (typeof notes[item.id] === "string" && notes[item.id].trim() && !state.selected[item.id])
        state.notes[item.id] = notes[item.id];
    }
    return state;
  }
  /* Asks for another time, or declines, or neither: one leaves the other */
  function settle(state, itemId, how) {
    delete state.selected[itemId];
    const was = state[how].includes(itemId);
    state.deferred = state.deferred.filter((id) => id !== itemId);
    state.declined = state.declined.filter((id) => id !== itemId);
    if (!was) state[how].push(itemId);
    else delete state.notes[itemId];
    return !was;
  }
  function choose(payload, state, itemId, optionId) {
    const item = payload.items.find((i) => i.id === itemId);
    const option = item?.options.find((o) => o.id === optionId);
    if (!option || conflicts(payload, state, itemId, option).length) return false;
    if (state.selected[itemId] === optionId) delete state.selected[itemId];
    else state.selected[itemId] = optionId;
    state.deferred = state.deferred.filter((id) => id !== itemId);
    state.declined = (state.declined || []).filter((id) => id !== itemId);
    delete (state.notes || {})[itemId];
    return true;
  }
  function decision(payload, state) {
    // Revalidate a draft at the hand-over boundary, not just at click time.
    const safe = restore(payload, state);
    const unresolved = payload.items.filter(
      (item) => !safe.selected[item.id] && !safe.deferred.includes(item.id) && !safe.declined.includes(item.id),
    );
    if (unresolved.length)
      throw new Error(`Choose a time, ask for another, or decline: ${unresolved.map((i) => i.title).join(", ")}.`);
    const out = {
      verdict: safe.deferred.length ? "revise" : "approve",
      timezone: payload.timezone,
      selections: selectedSlots(payload, safe).map(({ item_id, id, start, end }) => ({
        item_id,
        option_id: id,
        start,
        end,
      })),
      deferred: safe.deferred,
    };
    if (safe.declined.length) out.declined = safe.declined;
    const notes = payload.items
      .filter((i) => safe.notes[i.id])
      .map((i) => ({ item_id: i.id, note: safe.notes[i.id].trim() }));
    if (notes.length) out.notes = notes;
    return out;
  }
  // Split overnight events into local calendar days; conflicts always use instants.
  function segments(slot, day, timezone) {
    const first = dateKey(slot.start, timezone),
      last = dateKey(instant(slot.end) - 1, timezone);
    if (day < first || day > last) return null;
    const start = day === first ? minutes(slot.start, timezone) : 0;
    const end = dateKey(slot.end, timezone) === day ? minutes(slot.end, timezone) : 1440;
    return { ...slot, from: start, to: Math.max(start + 15, end) };
  }
  // Divide each connected overlap group into lanes, preserving every click target.
  function lanes(events) {
    const sorted = events.map((e) => ({ ...e })).sort((a, b) => a.from - b.from || b.to - a.to);
    let group = [],
      end = -Infinity;
    function finish() {
      const ends = [];
      for (const e of group) {
        let lane = ends.findIndex((t) => t <= e.from);
        if (lane === -1) lane = ends.length;
        ends[lane] = e.to;
        e.lane = lane;
      }
      for (const e of group) e.lanes = ends.length;
    }
    for (const event of sorted) {
      if (event.from >= end) {
        finish();
        group = [];
        end = -Infinity;
      }
      group.push(event);
      end = Math.max(end, event.to);
    }
    finish();
    return sorted;
  }
  const api = {
    overlaps,
    dateKey,
    minutes,
    addDays,
    validate,
    available,
    conflicts,
    restore,
    settle,
    choose,
    decision,
    selectedSlots,
    segments,
    lanes,
  };
  if (typeof module !== "undefined") module.exports = api;
  else root.CalendarCore = api;
})(typeof globalThis !== "undefined" ? globalThis : this);
