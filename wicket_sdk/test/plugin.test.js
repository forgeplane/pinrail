const { test } = require("node:test");
const assert = require("node:assert/strict");
const { Wicket, fakeEnv, fakeDocument, shell, gate, init } = require("./helpers");

test("connect posts ready at once, to any origin", () => {
  const env = fakeEnv();
  Wicket.createPlugin(env, {});
  assert.deepEqual(env.posted, [{ msg: { wicket: 1, type: "ready" }, target: "*" }]);
});

test("init hands the gate, previous, readonly and draft to onInit and pins the shell origin", () => {
  const env = fakeEnv();
  const seen = [];
  const plugin = Wicket.createPlugin(env, { onInit: (i) => seen.push(i), resize: "manual" });
  env.deliver(init({ previous: gate({ id: "g_0" }), draft: { a: 1 } }));

  assert.equal(seen.length, 1);
  assert.equal(seen[0].gate.id, "g_1");
  assert.equal(seen[0].previous.id, "g_0");
  assert.equal(seen[0].readonly, false);
  assert.deepEqual(seen[0].draft, { a: 1 });
  assert.equal(plugin.shellOrigin, "http://shell.test");
  assert.equal(plugin.initialised, true);

  plugin.submit({ ok: true });
  assert.deepEqual(env.last("submit"), { msg: { wicket: 1, type: "submit", data: { ok: true } }, target: "http://shell.test" });
});

test("messages without the protocol marker, or from another origin once pinned, are ignored", () => {
  const env = fakeEnv();
  const calls = [];
  Wicket.createPlugin(env, { onInit: () => calls.push("init"), onViolations: () => calls.push("violations"), resize: "manual" });

  env.deliver({ type: "init" });
  env.deliver("hello");
  env.deliver(null);
  assert.deepEqual(calls, []);

  env.deliver(init());
  env.deliver(shell({ type: "violations", errors: [] }), "http://evil.test");
  env.deliver(shell({ type: "violations", errors: [] }));
  assert.deepEqual(calls, ["init", "violations"]);
});

test("violations, submitted and collect dispatch; submitted flips read-only and records the decision", () => {
  const env = fakeEnv();
  const calls = [];
  const plugin = Wicket.createPlugin(env, {
    resize: "manual",
    onViolations: (e) => calls.push(["violations", e]),
    onSubmitted: (d) => calls.push(["submitted", d]),
    onCollect: () => calls.push(["collect"]),
  });
  env.deliver(init());
  env.deliver(shell({ type: "violations", errors: [{ path: "/x", message: "bad" }] }));
  env.deliver(shell({ type: "collect" }));
  env.deliver(shell({ type: "submitted", decision: { decided_by: "a", data: { ok: true } } }));
  env.deliver(shell({ type: "collect" }));

  assert.deepEqual(calls, [
    ["violations", [{ path: "/x", message: "bad" }]],
    ["collect"],
    ["submitted", { decided_by: "a", data: { ok: true } }],
  ]);
  assert.equal(plugin.readonly, true);
  assert.equal(plugin.gate.status, "decided");
  assert.deepEqual(plugin.gate.decision.data, { ok: true });
});

test("the keyboard shortcut collects unless read-only or disabled", () => {
  const env = fakeEnv();
  let collected = 0;
  Wicket.createPlugin(env, { resize: "manual", onCollect: () => collected++ });
  env.deliver(init());
  env.pressShortcut();
  assert.equal(collected, 1);
  env.deliver(init({ readonly: true }));
  env.pressShortcut();
  assert.equal(collected, 1);

  const env2 = fakeEnv();
  Wicket.createPlugin(env2, { resize: "manual", shortcut: false, onCollect: () => collected++ });
  assert.equal(env2.shortcuts.length, 0);
});

