const { test } = require("node:test");
const assert = require("node:assert/strict");
const { Pinrail, fakeEnv, fakeDocument, shell, review, init } = require("./helpers.cjs");

const settle = () => new Promise((resolve) => setImmediate(resolve));

test("connect posts ready at once, to any origin", () => {
  const env = fakeEnv();
  Pinrail.createPlugin(env, {});
  assert.deepEqual(env.posted, [{ msg: { pinrail: 1, type: "ready" }, target: "*" }]);
});

test("init hands the review, previous, readonly and draft to onInit and pins the shell origin", () => {
  const env = fakeEnv();
  const seen = [];
  const plugin = Pinrail.createPlugin(env, { onInit: (i) => seen.push(i), resize: "manual" });
  env.deliver(init({ previous: review({ id: "g_0" }), draft: { a: 1 } }));

  assert.equal(seen.length, 1);
  assert.equal(seen[0].review.id, "g_1");
  assert.equal(seen[0].previous.id, "g_0");
  assert.equal(seen[0].readonly, false);
  assert.deepEqual(seen[0].draft, { a: 1 });
  assert.equal(plugin.shellOrigin, "http://shell.test");
  assert.equal(plugin.initialised, true);
});

test("messages without the protocol marker, or from another origin once pinned, are ignored", () => {
  const env = fakeEnv();
  const calls = [];
  Pinrail.createPlugin(env, {
    onInit: () => calls.push("init"),
    onViolations: () => calls.push("violations"),
    resize: "manual",
  });

  env.deliver({ type: "init" });
  env.deliver("hello");
  env.deliver(null);
  assert.deepEqual(calls, []);

  env.deliver(init());
  env.deliver(shell({ type: "violations", errors: [] }), "http://evil.test");
  env.deliver(shell({ type: "violations", errors: [] }));
  assert.deepEqual(calls, ["init", "violations"]);
});

test("violations, submitted and collect dispatch; submitted flips read-only and records the decision", async () => {
  const env = fakeEnv();
  const calls = [];
  const plugin = Pinrail.createPlugin(env, {
    resize: "manual",
    onViolations: (e) => calls.push(["violations", e]),
    onSubmitted: (d) => calls.push(["submitted", d]),
    onCollect: () => calls.push(["collect"]),
  });
  env.deliver(init());
  env.deliver(shell({ type: "violations", errors: [{ path: "/x", message: "bad" }] }));
  env.deliver(shell({ type: "collect", req: 1 }));
  await settle();
  env.deliver(shell({ type: "submitted", decision: { decided_by: "a", data: { ok: true } } }));
  env.deliver(shell({ type: "collect", req: 2 }));
  await settle();

  assert.deepEqual(calls, [
    ["violations", [{ path: "/x", message: "bad" }]],
    ["collect"],
    ["submitted", { decided_by: "a", data: { ok: true } }],
  ]);
  assert.equal(plugin.readonly, true);
  assert.equal(plugin.review.status, "decided");
  assert.deepEqual(plugin.review.decision.data, { ok: true });
  // a decided review answers the app's request with nothing to hand over
  assert.deepEqual(env.last("defer").msg, { pinrail: 1, type: "defer", req: 2 });
});

test("⌘/Ctrl+Enter is the app's: the view registers no shortcut of its own", () => {
  const env = fakeEnv();
  Pinrail.createPlugin(env, { resize: "manual", onCollect: () => ({ ok: true }) });
  env.deliver(init());
  assert.equal(env.shortcuts.length, 0);
  env.appKeys[0]({ key: "Enter", code: "Enter", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false });
  assert.deepEqual(env.last("key").msg, {
    pinrail: 1,
    type: "key",
    key: "Enter",
    code: "Enter",
    metaKey: true,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
  });
  assert.equal(env.last("submit"), undefined, "the app starts the hand-over, not the view");
});

