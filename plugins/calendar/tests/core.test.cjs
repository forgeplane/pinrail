const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const C = require("../view/calendar-core.js");
const payload = require("../fixtures/01-personal-assistant.json").payload;
const fresh = () => C.restore(payload, null);

test("every supplied fixture is semantically valid", () => {
  for (const name of fs
    .readdirSync(path.join(__dirname, "../fixtures"))
    // a fixture's expected summaries are beside it, and are not a review
    .filter((n) => n.endsWith(".json") && !n.endsWith(".summary.json"))) {
    const review = require("../fixtures/" + name);
    assert.equal(C.validate(review.payload), review.payload);
    if (review.decision)
      assert.deepEqual(
        C.decision(review.payload, C.restore(review.payload, review.decision.data)),
        review.decision.data,
      );
  }
});
test("blocked calendar time is excluded before any choices", () => {
  assert(!C.available(payload, fresh(), payload.items[0]).some((o) => o.id === "doctor-busy"));
});
test("choosing a slot hides overlapping alternatives; clearing restores them", () => {
  const s = fresh();
  assert(C.choose(payload, s, "doctor", "doctor-mon"));
  assert(!C.available(payload, s, payload.items[1]).some((o) => o.id === "tennis-mon"));
  assert(!C.choose(payload, s, "tennis", "tennis-mon"));
  C.choose(payload, s, "doctor", "doctor-mon");
  assert(C.available(payload, s, payload.items[1]).some((o) => o.id === "tennis-mon"));
});
test("switching alternatives releases old conflicts and creates new ones", () => {
  const s = fresh();
  C.choose(payload, s, "tennis", "tennis-tue");
  assert(!C.available(payload, s, payload.items[2]).some((o) => o.id === "dinner-tue"));
  C.choose(payload, s, "tennis", "tennis-wed");
  assert(C.available(payload, s, payload.items[2]).some((o) => o.id === "dinner-tue"));
  assert(!C.available(payload, s, payload.items[2]).some((o) => o.id === "dinner-wed"));
});
test("touching intervals do not conflict; comparisons use instants across offsets", () => {
  assert(
    !C.overlaps(
      { start: "2026-09-21T10:00:00+03:00", end: "2026-09-21T11:00:00+03:00" },
      { start: "2026-09-21T08:00:00Z", end: "2026-09-21T09:00:00Z" },
    ),
  );
  assert(
    C.overlaps(
      { start: "2026-09-21T10:00:00+03:00", end: "2026-09-21T11:00:00+03:00" },
      { start: "2026-09-21T07:30:00Z", end: "2026-09-21T08:30:00Z" },
    ),
  );
});
test("corrupt, removed and conflicting draft choices cannot be submitted", () => {
  const s = C.restore(payload, {
    selected: { doctor: "doctor-mon", tennis: "tennis-mon", dinner: "deleted" },
    deferred: ["not-an-item"],
  });
  assert.equal(s.selected.doctor, "doctor-mon");
  assert(!s.selected.tennis);
  assert(!s.selected.dinner);
  assert.throws(() => C.decision(payload, s), /Choose a time/);
});
test("handover contains only exact selected options, with explicit deferred IDs", () => {
  const s = fresh();
  C.choose(payload, s, "doctor", "doctor-mon");
  C.choose(payload, s, "tennis", "tennis-tue");
  s.deferred.push("dinner");
  const result = C.decision(payload, s);
  assert.equal(result.verdict, "revise");
  assert.deepEqual(result.deferred, ["dinner"]);
  assert.deepEqual(
    result.selections.map((x) => x.option_id),
    ["doctor-mon", "tennis-tue"],
  );
  assert.equal(result.selections[0].start, payload.items[0].options[0].start);
});
test("overnight event splits without losing its second day", () => {
  const slot = { id: "night", start: "2026-09-21T23:00:00+03:00", end: "2026-09-22T01:00:00+03:00" };
  assert.equal(C.segments(slot, "2026-09-21", "Europe/Athens").to, 1440);
  assert.equal(C.segments(slot, "2026-09-22", "Europe/Athens").from, 0);
  assert.equal(C.segments(slot, "2026-09-22", "Europe/Athens").to, 60);
  assert.equal(C.segments(slot, "2026-09-23", "Europe/Athens"), null);
});
test("overlapping visual events occupy different lanes", () => {
  const result = C.lanes([
    { id: "a", from: 60, to: 120 },
    { id: "b", from: 90, to: 180 },
    { id: "c", from: 120, to: 150 },
    { id: "d", from: 200, to: 220 },
  ]);
  assert.equal(result[0].lanes, 2);
  assert.notEqual(result[0].lane, result[1].lane);
  assert.notEqual(result[1].lane, result[2].lane);
  assert.equal(result[3].lanes, 1);
});
test("invalid zones, duplicate IDs, invalid dates and out-of-range suggestions fail closed", () => {
  for (const mutate of [
    (p) => (p.timezone = "No/Such_Zone"),
    (p) => (p.items[0].options[0].id = "doctor"),
    (p) => (p.items[0].options[0].end = p.items[0].options[0].start),
    (p) => (p.start_date = "2026-02-30"),
    (p) => (p.items[0].options[0].start = "2026-08-21T10:00:00+03:00"),
  ]) {
    const p = structuredClone(payload);
    mutate(p);
    assert.throws(() => C.validate(p));
  }
});
test("an item can be declined or sent back for another time, each with a note", () => {
  const s = fresh();
  C.choose(payload, s, "doctor", "doctor-mon");
  C.settle(s, "tennis", "deferred");
  s.notes.tennis = "any evening after 18:00";
  C.settle(s, "dinner", "declined");
  s.notes.dinner = "  not this week  ";
  const result = C.decision(payload, s);
  assert.equal(result.verdict, "revise");
  assert.deepEqual(result.deferred, ["tennis"]);
  assert.deepEqual(result.declined, ["dinner"]);
  assert.deepEqual(result.notes, [
    { item_id: "tennis", note: "any evening after 18:00" },
    { item_id: "dinner", note: "not this week" },
  ]);
  // one leaves the other, and a chosen time clears both
  C.settle(s, "tennis", "declined");
  assert.deepEqual([s.deferred, s.declined], [[], ["dinner", "tennis"]]);
  C.choose(payload, s, "tennis", "tennis-tue");
  assert.deepEqual(s.declined, ["dinner"]);
  assert.equal(s.notes.tennis, undefined);
  // declining alone is still an approval of the rest
  C.settle(s, "dinner", "declined");
  C.settle(s, "dinner", "declined");
  assert.equal(C.decision(payload, s).verdict, "approve");
  // the decision restores as it was handed over
  const back = C.restore(payload, C.decision(payload, s));
  assert.deepEqual(C.decision(payload, back), C.decision(payload, s));
});