test("drafts are debounced, coalesced, flushable, and dropped when read-only", () => {
  const env = fakeEnv();
  const plugin = Wicket.createPlugin(env, { resize: "manual" });
  env.deliver(init());

  plugin.draft({ n: 1 });
  plugin.draft({ n: 2 });
  assert.equal(env.types().filter((t) => t === "draft").length, 0);
  env.tick();
  assert.deepEqual(env.last("draft").msg.data, { n: 2 });

  plugin.draft({ n: 3 }, { flush: true });
  assert.deepEqual(env.last("draft").msg.data, { n: 3 });
  assert.equal(env.timers.length, 0);

  env.deliver(shell({ type: "submitted", decision: null }));
  plugin.draft({ n: 4 }, { flush: true });
  assert.deepEqual(env.last("draft").msg.data, { n: 3 });
});

test("resize: auto observes after init, fill posts once, manual posts nothing", () => {
  const auto = fakeEnv();
  Wicket.createPlugin(auto, {});
  assert.equal(auto.observers.length, 0);
  auto.deliver(init());
  assert.equal(auto.observers.length, 1);
  assert.deepEqual(auto.last("resize").msg, { wicket: 1, type: "resize", height: 321 });
  auto.deliver(init());
  assert.equal(auto.observers.length, 1, "a second init does not observe twice");

  const fill = fakeEnv();
  Wicket.createPlugin(fill, { resize: "fill" });
  fill.deliver(init());
  assert.deepEqual(fill.last("resize").msg, { wicket: 1, type: "resize", height: "fill" });

  const manual = fakeEnv();
  const plugin = Wicket.createPlugin(manual, { resize: "manual" });
  manual.deliver(init());
  assert.equal(manual.last("resize"), undefined);
  plugin.resize(500);
  assert.deepEqual(manual.last("resize").msg.height, 500);
});

test("status tells the shell what handing over would do", () => {
  const env = fakeEnv();
  const plugin = Wicket.createPlugin(env, { resize: "manual" });
  env.deliver(init());

  plugin.status({ label: "Hand over 3 decisions" });
  assert.deepEqual(env.last("status").msg, { wicket: 1, type: "status", label: "Hand over 3 decisions" });
  assert.equal(env.last("status").target, "http://shell.test");
});

test("collect is what the hand-over asks for, from the shell or the shortcut", () => {
  const env = fakeEnv();
  let asked = 0;
  Wicket.createPlugin(env, { resize: "manual", onCollect: () => asked++ });
  env.deliver(init());

  env.deliver(shell({ type: "collect" }));
  env.pressShortcut();
  assert.equal(asked, 2, "the button and the shortcut are the same request");

  env.deliver(shell({ type: "submitted", decision: null }));
  env.deliver(shell({ type: "collect" }));
  env.pressShortcut();
  assert.equal(asked, 2, "and neither reaches a decided gate");
});

test("appearance applies the theme, exposes it, and ignores anything else", () => {
  const env = fakeEnv();
  const seen = [];
  const plugin = Wicket.createPlugin(env, { resize: "manual", onAppearance: (t) => seen.push(t) });

  assert.equal(plugin.theme, "dark", "dark until the shell says otherwise");

  // the shell sends it before init, so the first paint is already correct
  env.deliver(shell({ type: "appearance", theme: "light" }));
  assert.equal(plugin.theme, "light");
  assert.deepEqual(env.themes, ["light"]);
  assert.deepEqual(seen, ["light"]);

  env.deliver(shell({ type: "appearance", theme: "sepia" }));
  env.deliver(shell({ type: "appearance" }));
  assert.equal(plugin.theme, "light");
  assert.deepEqual(env.themes, ["light"]);
  assert.deepEqual(seen, ["light"]);

  env.deliver(shell({ type: "appearance", theme: "dark" }));
  assert.deepEqual(env.themes, ["light", "dark"]);
});

