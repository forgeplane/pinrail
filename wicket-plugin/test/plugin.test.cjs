const { test } = require("node:test");
const assert = require("node:assert/strict");
const { Wicket, fakeEnv, fakeDocument, shell, gate, init } = require("./helpers.cjs");

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

test("the theme comes from the environment first, and the shell can still change it", () => {
  // In a browser this is the theme on the frame's URL, which is the only one
  // that can be in place before the view paints.
  const env = Object.assign(fakeEnv(), { initialTheme: () => "light" });
  const seen = [];
  const plugin = Wicket.createPlugin(env, { resize: "manual", onAppearance: (t) => seen.push(t) });

  assert.equal(plugin.theme, "light", "in the shell's theme before a single message");
  assert.deepEqual(seen, [], "and without anything to react to");

  env.deliver(shell({ type: "appearance", theme: "dark" }));
  assert.equal(plugin.theme, "dark", "the shell still owns every later change");
  assert.deepEqual(env.themes, ["dark"]);
});

test("an environment with no theme of its own leaves the plugin dark", () => {
  const plugin = Wicket.createPlugin(fakeEnv(), { resize: "manual" });
  assert.equal(plugin.theme, "dark");
});

test("appearance applies the theme, exposes it, and ignores anything else", () => {
  const env = fakeEnv();
  const seen = [];
  const plugin = Wicket.createPlugin(env, { resize: "manual", onAppearance: (t) => seen.push(t) });

  assert.equal(plugin.theme, "dark", "dark until the environment or the shell says otherwise");

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

  // The parser is the package's own dependency here and the app's in a
  // browser; what it renders in a view's frame is settled in markdown.spec.ts.
  assert.equal(Wicket.markdown("# Title"), "<h1>Title</h1>\n");
  assert.equal(Wicket.markdownInline("a *b*"), "a <em>b</em>");
  // raw HTML is escaped: a view's frame runs inline scripts
  assert.equal(Wicket.markdown("<script>alert(1)</script>"), "<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>\n");
  // and an address a click would run is not made a link at all
  assert.equal(Wicket.markdown("[x](javascript:alert(1))"), "<p>[x](javascript:alert(1))</p>\n");

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

test("icon markup takes the name, the colour of its text, and nothing from a payload", () => {
  const plain = Wicket.icon("check");
  assert.match(plain, /class="wi"/);
  assert.match(plain, /--wi:url\(\/sdk\/v1\/icons\/check\.svg\)/);
  assert.match(plain, /aria-hidden="true"/, "decorative unless it is given a name");
  assert.match(plain, /data-icon="check"/, "the name stays on the element, to find a typo by");

  // A view may take the name from a gate payload, which is not ours to trust.
  const hostile = Wicket.icon('x.svg) url(https://evil.test/pixel.svg');
  assert.match(hostile, /--wi:url\(\/sdk\/v1\/icons\/[a-z0-9-]*\.svg\);/);
  assert.equal(hostile.includes("evil.test"), false, "the host is gone");
  assert.equal(/url\(/.test(hostile.replace("url(/sdk/v1/icons/", "")), false, "no second url()");

  assert.match(Wicket.icon("check", { size: 18 }), /--wi-size:18px/);
  assert.match(Wicket.icon("check", { size: "1.25em" }), /--wi-size:1\.25em/);
  assert.match(Wicket.icon("check", { class: "spacer" }), /class="wi spacer"/);

  const named = Wicket.icon("trash-2", { label: "delete" });
  assert.match(named, /role="img"/);
  assert.match(named, /aria-label="delete"/);
  assert.equal(named.includes("aria-hidden"), false);

  assert.match(Wicket.icon(null), /data-icon=""/, "a missing name is not a crash");
});

test("settings arrive with init and again as a message; setSetting asks the shell", () => {
  const env = fakeEnv();
  const seen = [];
  const plugin = Wicket.createPlugin(env, {
    resize: "manual",
    onInit: (i) => seen.push(["init", i.settings]),
    onSettings: (s) => seen.push(["settings", s]),
  });
  env.deliver(init({ settings: { diff: "split", wrap: true } }));
  assert.deepEqual(plugin.settings, { diff: "split", wrap: true });
  env.deliver(shell({ type: "settings", settings: { diff: "inline", wrap: true } }));
  assert.deepEqual(plugin.settings, { diff: "inline", wrap: true });
  assert.deepEqual(seen, [
    ["init", { diff: "split", wrap: true }],
    ["settings", { diff: "inline", wrap: true }],
  ]);

  plugin.setSetting("diff", "split");
  assert.deepEqual(env.last("settings_set").msg, { wicket: 1, type: "settings_set", patch: { diff: "split" } });

  // a shell that says nothing, or nonsense, about settings leaves them empty
  env.deliver(init());
  assert.deepEqual(plugin.settings, {});
  env.deliver(shell({ type: "settings", settings: [1, 2] }));
  assert.deepEqual(plugin.settings, {});
});

test("a forwarded key lands on the document and on onKey; junk is ignored", () => {
  const env = fakeEnv();
  const seen = [];
  Wicket.createPlugin(env, { resize: "manual", onKey: (k) => seen.push(k) });
  env.deliver(init());
  env.deliver(shell({ type: "key", key: "j", code: "KeyJ" }));
  env.deliver(shell({ type: "key", key: "M", code: "KeyM", metaKey: true, shiftKey: true }));
  env.deliver(shell({ type: "key" }));
  const expected = [
    { key: "j", code: "KeyJ", metaKey: false, ctrlKey: false, altKey: false, shiftKey: false },
    { key: "M", code: "KeyM", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true },
  ];
  assert.deepEqual(env.keys, expected);
  assert.deepEqual(seen, expected);
});

test("the SDK announces the package's version", () => {
  const pkg = require("../package.json");
  assert.equal(Wicket.version, pkg.version);
  assert.equal(Wicket.protocol, 1);
  assert.equal(pkg.version.split(".")[0], String(Wicket.protocol));
});