test("drafts are debounced, coalesced, flushable, and dropped when read-only", () => {
  const env = fakeEnv();
  const plugin = Pinrail.createPlugin(env, { resize: "manual" });
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
  Pinrail.createPlugin(auto, {});
  assert.equal(auto.observers.length, 0);
  auto.deliver(init());
  assert.equal(auto.observers.length, 1);
  assert.deepEqual(auto.last("resize").msg, { pinrail: 1, type: "resize", height: 321 });
  auto.deliver(init());
  assert.equal(auto.observers.length, 1, "a second init does not observe twice");

  const fill = fakeEnv();
  Pinrail.createPlugin(fill, { resize: "fill" });
  fill.deliver(init());
  assert.deepEqual(fill.last("resize").msg, { pinrail: 1, type: "resize", height: "fill" });

  const manual = fakeEnv();
  const plugin = Pinrail.createPlugin(manual, { resize: "manual" });
  manual.deliver(init());
  assert.equal(manual.last("resize"), undefined);
  plugin.resize(500);
  assert.deepEqual(manual.last("resize").msg.height, 500);
});

test("status tells the shell what handing over would do", () => {
  const env = fakeEnv();
  const plugin = Pinrail.createPlugin(env, { resize: "manual" });
  env.deliver(init());

  plugin.status({ label: "Hand over 3 decisions" });
  assert.deepEqual(env.last("status").msg, { pinrail: 1, type: "status", label: "Hand over 3 decisions" });
  assert.equal(env.last("status").target, "http://shell.test");
});

test("the decision onCollect returns answers the app's request, by its number", async () => {
  const env = fakeEnv();
  Pinrail.createPlugin(env, { resize: "manual", onCollect: () => ({ ok: true }) });
  env.deliver(init());
  env.deliver(shell({ type: "collect", req: 7 }));
  await settle();
  assert.deepEqual(env.last("submit"), {
    msg: { pinrail: 1, type: "submit", req: 7, data: { ok: true } },
    target: "http://shell.test",
  });
});

test("a promise of the decision is waited for", async () => {
  const env = fakeEnv();
  let confirm;
  Pinrail.createPlugin(env, {
    resize: "manual",
    onCollect: () => new Promise((resolve) => (confirm = () => resolve({ ok: false }))),
  });
  env.deliver(init());
  env.deliver(shell({ type: "collect", req: 3 }));
  await settle();
  assert.equal(env.last("submit"), undefined);
  confirm();
  await settle();
  assert.deepEqual(env.last("submit").msg, { pinrail: 1, type: "submit", req: 3, data: { ok: false } });
});

test("nothing returned is defer: the view needs more from the person first", async () => {
  const env = fakeEnv();
  const answers = [undefined, null, Promise.resolve(undefined)];
  Pinrail.createPlugin(env, { resize: "manual", onCollect: () => answers.shift() });
  env.deliver(init());
  for (const req of [1, 2, 3]) {
    env.deliver(shell({ type: "collect", req }));
    await settle();
  }
  assert.deepEqual(
    env.posted.map((p) => p.msg).filter((m) => m.type === "defer" || m.type === "submit"),
    [1, 2, 3].map((req) => ({ pinrail: 1, type: "defer", req })),
  );
});

test("a handler that throws, or a decision JSON cannot hold, reaches onError and hands nothing over", async () => {
  const env = fakeEnv();
  const errors = [];
  const loop = { a: 1 };
  loop.self = loop;
  const answers = [
    () => {
      throw new Error("the view broke");
    },
    () => loop,
    () => Promise.reject(new Error("the preview failed")),
  ];
  Pinrail.createPlugin(env, { resize: "manual", onCollect: () => answers.shift()(), onError: (e) => errors.push(e) });
  env.deliver(init());
  for (const req of [1, 2, 3]) {
    env.deliver(shell({ type: "collect", req }));
    await settle();
  }
  assert.equal(env.last("submit"), undefined);
  assert.deepEqual(
    env.types().filter((t) => t === "defer"),
    ["defer", "defer", "defer"],
  );
  assert.equal(errors[0].message, "the view broke");
  assert.match(errors[1].message, /^the decision is not JSON/);
  assert.equal(errors[2].message, "the preview failed");
});

