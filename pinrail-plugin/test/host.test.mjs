// The app's side of the protocol, in Node: what every host does, whichever
// it is, given the callbacks only it can provide.

import assert from "node:assert/strict";
import test from "node:test";
import { PROTOCOL, createHost } from "../host/host.js";

const review = { id: "r1", title: "A review", status: "pending", payload: { n: 1 }, attachments: [] };

/** A host with recording callbacks, and the messages it posted. */
function host(overrides = {}) {
  const posted = [];
  const calls = { drafts: [], labels: [], opened: [], keys: [], left: 0, deferred: 0 };
  const state = { review, previous: null, readonly: false, ...overrides.state };
  // the time limit on a request, run by the test rather than the clock
  const timers = new Set();
  const h = createHost({
    setTimer: (fn) => {
      const timer = { fn };
      timers.add(timer);
      return timer;
    },
    clearTimer: (timer) => timers.delete(timer),
    onDefer: () => (calls.deferred += 1),
    post: (msg, transfer) => posted.push({ msg, transfer }),
    origin: "http://app.test",
    review: () => state.review,
    previous: () => state.previous,
    readonly: () => state.readonly,
    settings: () => ({ mode: "a" }),
    theme: () => "dark",
    loadDraft: () => overrides.draft ?? null,
    saveDraft: (data) => calls.drafts.push(data),
    label: (text) => calls.labels.push(text),
    open: (url) => calls.opened.push(url),
    appKey: (msg) => calls.keys.push(msg.key),
    onLeft: () => (calls.left += 1),
    ...overrides.options,
  });
  const types = () => posted.map((p) => p.msg.type);
  const last = (type) => posted.filter((p) => p.msg.type === type).at(-1)?.msg;
  const from = (msg) => h.receive({ pinrail: PROTOCOL, ...msg });
  /** Runs out the clock on every open request. */
  const expire = () => {
    for (const timer of [...timers]) {
      timers.delete(timer);
      timer.fn();
    }
  };
  /** Asks for the decision, and the number the request went out with. */
  const ask = () => {
    h.collect();
    return last("collect")?.req;
  };
  return { h, posted, calls, state, types, last, from, expire, ask };
}

const tick = () => new Promise((resolve) => setImmediate(resolve));

test("ready is answered with the appearance, then init with every field", () => {
  const { from, types, last } = host({ draft: { step: 2 } });
  from({ type: "ready" });
  assert.deepEqual(types(), ["appearance", "init"]);
  const init = last("init");
  assert.equal(init.pinrail, PROTOCOL);
  assert.deepEqual(init.review, { ...review, created_at: null, decision: null });
  assert.equal(init.previous, null);
  assert.equal(init.readonly, false);
  assert.deepEqual(init.draft, { step: 2 });
  assert.deepEqual(init.settings, { mode: "a" });
  assert.equal(init.app_origin, "http://app.test");
  assert.deepEqual(init.capabilities, []);
});

test("a read-only review is sent no draft, and keeps none", () => {
  const { from, last, calls } = host({ draft: { step: 2 }, state: { readonly: true } });
  from({ type: "ready" });
  assert.equal(last("init").draft, null);
  from({ type: "draft", data: { step: 3 } });
  assert.deepEqual(calls.drafts, []);
});

test("a second ready is another page, which gets nothing from then on", () => {
  const { h, from, types, calls } = host();
  from({ type: "ready" });
  from({ type: "ready" });
  assert.equal(h.left, true);
  assert.equal(calls.left, 1);
  assert.deepEqual(types(), ["appearance", "init"]);
  h.collect();
  from({ type: "draft", data: 1 });
  assert.deepEqual(types(), ["appearance", "init"]);
  assert.deepEqual(calls.drafts, []);
});

test("a second page load is leaving too, and a reload starts the view anew", () => {
  const { h, from, types } = host();
  h.loaded();
  from({ type: "ready" });
  h.loaded();
  assert.equal(h.left, true);
  h.reload();
  h.loaded();
  from({ type: "ready" });
  assert.equal(h.left, false);
  assert.deepEqual(types(), ["appearance", "init", "appearance", "init"]);
});

test("nothing is posted before ready, or for a message that is not the protocol's", () => {
  const { h, posted, calls } = host();
  h.collect();
  h.settings({ mode: "b" });
  h.appearance("light");
  h.receive({ type: "draft", data: 1 });
  h.receive({ pinrail: 2, type: "draft", data: 1 });
  h.receive("ready");
  assert.deepEqual(posted, []);
  assert.deepEqual(calls.drafts, []);
});