test("a theme change neither re-initialises the view nor disturbs a draft", () => {
  const env = fakeEnv();
  let inits = 0;
  const plugin = Wicket.createPlugin(env, { resize: "manual", onInit: () => inits++ });
  env.deliver(init());
  plugin.draft({ n: 1 }, { flush: true });

  env.deliver(shell({ type: "appearance", theme: "light" }));

  assert.equal(inits, 1);
  assert.equal(env.types().filter((t) => t === "draft").length, 1);
  assert.deepEqual(env.last("draft").msg.data, { n: 1 });
  assert.equal(plugin.readonly, false);
});

test("escape, markdown and previousVerdict", () => {
  assert.equal(Wicket.escape(`<a href="x">&'`), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");

  const html = Wicket.markdown("Hi **there** `x < y`\n\n- one\n- two\n\n1. a\n\n```\ncode <b>\n```\n[l](https://e.x)");
  assert.equal(html, '<p>Hi <b>there</b> <code class="inl">x &lt; y</code></p><ul><li>one</li><li>two</li></ul><ol><li>a</li></ol><pre>code &lt;b&gt;</pre><p><a href="https://e.x" target="_blank" rel="noreferrer">l</a></p>');
  assert.equal(Wicket.markdown("<script>alert(1)</script>"), "<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>");
  assert.equal(Wicket.markdown("[x](javascript:alert(1))"), "<p>[x](javascript:alert(1))</p>");

  const previous = { decision: { data: { decisions: [{ id: 1, action: "reject", note: "no" }], undecided: [2] } } };
  assert.deepEqual(Wicket.previousVerdict(previous, 1), { action: "reject", note: "no" });
  assert.deepEqual(Wicket.previousVerdict(previous, 2), { action: "undecided", note: "" });
  assert.equal(Wicket.previousVerdict(previous, 3), null);
  assert.equal(Wicket.previousVerdict(null, 1), null);
});

test("layout builds a body on its own, and a header when asked for one", () => {
  const bare = fakeDocument();
  const plain = Wicket.layout({ document: bare });
  assert.equal(plain.header, null, "no header unless the view wants one");
  assert.equal(bare.body.className, "plugin-layout");
  assert.deepEqual(bare.body.children.map((n) => n.className), ["plugin-scroll"]);
  assert.deepEqual(plain.scroll.children, [plain.content], "the body scrolls, the document does not");

  const doc = fakeDocument();
  const view = Wicket.layout({ document: doc, title: "5 items" });
  assert.deepEqual(doc.body.children.map((n) => n.className), ["plugin-header", "plugin-scroll"]);
  assert.deepEqual(view.header.children.map((n) => n.className), [
    "plugin-title",
    "plugin-meta",
    "plugin-controls",
  ]);
  assert.equal(view.header.children[0].textContent, "5 items");
});

test("layout takes strings or elements, and replaces rather than appends", () => {
  const doc = fakeDocument();
  const button = doc.createElement("button");
  const view = Wicket.layout({ document: doc, meta: ["acme-api", "7 days"], controls: button });

  const [, meta, controls] = view.header.children;
  assert.deepEqual(meta.children.map((n) => n.textContent), ["acme-api", "7 days"]);
  assert.deepEqual(controls.children, [button]);

  assert.equal(view.title("4 items").meta("acme-worker"), view, "setters chain");
  assert.equal(view.header.children[0].textContent, "4 items");
  assert.deepEqual(meta.children.map((n) => n.textContent), ["acme-worker"]);

  view.meta(null);
  assert.deepEqual(meta.children, []);
});

test("layout can be put somewhere other than the body", () => {
  const doc = fakeDocument();
  const host = doc.createElement("div");
  const view = Wicket.layout({ document: doc, into: host, header: true });

  assert.deepEqual(doc.body.children, []);
  assert.deepEqual(host.children.map((n) => n.className), ["plugin-header", "plugin-scroll"]);
  assert.equal(host.children[1], view.scroll);
  assert.equal(view.scroll.children[0], view.content);
});

test("the module exposes a version and the protocol number", () => {
  assert.equal(Wicket.protocol, 1);
  assert.match(Wicket.version, /^\d+\.\d+\.\d+$/);
});