test("without onError, an error goes to the console", async () => {
  const env = fakeEnv();
  Pinrail.createPlugin(env, {
    resize: "manual",
    onCollect: () => {
      throw new Error("unhandled");
    },
  });
  env.deliver(init());
  env.deliver(shell({ type: "collect", req: 1 }));
  await settle();
  assert.equal(env.errors[0].message, "unhandled");
});

test("a read-only view answers defer without asking onCollect", async () => {
  const env = fakeEnv();
  let asked = 0;
  Pinrail.createPlugin(env, { resize: "manual", onCollect: () => (asked++, { ok: true }) });
  env.deliver(init({ readonly: true }));
  env.deliver(shell({ type: "collect", req: 1 }));
  await settle();
  assert.equal(asked, 0);
  assert.deepEqual(env.last("defer").msg, { pinrail: 1, type: "defer", req: 1 });
});

test("a collect without a request number is not the protocol's, and is ignored", async () => {
  const env = fakeEnv();
  let asked = 0;
  Pinrail.createPlugin(env, { resize: "manual", onCollect: () => (asked++, { ok: true }) });
  env.deliver(init());
  env.deliver(shell({ type: "collect" }));
  await settle();
  assert.equal(asked, 0);
  assert.equal(env.last("submit"), undefined);
});

test("the theme comes from the environment first, and the shell can still change it", () => {
  // In a browser this is the theme on the frame's URL, which is the only one
  // that can be in place before the view paints.
  const env = Object.assign(fakeEnv(), { initialTheme: () => "light" });
  const seen = [];
  const plugin = Pinrail.createPlugin(env, { resize: "manual", onAppearance: (t) => seen.push(t) });

  assert.equal(plugin.theme, "light", "in the shell's theme before a single message");
  assert.deepEqual(seen, [], "and without anything to react to");

  env.deliver(shell({ type: "appearance", theme: "dark" }));
  assert.equal(plugin.theme, "dark", "the shell still owns every later change");
  assert.deepEqual(env.themes, ["dark"]);
});

test("an environment with no theme of its own leaves the plugin dark", () => {
  const plugin = Pinrail.createPlugin(fakeEnv(), { resize: "manual" });
  assert.equal(plugin.theme, "dark");
});

test("appearance applies the theme, exposes it, and ignores anything else", () => {
  const env = fakeEnv();
  const seen = [];
  const plugin = Pinrail.createPlugin(env, { resize: "manual", onAppearance: (t) => seen.push(t) });

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
  const plugin = Pinrail.createPlugin(env, { resize: "manual", onInit: () => inits++ });
  env.deliver(init());
  plugin.draft({ n: 1 }, { flush: true });

  env.deliver(shell({ type: "appearance", theme: "light" }));

  assert.equal(inits, 1);
  assert.equal(env.types().filter((t) => t === "draft").length, 1);
  assert.deepEqual(env.last("draft").msg.data, { n: 1 });
  assert.equal(plugin.readonly, false);
});