test("drafts, labels, links and app keys reach the host's callbacks", () => {
  const { from, calls } = host();
  from({ type: "ready" });
  from({ type: "draft", data: false });
  from({ type: "status", label: "Approve 3" });
  from({ type: "status", label: "  " });
  from({ type: "open", url: "https://example.com" });
  from({ type: "open", url: 7 });
  from({ type: "key", key: "?" });
  assert.deepEqual(calls.drafts, [false]);
  assert.deepEqual(calls.labels, ["Approve 3"]);
  assert.deepEqual(calls.opened, ["https://example.com"]);
  assert.deepEqual(calls.keys, ["?"]);
});

test("collect is asked only of a ready view that can still decide, one request at a time", () => {
  const ready = host();
  ready.from({ type: "ready" });
  assert.equal(ready.h.collect(), true);
  assert.equal(ready.last("collect").req, 1);
  // while one is open, another press asks nothing
  assert.equal(ready.h.collect(), false);
  assert.equal(ready.posted.filter((p) => p.msg.type === "collect").length, 1);

  const locked = host({ state: { readonly: true } });
  locked.from({ type: "ready" });
  assert.equal(locked.h.collect(), false);
  assert.equal(locked.last("collect"), undefined);
});

test("a submit nobody asked for decides nothing", async () => {
  const handed = [];
  const { from, last } = host({
    options: { handOver: async (data) => (handed.push(data), { ok: true, decision: { data } }) },
  });
  from({ type: "ready" });
  from({ type: "submit", data: { ok: true } });
  from({ type: "submit", req: 1, data: { ok: true } });
  await tick();
  assert.deepEqual(handed, []);
  assert.equal(last("submitted"), undefined);
});

test("the answer to the open request is handed over, and a refused one gets its violations", async () => {
  const outcomes = [
    { ok: false, violations: [{ path: "/ok", message: "is required" }] },
    { ok: true, decision: { data: { ok: true }, decided_by: "me", decided_at: "now" } },
  ];
  const handed = [];
  const { from, last, ask } = host({ options: { handOver: async (data) => (handed.push(data), outcomes.shift()) } });
  from({ type: "ready" });
  from({ type: "submit", req: ask(), data: {} });
  await tick();
  assert.deepEqual(last("violations").errors, [{ path: "/ok", message: "is required" }]);
  // a refused decision closes its request: the next press asks again
  const second = ask();
  assert.equal(second, 2);
  from({ type: "submit", req: second, data: { ok: true } });
  await tick();
  assert.deepEqual(last("submitted").decision.data, { ok: true });
  assert.deepEqual(handed, [{}, { ok: true }]);
});

test("defer closes the request with nothing handed over, and the next press asks again", async () => {
  const handed = [];
  const { h, from, calls, ask } = host({ options: { handOver: async (data) => (handed.push(data), { ok: true }) } });
  from({ type: "ready" });
  const first = ask();
  from({ type: "defer", req: first });
  assert.equal(calls.deferred, 1);
  assert.equal(h.collecting, false);
  // a defer for a request that is not open counts for nothing
  from({ type: "defer", req: first });
  assert.equal(calls.deferred, 1);
  // a preview at the first press, the decision at the second
  from({ type: "submit", req: ask(), data: { ok: true } });
  await tick();
  assert.deepEqual(handed, [{ ok: true }]);
});

test("a second answer to one request is ignored", async () => {
  const handed = [];
  const { from, ask } = host({
    options: { handOver: async (data) => (handed.push(data), { ok: false, violations: [] }) },
  });
  from({ type: "ready" });
  const req = ask();
  from({ type: "submit", req, data: 1 });
  from({ type: "submit", req, data: 2 });
  from({ type: "defer", req });
  await tick();
  assert.deepEqual(handed, [1]);
});

