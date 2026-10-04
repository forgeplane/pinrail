// The app's side of the protocol, in Node: what every host does, whichever
// it is, given the callbacks only it can provide.

import assert from "node:assert/strict";
import test from "node:test";
import { PROTOCOL, createHost } from "../host/host.js";

const review = { id: "r1", title: "A review", status: "pending", payload: { n: 1 }, attachments: [] };

/** A host with recording callbacks, and the messages it posted. */
function host(overrides = {}) {
  const posted = [];
  const calls = { drafts: [], labels: [], opened: [], keys: [], left: 0 };
  const state = { review, previous: null, readonly: false, ...overrides.state };
  const h = createHost({
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
  return { h, posted, calls, state, types, last, from };
}

const tick = () => new Promise((resolve) => setImmediate(resolve));

test("ready is answered with the appearance, then init with every field", () => {
  const { from, types, last } = host({ draft: { step: 2 } });
  from({ type: "ready" });
  assert.deepEqual(types(), ["appearance", "init"]);
  const init = last("init");
  assert.equal(init.pinrail, PROTOCOL);
  assert.deepEqual(init.review, review);
  assert.equal(init.previous, null);
  assert.equal(init.readonly, false);
  assert.deepEqual(init.draft, { step: 2 });
  assert.deepEqual(init.settings, { mode: "a" });
  assert.equal(init.shell_origin, "http://app.test");
  assert.deepEqual(init.capabilities, ["attachments"]);
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

test("collect is asked only of a ready view that can still decide", () => {
  const ready = host();
  ready.from({ type: "ready" });
  ready.h.collect();
  assert.equal(ready.last("collect").type, "collect");

  const locked = host({ state: { readonly: true } });
  locked.from({ type: "ready" });
  locked.h.collect();
  assert.equal(locked.last("collect"), undefined);
});

test("a decision handed over is answered submitted, and a refused one with its violations", async () => {
  const outcomes = [
    { ok: false, violations: [{ path: "/ok", message: "is required" }] },
    { ok: true, decision: { data: { ok: true }, decided_by: "me", decided_at: "now" } },
  ];
  const handed = [];
  const { from, last } = host({ options: { handOver: async (data) => (handed.push(data), outcomes.shift()) } });
  from({ type: "ready" });
  from({ type: "submit", data: {} });
  await tick();
  assert.deepEqual(last("violations").errors, [{ path: "/ok", message: "is required" }]);
  from({ type: "submit", data: { ok: true } });
  await tick();
  assert.deepEqual(last("submitted").decision.data, { ok: true });
  assert.deepEqual(handed, [{}, { ok: true }]);
});

test("a second submit while one is being handed over is ignored", async () => {
  let release;
  const handed = [];
  const { h, from } = host({
    options: {
      handOver: (data) => {
        handed.push(data);
        return new Promise((resolve) => (release = () => resolve({ ok: true, decision: { data } })));
      },
    },
  });
  from({ type: "ready" });
  from({ type: "submit", data: 1 });
  from({ type: "submit", data: 2 });
  assert.equal(h.handingOver, true);
  h.collect();
  release();
  await tick();
  assert.deepEqual(handed, [1]);
});

test("a read-only view hands nothing over, and a host without a hand-over leaves submit alone", async () => {
  const handed = [];
  const locked = host({
    state: { readonly: true },
    options: { handOver: async (d) => (handed.push(d), { ok: true }) },
  });
  locked.from({ type: "ready" });
  locked.from({ type: "submit", data: 1 });
  const bare = host();
  bare.from({ type: "ready" });
  bare.from({ type: "submit", data: 2 });
  await tick();
  assert.deepEqual(handed, []);
  assert.equal(bare.last("submitted"), undefined);
  assert.equal(bare.last("violations"), undefined);
});

test("a hand-over that throws comes back as a violation", async () => {
  const { from, last } = host({
    options: {
      handOver: async () => {
        throw new Error("the server is gone");
      },
    },
  });
  from({ type: "ready" });
  from({ type: "submit", data: 1 });
  await tick();
  assert.deepEqual(last("violations").errors, [{ path: "", message: "the server is gone" }]);
});

test("a review that becomes read-only from elsewhere is sent init again, but not one the view handed over", async () => {
  const withdrawn = host();
  withdrawn.from({ type: "ready" });
  withdrawn.state.readonly = true;
  withdrawn.h.changed();
  assert.equal(withdrawn.last("init").readonly, true);

  const decided = host({ options: { handOver: async (data) => ({ ok: true, decision: { data } }) } });
  decided.from({ type: "ready" });
  decided.from({ type: "submit", data: 1 });
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

test("a settings change the host refuses comes back as violations", async () => {
  const patches = [];
  const { from, last } = host({
    options: {
      setSettings: async (patch) => (patches.push(patch), patch.mode === "z" ? [{ path: "/mode", message: "no" }] : []),
    },
  });
  from({ type: "ready" });
  from({ type: "settings_set", patch: { mode: "b" } });
  from({ type: "settings_set", patch: [1] });
  from({ type: "settings_set", patch: { mode: "z" } });
  await tick();
  assert.deepEqual(patches, [{ mode: "b" }, { mode: "z" }]);
  assert.deepEqual(last("violations").errors, [{ path: "/mode", message: "no" }]);
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