test("escape, markdown and previousVerdict", () => {
  assert.equal(Pinrail.escape(`<a href="x">&'`), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");

  // The parser is the package's own dependency here and the app's in a
  // browser; what it renders in a view's frame is settled in markdown.spec.ts.
  assert.equal(Pinrail.markdown("# Title"), "<h1>Title</h1>\n");
  assert.equal(Pinrail.markdownInline("a *b*"), "a <em>b</em>");
  // raw HTML is escaped: a view's frame runs inline scripts
  assert.equal(Pinrail.markdown("<script>alert(1)</script>"), "<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>\n");
  // and an address a click would run is not made a link at all
  assert.equal(Pinrail.markdown("[x](javascript:alert(1))"), "<p>[x](javascript:alert(1))</p>\n");

  const previous = { decision: { data: { decisions: [{ id: 1, action: "reject", note: "no" }], undecided: [2] } } };
  assert.deepEqual(Pinrail.previousVerdict(previous, 1), { action: "reject", note: "no" });
  assert.deepEqual(Pinrail.previousVerdict(previous, 2), { action: "undecided", note: "" });
  assert.equal(Pinrail.previousVerdict(previous, 3), null);
  assert.equal(Pinrail.previousVerdict(null, 1), null);
});

test("layout builds a body on its own, and a header when asked for one", () => {
  const bare = fakeDocument();
  const plain = Pinrail.layout({ document: bare });
  assert.equal(plain.header, null, "no header unless the view wants one");
  assert.equal(bare.body.className, "plugin-layout");
  assert.deepEqual(
    bare.body.children.map((n) => n.className),
    ["plugin-scroll"],
  );
  assert.deepEqual(plain.scroll.children, [plain.content], "the body scrolls, the document does not");

  const doc = fakeDocument();
  const view = Pinrail.layout({ document: doc, title: "5 items" });
  assert.deepEqual(
    doc.body.children.map((n) => n.className),
    ["plugin-header", "plugin-scroll"],
  );
  assert.deepEqual(
    view.header.children.map((n) => n.className),
    ["plugin-title", "plugin-meta", "plugin-controls"],
  );
  assert.equal(view.header.children[0].textContent, "5 items");
});

test("layout takes strings or elements, and replaces rather than appends", () => {
  const doc = fakeDocument();
  const button = doc.createElement("button");
  const view = Pinrail.layout({ document: doc, meta: ["acme-api", "7 days"], controls: button });

  const [, meta, controls] = view.header.children;
  assert.deepEqual(
    meta.children.map((n) => n.textContent),
    ["acme-api", "7 days"],
  );
  assert.deepEqual(controls.children, [button]);

  assert.equal(view.title("4 items").meta("acme-worker"), view, "setters chain");
  assert.equal(view.header.children[0].textContent, "4 items");
  assert.deepEqual(
    meta.children.map((n) => n.textContent),
    ["acme-worker"],
  );

  view.meta(null);
  assert.deepEqual(meta.children, []);
});

test("layout can be put somewhere other than the body", () => {
  const doc = fakeDocument();
  const host = doc.createElement("div");
  const view = Pinrail.layout({ document: doc, into: host, header: true });

  assert.deepEqual(doc.body.children, []);
  assert.deepEqual(
    host.children.map((n) => n.className),
    ["plugin-header", "plugin-scroll"],
  );
  assert.equal(host.children[1], view.scroll);
  assert.equal(view.scroll.children[0], view.content);
});

test("the module exposes a version and the protocol number", () => {
  assert.equal(Pinrail.protocol, 1);
  assert.match(Pinrail.version, /^\d+\.\d+\.\d+$/);
});

test("icon markup takes the name, the colour of its text, and nothing from a payload", () => {
  const plain = Pinrail.icon("check");
  assert.match(plain, /class="wi"/);
  assert.match(
    plain,
    /--wi:url\(&quot;http:\/\/plugin\.invalid\/view\/icons\/check\.svg&quot;\)/,
    "the plugin's own, beside the view",
  );
  assert.match(plain, /aria-hidden="true"/, "decorative unless it is given a name");
  assert.match(plain, /data-icon="check"/, "the name stays on the element, to find a typo by");

  // A view may take the name from a review payload, which is not ours to trust.
  const hostile = Pinrail.icon("x.svg) url(https://evil.test/pixel.svg");
  assert.match(hostile, /--wi:url\(&quot;http:\/\/plugin\.invalid\/view\/icons\/[a-z0-9-]*\.svg&quot;\);/);
  assert.equal(hostile.includes("evil.test"), false, "the host is gone");
  assert.equal(
    /url\(/.test(hostile.replace("url(&quot;http://plugin.invalid/view/icons/", "")),
    false,
    "no second url()",
  );

  assert.match(Pinrail.icon("check", { size: 18 }), /--wi-size:18px/);
  assert.match(Pinrail.icon("check", { size: "1.25em" }), /--wi-size:1\.25em/);
  // a size taken from a payload cannot leave the style attribute
  const breakout = Pinrail.icon("check", { size: '1px" onmouseover="alert(1)' });
  assert.doesNotMatch(breakout, /onmouseover/);
  assert.doesNotMatch(breakout, /--wi-size/);
  assert.match(Pinrail.icon("check", { class: "spacer" }), /class="wi spacer"/);

  const named = Pinrail.icon("trash-2", { label: "delete" });
  assert.match(named, /role="img"/);
  assert.match(named, /aria-label="delete"/);
  assert.equal(named.includes("aria-hidden"), false);

  assert.match(Pinrail.icon(null), /data-icon=""/, "a missing name is not a crash");
});

test("settings arrive with init and again as a message; setSetting asks the shell", () => {
  const env = fakeEnv();
  const seen = [];
  const plugin = Pinrail.createPlugin(env, {
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
  assert.deepEqual(env.last("settings_set").msg, { pinrail: 1, type: "settings_set", patch: { diff: "split" } });

  // a shell that says nothing, or nonsense, about settings leaves them empty
  env.deliver(init());
  assert.deepEqual(plugin.settings, {});
  env.deliver(shell({ type: "settings", settings: [1, 2] }));
  assert.deepEqual(plugin.settings, {});
});

test("a forwarded key lands on the document; junk is ignored", () => {
  const env = fakeEnv();
  Pinrail.createPlugin(env, { resize: "manual" });
  env.deliver(init());
  env.deliver(shell({ type: "key", key: "j", code: "KeyJ" }));
  env.deliver(shell({ type: "key", key: "M", code: "KeyM", metaKey: true, shiftKey: true }));
  env.deliver(shell({ type: "key" }));
  const expected = [
    { key: "j", code: "KeyJ", metaKey: false, ctrlKey: false, altKey: false, shiftKey: false },
    { key: "M", code: "KeyM", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true },
  ];
  assert.deepEqual(env.keys, expected);
});

test("the SDK announces the package's version", () => {
  const pkg = require("../package.json");
  assert.equal(Pinrail.version, pkg.version);
  assert.equal(Pinrail.protocol, 1);
  assert.equal(pkg.version.split(".")[0], String(Pinrail.protocol));
});

test("a link is the shell's to open, and only where a view may send someone", () => {
  const env = fakeEnv();
  const plugin = Pinrail.createPlugin(env, { resize: "manual" });
  env.deliver(init());

  // a click in the frame: sandboxed without popups, it opens nothing itself
  env.clickLink("https://example.com/docs");
  assert.deepEqual(env.last("open").msg, { pinrail: 1, type: "open", url: "https://example.com/docs" });
  assert.equal(env.last("open").target, "http://shell.test", "and only to the shell");

  // the same from the view's own code
  plugin.open("mailto:x@example.com");
  assert.equal(env.last("open").msg.url, "mailto:x@example.com");

  // an address that would run something, or reach the machine, is not sent
  plugin.open("javascript:alert(1)");
  plugin.open("file:///etc/passwd");
  assert.equal(env.types().filter((t) => t === "open").length, 2);
});

test("attachment asks the shell for a file the review lists, and resolves with the bytes it answers", async () => {
  const env = fakeEnv();
  const plugin = Pinrail.createPlugin(env, { resize: "manual" });
  const files = [{ name: "pivot.glb", size: 3, media_type: "model/gltf-binary", sha256: "ab" }];
  env.deliver(
    init({
      review: review({ attachments: files }),
      previous: review({
        id: "g_0",
        attachments: [{ name: "old.glb", size: 1, media_type: "model/gltf-binary", sha256: "cd" }],
      }),
      capabilities: ["attachments"],
    }),
  );
  assert.deepEqual(plugin.attachments, files);

  const asked = plugin.attachment("pivot.glb");
  assert.deepEqual(env.last("attachment"), {
    msg: { pinrail: 1, type: "attachment", req: 1, name: "pivot.glb" },
    target: "http://shell.test",
  });
  const bytes = new Uint8Array([1, 2, 3]).buffer;
  env.deliver(shell({ type: "attachment", req: 1, ok: true, name: "pivot.glb", bytes }));
  assert.equal(await asked, bytes);

  // a file of the round this one revises
  const old = plugin.attachment("old.glb", { round: "previous" });
  assert.deepEqual(env.last("attachment").msg, {
    pinrail: 1,
    type: "attachment",
    req: 2,
    name: "old.glb",
    round: "previous",
  });
  env.deliver(shell({ type: "attachment", req: 2, ok: false, error: "gone" }));
  await assert.rejects(old, /gone/);

  // as a blob: URL, typed as the review lists it
  const url = plugin.attachmentUrl("pivot.glb");
  env.deliver(shell({ type: "attachment", req: 3, ok: true, name: "pivot.glb", bytes }));
  assert.equal(await url, "blob:test/model/gltf-binary/3");
});

test("attachment refuses a name the review does not list, and a shell that cannot hand files over", async () => {
  const env = fakeEnv();
  const plugin = Pinrail.createPlugin(env, { resize: "manual" });
  env.deliver(
    init({
      review: review({ attachments: [{ name: "a.glb", size: 1, media_type: "x/y", sha256: "ab" }] }),
      capabilities: ["attachments"],
    }),
  );
  await assert.rejects(plugin.attachment("b.glb"), /no attachment "b.glb" on this review/);
  assert.equal(env.last("attachment"), undefined, "nothing was asked");

  const old = fakeEnv();
  const older = Pinrail.createPlugin(old, { resize: "manual" });
  old.deliver(init({ review: review({ attachments: [{ name: "a.glb", size: 1, media_type: "x/y", sha256: "ab" }] }) }));
  await assert.rejects(older.attachment("a.glb"), /cannot hand files to a view; update the app/);
});

test("attachmentName reads a reference, and ATTACHMENT_SCHEMA describes one", () => {
  assert.equal(Pinrail.attachmentName({ $attachment: "pivot.glb" }), "pivot.glb");
  for (const not of [null, "attachment:pivot.glb", { $attachment: 7 }, {}])
    assert.equal(Pinrail.attachmentName(not), null);
  assert.deepEqual(Pinrail.ATTACHMENT_SCHEMA.required, ["$attachment"]);
  assert.equal(Object.isFrozen(Pinrail.ATTACHMENT_SCHEMA), true);
});

test("the script a view loads names no source map, which nothing serves", () => {
  const path = require("node:path");
  const { sdkScript } = require("../lib/paths.cjs");
  const script = sdkScript(path.resolve(__dirname, ".."));
  assert.ok(script.includes("markdownit"), "the parser is in it");
  assert.ok(!/sourceMappingURL/.test(script), "no source map comment");
});

test("the app's own keys, pressed in the view, go up to the app", () => {
  const env = fakeEnv();
  Pinrail.createPlugin(env, {});
  env.pressAppKey({ key: "?", code: "Slash", metaKey: false, ctrlKey: false, altKey: false, shiftKey: true });
  assert.deepEqual(env.last("key").msg, {
    pinrail: 1,
    type: "key",
    key: "?",
    code: "Slash",
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: true,
  });
});

test("a draft still pending when the decision is handed over is never sent", async () => {
  const env = fakeEnv();
  const plugin = Pinrail.createPlugin(env, { resize: "manual", onCollect: () => ({ ok: true }) });
  env.deliver(init());
  plugin.draft({ note: "half typed" });
  env.deliver(shell({ type: "collect", req: 1 }));
  await settle();
  for (const timer of env.timers.splice(0)) timer.fn();
  const types = env.posted.map((p) => p.msg.type);
  assert.equal(types.at(-1), "submit", types.join(", "));
  assert.ok(!types.slice(types.indexOf("submit")).includes("draft"), types.join(", "));
});

test("a decision or draft held in reactive state is sent as the plain data it holds", async () => {
  const env = fakeEnv();
  // what a browser does with every message: a structured clone, which
  // refuses a Proxy such as Vue's reactive() or Svelte's $state
  env.post = (msg, target) => env.posted.push({ msg: structuredClone(msg), target });
  const reactive = (value) => new Proxy(value, {});
  const plugin = Pinrail.createPlugin(env, {
    resize: "manual",
    onCollect: () => reactive({ ok: true, items: reactive([1, 2]), skipped: undefined }),
  });
  env.deliver(init());
  env.deliver(shell({ type: "collect", req: 1 }));
  await settle();
  plugin.draft(reactive({ step: 2 }), { flush: true });
  const sent = env.posted.map((p) => p.msg).filter((m) => m.type === "submit" || m.type === "draft");
  assert.deepEqual(sent, [
    { pinrail: sent[0].pinrail, type: "submit", req: 1, data: { ok: true, items: [1, 2] } },
    { pinrail: sent[0].pinrail, type: "draft", data: { step: 2 } },
  ]);
});

// ---------------------------------------------------------------- step 3's defects

test("a draft of false, 0 or an empty string comes back as it was", () => {
  for (const kept of [false, 0, ""]) {
    const env = fakeEnv();
    const seen = [];
    Pinrail.createPlugin(env, { resize: "manual", onInit: (i) => seen.push(i.draft) });
    env.deliver(init({ draft: kept }));
    assert.deepEqual(seen, [kept]);
  }
});

test("a forwarded key reaches the view once, as a keydown", () => {
  const env = fakeEnv();
  const handled = [];
  Pinrail.createPlugin(env, { resize: "manual", onKey: (k) => handled.push(k.key) });
  env.deliver(init());
  env.deliver(shell({ type: "key", key: "j", code: "KeyJ" }));
  assert.equal(env.keys.length, 1);
  assert.deepEqual(handled, [], "onKey is gone: the keydown is the one way");
});

test("a handler that throws in onInit does not stop the client", () => {
  const env = fakeEnv();
  const errors = [];
  Pinrail.createPlugin(env, {
    onInit() {
      throw new Error("the view broke");
    },
    onError: (e) => errors.push(e.message),
  });
  env.deliver(init());
  // the client still starts reporting the view's size
  assert.equal(env.observers.length, 1);
  assert.deepEqual(errors, ["the view broke"]);
});

test("submitted replaces the review the view holds, and leaves the old object as it was", () => {
  const env = fakeEnv();
  const plugin = Pinrail.createPlugin(env, { resize: "manual" });
  env.deliver(init());
  const held = plugin.review;
  env.deliver(shell({ type: "submitted", decision: { decided_by: "a", data: { ok: true } } }));
  assert.notEqual(plugin.review, held);
  assert.equal(held.status, "pending");
  assert.equal(held.decision, null);
  assert.equal(plugin.review.status, "decided");
  assert.deepEqual(plugin.review.decision.data, { ok: true });
});

test("the teardown leaves no listener or timer of the client behind", () => {
  const env = fakeEnv();
  const plugin = Pinrail.createPlugin(env, { onInit() {} });
  env.deliver(init());
  plugin.draft({ step: 1 });
  assert.ok(env.active() > 0);
  plugin[Symbol.for("pinrail.teardown")]();
  assert.equal(env.active(), 0);
  assert.equal(env.timers.length, 0);
});