test("an answer after the time limit, a reload or the review's end decides nothing, nor answers a later request", async () => {
  const handed = [];
  const late = host({ options: { handOver: async (data) => (handed.push(data), { ok: true }) } });
  late.from({ type: "ready" });
  const timedOut = late.ask();
  late.expire();
  assert.equal(late.h.collecting, false);
  late.from({ type: "submit", req: timedOut, data: "late" });
  // a later request has a number of its own
  const next = late.ask();
  assert.notEqual(next, timedOut);
  late.from({ type: "submit", req: timedOut, data: "late again" });

  const reloaded = host({ options: { handOver: async (data) => (handed.push(data), { ok: true }) } });
  reloaded.from({ type: "ready" });
  const before = reloaded.ask();
  reloaded.h.reload();
  reloaded.from({ type: "ready" });
  reloaded.from({ type: "submit", req: before, data: "after reload" });

  const ended = host({ options: { handOver: async (data) => (handed.push(data), { ok: true }) } });
  ended.from({ type: "ready" });
  const pending = ended.ask();
  ended.state.readonly = true;
  ended.h.changed();
  ended.from({ type: "submit", req: pending, data: "after the end" });
  await tick();
  assert.deepEqual(handed, []);
});

test("a read-only view hands nothing over, and a host without a hand-over leaves submit alone", async () => {
  const bare = host();
  bare.from({ type: "ready" });
  bare.from({ type: "submit", req: bare.ask(), data: 2 });
  await tick();
  assert.equal(bare.last("submitted"), undefined);
  assert.equal(bare.last("violations"), undefined);
  assert.equal(bare.h.collecting, false);
});

test("a hand-over that throws comes back as a violation", async () => {
  const { from, last, ask } = host({
    options: {
      handOver: async () => {
        throw new Error("the server is gone");
      },
    },
  });
  from({ type: "ready" });
  from({ type: "submit", req: ask(), data: 1 });
  await tick();
  assert.deepEqual(last("violations").errors, [{ path: "", message: "the server is gone" }]);
});

test("⌘/Ctrl+Enter inside the view starts the hand-over, and goes to the host when it has its own", () => {
  const plain = host();
  plain.from({ type: "ready" });
  plain.from({ type: "key", key: "Enter", code: "Enter", metaKey: true, ctrlKey: false });
  assert.equal(plain.last("collect").req, 1);
  assert.deepEqual(plain.calls.keys, []);
  // without a modifier it is no hand-over
  plain.from({ type: "key", key: "Enter", code: "Enter", metaKey: false, ctrlKey: false });
  assert.deepEqual(plain.calls.keys, ["Enter"]);

  let pressed = 0;
  const own = host({ options: { handOverKey: () => (pressed += 1) } });
  own.from({ type: "ready" });
  own.from({ type: "key", key: "Enter", code: "Enter", metaKey: false, ctrlKey: true });
  assert.equal(pressed, 1);
  assert.equal(own.last("collect"), undefined);
});

test("a review that becomes read-only from elsewhere is sent init again, but not one the view handed over", async () => {
  const withdrawn = host();
  withdrawn.from({ type: "ready" });
  withdrawn.state.readonly = true;
  withdrawn.h.changed();
  assert.equal(withdrawn.last("init").readonly, true);

  const decided = host({ options: { handOver: async (data) => ({ ok: true, decision: { data } }) } });
  decided.from({ type: "ready" });
  decided.from({ type: "submit", req: decided.ask(), data: 1 });
  await tick();
  decided.state.readonly = true;
  decided.h.changed();
  assert.equal(decided.posted.filter((p) => p.msg.type === "init").length, 1);
});

test("a file the review lists is fetched and transferred; any other is refused", async () => {
  const listed = { name: "note.txt", size: 5, media_type: "text/plain", sha256: "x" };
  const asked = [];
  const { from, posted } = host({
    state: { review: { ...review, attachments: [listed] } },
    options: {
      file: async (from, entry, round) => {
        asked.push([from.id, entry.name, round]);
        return new TextEncoder().encode("hello").buffer;
      },
    },
  });
  from({ type: "ready" });
  from({ type: "attachment", req: 7, name: "note.txt" });
  from({ type: "attachment", req: 8, name: "missing.txt" });
  await tick();
  const answers = posted.filter((p) => p.msg.type === "attachment");
  const found = answers.find((p) => p.msg.req === 7);
  assert.equal(found.msg.ok, true);
  assert.equal(found.msg.media_type, "text/plain");
  assert.equal(new TextDecoder().decode(found.msg.bytes), "hello");
  assert.deepEqual(found.transfer, [found.msg.bytes]);
  const refused = answers.find((p) => p.msg.req === 8);
  assert.equal(refused.msg.ok, false);
  assert.match(refused.msg.error, /no attachment "missing.txt" on this review/);
  assert.deepEqual(asked, [["r1", "note.txt", "current"]]);
});

test("a file that arrives after the frame moved to another review is not sent", async () => {
  const listed = { name: "a.png", size: 1, media_type: "image/png", sha256: "x" };
  let release;
  const { from, state, posted } = host({
    state: { review: { ...review, attachments: [listed] } },
    options: { file: () => new Promise((resolve) => (release = () => resolve(new ArrayBuffer(1)))) },
  });
  from({ type: "ready" });
  from({ type: "attachment", req: 1, name: "a.png" });
  state.review = { ...review, id: "r2" };
  release();
  await tick();
  assert.equal(posted.filter((p) => p.msg.type === "attachment").length, 0);
});

test("a settings change is answered by its request number: kept, or refused with the errors", async () => {
  const patches = [];
  const { from, posted } = host({
    options: {
      setSettings: async (patch) => (patches.push(patch), patch.mode === "z" ? [{ path: "/mode", message: "no" }] : []),
    },
  });
  from({ type: "ready" });
  from({ type: "settings_set", req: 1, patch: { mode: "b" } });
  from({ type: "settings_set", req: 2, patch: [1] });
  from({ type: "settings_set", req: 3, patch: { mode: "z" } });
  // one with no number is not the protocol's
  from({ type: "settings_set", patch: { mode: "c" } });
  await tick();
  assert.deepEqual(patches, [{ mode: "b" }, { mode: "z" }]);
  // in the order the requests were made: an answer may come before an earlier one
  const answers = posted
    .filter((p) => p.msg.type === "settings")
    .map((p) => p.msg)
    .sort((a, b) => a.req - b.req);
  assert.deepEqual(answers, [
    { pinrail: PROTOCOL, type: "settings", req: 1, ok: true },
    {
      pinrail: PROTOCOL,
      type: "settings",
      req: 2,
      ok: false,
      errors: [{ path: "", message: "a settings change is an object of settings" }],
    },
    { pinrail: PROTOCOL, type: "settings", req: 3, ok: false, errors: [{ path: "/mode", message: "no" }] },
  ]);
  assert.equal(posted.filter((p) => p.msg.type === "violations").length, 0, "violations are for a decision alone");
});

test("every message in and out reaches the observer, after the host is let go nothing does", () => {
  const seen = [];
  const { h, from, posted } = host({ options: { observe: (dir, msg) => seen.push(`${dir} ${msg.type}`) } });
  from({ type: "ready" });
  from({ type: "status", label: "Go" });
  assert.deepEqual(seen, ["in ready", "out appearance", "out init", "in status"]);
  h.dispose();
  from({ type: "status", label: "Again" });
  h.collect();
  assert.equal(posted.length, 2);
});

test("a view receives the review's stated fields and no others, for this round and the previous", () => {
  const api = {
    id: "r2",
    title: "Round two",
    status: "decided",
    created_at: "2026-10-04T10:00:00Z",
    payload: { n: 2 },
    attachments: [{ name: "a.png", size: 3, media_type: "image/png", sha256: "abc", stored_at: "x" }],
    decision: { data: { ok: true }, decided_by: "me", decided_at: "2026-10-04T11:00:00Z", summary: { counts: [] } },
    origin: { repo: "acme" },
    requested_by: "claude-code",
    plugin: "list",
    plugin_version: "1.0.0",
    plugin_bundle: "hash",
    agent_note: "a note",
    revises: "r1",
    expires_at: null,
  };
  const { from, last } = host({ state: { review: api, previous: { ...api, id: "r1", decision: null } } });
  from({ type: "ready" });
  const init = last("init");
  const fields = ["id", "title", "status", "created_at", "payload", "attachments", "decision"];
  assert.deepEqual(Object.keys(init.review), fields);
  assert.deepEqual(Object.keys(init.previous), fields);
  assert.deepEqual(init.review.attachments, [{ name: "a.png", size: 3, media_type: "image/png", sha256: "abc" }]);
  assert.deepEqual(init.review.decision, { data: { ok: true }, decided_by: "me", decided_at: "2026-10-04T11:00:00Z" });
  assert.equal(init.previous.decision, null);
});

test("submitted carries the decision as a view sees it", async () => {
  const stored = { data: { ok: true }, decided_by: "me", decided_at: "now", summary: { counts: [] }, agent_note: "x" };
  const { from, last, ask } = host({ options: { handOver: async () => ({ ok: true, decision: stored }) } });
  from({ type: "ready" });
  from({ type: "submit", req: ask(), data: { ok: true } });
  await tick();
  assert.deepEqual(last("submitted").decision, { data: { ok: true }, decided_by: "me", decided_at: "now" });
});
