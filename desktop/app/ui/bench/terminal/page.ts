// The bench page. It loads one of the window's renderers from the app's
// registry, runs one case against the fake core and posts the measurements
// back:
//
//   /page.html?adapter=ghostty&case=burst&item=synthetic/tui.bin&run=ID
//
// Cases: load, burst, stream, replay, idle, input, attach, memory, switch,
// agents and screen, which README.md describes, and live for the checks.

import { RENDERERS } from "../../src/components/terminal/renderers";
import type { BenchHooks, TerminalRenderer, TerminalView, ViewOptions } from "../../src/components/terminal/view";

/** Renderers measured beside the app's for comparison, which the app does
 *  not register. */
const COMPARISONS: Record<string, () => Promise<TerminalRenderer>> = {
  "ghostty-web": () => import("./adapters/ghostty-web").then((m) => m.ghosttyWebRenderer),
};

/** The hooks a renderer offers the benchmark, where it has them. */
const benchHooks = (view: TerminalView) => view as Partial<BenchHooks>;

const q = new URLSearchParams(location.search);
const ADAPTER = q.get("adapter") ?? "ghostty";
const CASE = q.get("case") ?? "load";
const ITEM = q.get("item") ?? "synthetic/tui.bin";
const RUN = q.get("run") ?? "manual";
/** The fake core: where the page came from, or, for the page the app's
 *  window loads from Vite, the `core` the runner names. */
const CORE = new URL(q.get("core") ?? location.origin);
const coreUrl = (path: string) => new URL(path, CORE).href;
const socketUrl = (path: string) => `ws://${CORE.host}${path}`;
const num = (k: string, d: number) => (q.has(k) ? Number(q.get(k)) : d);

const OPTIONS: ViewOptions = {
  cols: 120,
  rows: 40,
  scrollback: 5000,
  font: { family: "Menlo, 'DejaVu Sans Mono', monospace", size: 13 },
  theme: {
    foreground: "#d4d4d4",
    background: "#1e1e1e",
    cursor: "#d4d4d4",
    selectionBackground: "#264f78",
  },
};

// Violations of the page's Content-Security-Policy (core/csp.mjs), reported
// with the result so that a candidate's needs show in every run.
const cspViolations: string[] = [];
/** Whether a policy is enforced at all: one without 'unsafe-eval', as the
 *  app's, blocks this. Its own violation is left out of the list. */
const cspEnforced = (() => {
  try {
    new Function("return 1")();
    return false;
  } catch {
    return true;
  }
})();
document.addEventListener("securitypolicyviolation", (e) => {
  if (e.blockedURI === "eval") return;
  const v = `${e.effectiveDirective} blocked ${e.blockedURI || "inline"}${e.sourceFile ? ` (${e.sourceFile.split("/").pop()}:${e.lineNumber})` : ""}`;
  if (!cspViolations.includes(v)) cspViolations.push(v);
});

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const raf = () => new Promise<number>((r) => requestAnimationFrame(r));
const log = (...a: unknown[]) => console.log("[bench]", ...a);

// Counts animation-frame callbacks requested by anything but the bench, for
// the idle probe of renderers without a frame hook.
let foreignRafs = 0;
let benchRaf = false;
const nativeRaf = window.requestAnimationFrame.bind(window);
window.requestAnimationFrame = (cb) => {
  if (!benchRaf) foreignRafs++;
  return nativeRaf(cb);
};
function ownRaf(): Promise<number> {
  benchRaf = true;
  try {
    return raf();
  } finally {
    benchRaf = false;
  }
}

// ---------------------------------------------------------------- fake core

async function api(
  path: string,
  body?: unknown,
): Promise<Record<string, unknown> & { rssMiB?: number; path?: string }> {
  const r = await fetch(coreUrl(`/api/${path}${path.includes("?") ? "&" : "?"}run=${RUN}`), {
    method: body === undefined ? "GET" : "POST",
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return r.json();
}

/** Process memory as the runner sees it, in MiB, or null. */
async function sampleRss(label: string): Promise<number | null> {
  const r = await api(`sample?label=${encodeURIComponent(label)}`);
  return r.rssMiB ?? null;
}

interface PlayOptions {
  item: string;
  /** max: as fast as credit allows; rate: bytes per second; realtime: recorded timing */
  mode: "max" | "rate" | "realtime";
  rate?: number;
  seconds?: number;
  maxBytes?: number;
  gapCap?: number;
}

/** Plays a corpus item into a view through the WebSocket path, with credit. */
function play(
  view: TerminalView,
  opts: PlayOptions,
): Promise<{ bytes: number; firstByteMs: number; consumedMs: number }> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(socketUrl("/ws/replay"));
    ws.binaryType = "arraybuffer";
    let total = -1;
    let consumed = 0;
    let pendingCredit = 0;
    let firstByteMs = 0;
    const t0 = performance.now();
    const check = () => {
      if (total >= 0 && consumed >= total) {
        ws.close();
        resolve({ bytes: total, firstByteMs, consumedMs: performance.now() - t0 });
      }
    };
    ws.onopen = () => ws.send(JSON.stringify({ t: "play", ...opts }));
    ws.onerror = () => reject(new Error("replay socket failed"));
    ws.onmessage = (m) => {
      if (typeof m.data === "string") {
        const msg = JSON.parse(m.data);
        if (msg.t === "end") {
          total = msg.bytes;
          check();
        } else if (msg.t === "resize") view.resize(msg.cols, msg.rows);
        else if (msg.t === "error") reject(new Error(msg.message));
        return;
      }
      const bytes = new Uint8Array(m.data as ArrayBuffer);
      if (!firstByteMs) firstByteMs = performance.now() - t0;
      view.write(bytes, () => {
        parsed(view);
        consumed += bytes.length;
        pendingCredit += bytes.length;
        if (pendingCredit >= 64 * 1024 || (total >= 0 && consumed >= total)) {
          if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ t: "credit", n: pendingCredit }));
          pendingCredit = 0;
        }
        check();
      });
    };
  });
}

/** The attach path: asks the core for a snapshot of a terminal with history. */
function fetchSnapshot(name: string, formats: readonly string[]): Promise<{ format: string; bytes: Uint8Array }> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(socketUrl("/ws/replay"));
    ws.binaryType = "arraybuffer";
    let format = "";
    ws.onopen = () => ws.send(JSON.stringify({ t: "attach", snapshot: name, formats }));
    ws.onerror = () => reject(new Error("attach socket failed"));
    ws.onmessage = (m) => {
      if (typeof m.data === "string") {
        const msg = JSON.parse(m.data);
        if (msg.t === "snapshot") format = msg.format;
        else if (msg.t === "error") reject(new Error(msg.message));
        return;
      }
      ws.close();
      resolve({ format, bytes: new Uint8Array(m.data as ArrayBuffer) });
    };
  });
}

/** The snapshot formats to ask for: the view's, or the one named by `&format=` (e.g. vt). */
function formatsFor(view: TerminalView): readonly string[] {
  const forced = q.get("format");
  return forced ? [forced] : view.snapshotFormats;
}

async function fetchItem(item: string): Promise<Uint8Array> {
  return new Uint8Array(await (await fetch(coreUrl(`/corpus/${item}`))).arrayBuffer());
}

// ---------------------------------------------------------------- probes

function stats(values: number[]) {
  const d = [...values].sort((a, b) => a - b);
  const pct = (p: number) => (d.length ? d[Math.min(d.length - 1, Math.ceil(d.length * p) - 1)]! : 0);
  return {
    n: d.length,
    p50: pct(0.5),
    p95: pct(0.95),
    p99: pct(0.99),
    max: d.length ? d[d.length - 1]! : 0,
    mean: d.length ? d.reduce((a, b) => a + b, 0) / d.length : 0,
  };
}

/** Frame times from animation-frame deltas, plus long tasks and long animation frames where the browser reports them. */
function frameRecorder(view?: TerminalView) {
  const deltas: number[] = [];
  let last = performance.now();
  let on = true;
  let drawn = 0;
  const loop = (t: number) => {
    deltas.push(t - last);
    last = t;
    if (on) {
      benchRaf = true;
      requestAnimationFrame(loop);
      benchRaf = false;
    }
  };
  benchRaf = true;
  requestAnimationFrame(loop);
  benchRaf = false;
  const longTasks: number[] = [];
  const loafs: number[] = [];
  const observers: PerformanceObserver[] = [];
  const supported = PerformanceObserver.supportedEntryTypes ?? [];
  for (const [type, sink] of [
    ["longtask", longTasks],
    ["long-animation-frame", loafs],
  ] as const) {
    if (!supported.includes(type)) continue;
    const o = new PerformanceObserver((l) => l.getEntries().forEach((e) => sink.push(e.duration)));
    o.observe({ type, buffered: false } as PerformanceObserverInit);
    observers.push(o);
  }
  const unhook = view ? benchHooks(view).onFrameDrawn?.(() => drawn++) : undefined;
  return () => {
    on = false;
    observers.forEach((o) => o.disconnect());
    unhook?.();
    const s = stats(deltas);
    return {
      frames: s.n,
      drawnFrames: unhook ? drawn : null,
      p50: s.p50,
      p95: s.p95,
      max: s.max,
      over16: deltas.filter((x) => x > 16.7 * 1.5).length,
      over33: deltas.filter((x) => x > 33.4).length,
      over50: deltas.filter((x) => x > 50).length,
      longTasks: supported.includes("longtask")
        ? { n: longTasks.length, max: Math.max(0, ...longTasks), over50: longTasks.filter((x) => x > 50).length }
        : null,
      longFrames: supported.includes("long-animation-frame") ? { n: loafs.length, max: Math.max(0, ...loafs) } : null,
    };
  };
}

/** When each view drew, and how many draws it had made when its output
 *  was last parsed. A renderer may draw in the same task that parses, so
 *  a probe that waited for the next draw after the parse would miss it. */
const draws = new WeakMap<TerminalView, { times: number[]; atParse: number }>();

function logDraws(view: TerminalView) {
  const log = { times: [] as number[], atParse: 0 };
  benchHooks(view).onFrameDrawn?.(() => log.times.push(performance.now()));
  draws.set(view, log);
}

/** Notes that the view parsed output, for drawnAfterParse. */
function parsed(view: TerminalView) {
  const log = draws.get(view);
  if (log) log.atParse = log.times.length;
}

/** The time of the view's first draw after its output was last parsed,
 *  whether it came before this call or after. */
function drawnAfterParse(view: TerminalView, timeoutMs = 2000): Promise<number> {
  const log = draws.get(view);
  if (log && log.times.length > log.atParse) return Promise.resolve(log.times[log.atParse]);
  return nextDrawn(view, timeoutMs);
}

/** Resolves after the view has drawn a frame (or, without a hook, after the next frame). */
function nextDrawn(view: TerminalView, timeoutMs = 2000): Promise<number> {
  const hook = benchHooks(view).onFrameDrawn;
  if (!hook)
    return ownRaf()
      .then(() => ownRaf())
      .then(() => performance.now());
  return new Promise((resolve) => {
    let done = false;
    const off = hook.call(view, () => {
      if (done) return;
      done = true;
      off();
      resolve(performance.now());
    });
    setTimeout(() => {
      if (done) return;
      done = true;
      off();
      resolve(performance.now());
    }, timeoutMs);
  });
}

function jsHeapMiB(): number | null {
  const m = (performance as unknown as { memory?: { usedJSHeapSize: number } }).memory;
  return m ? m.usedJSHeapSize / 1048576 : null;
}

function gc() {
  (window as unknown as { gc?: () => void }).gc?.();
}

// ---------------------------------------------------------------- views

let renderer: TerminalRenderer;

function hostElement(id: string): HTMLElement {
  const el = document.getElementById(id)!;
  el.style.display = "block";
  return el;
}

// The most recent view, whose backend goes in the results.
let lastView: TerminalView | null = null;

async function newView(hostId = "main"): Promise<TerminalView> {
  const view = renderer.create();
  await view.mount(hostElement(hostId), OPTIONS);
  lastView = view;
  logDraws(view);
  (window as unknown as { __view: TerminalView }).__view = view;
  return view;
}

// ---------------------------------------------------------------- cases

const cases: Record<string, () => Promise<Record<string, unknown>>> = {
  // Load: code and wasm fetched and compiled, a view mounted, first frame.
  async load() {
    const tLoad0 = performance.now();
    const view = await newView();
    const tMounted = performance.now();
    const drawn = nextDrawn(view);
    view.write(new TextEncoder().encode("Pinrail terminal bench\r\n$ "), () => {});
    const tFirst = await drawn;
    const resources = performance.getEntriesByType("resource") as PerformanceResourceTiming[];
    const bytes = resources
      .filter((r) => /\/dist\/|\/ghostty\//.test(r.name))
      .reduce((a, r) => a + (r.decodedBodySize || r.transferSize || 0), 0);
    return {
      loadMs,
      loadedSinceNavigationMs: loadedAt,
      mountMs: tMounted - tLoad0,
      firstFrameSinceNavigationMs: tFirst,
      codeBytes: bytes + pageBytes(),
    };
  },

  // Burst: a whole item as fast as credit allows, until drawn.
  async burst() {
    const view = await newView();
    await nextDrawn(view, 300);
    const stop = frameRecorder(view);
    const t0 = performance.now();
    const r = await play(view, { item: ITEM, mode: "max", maxBytes: num("maxBytes", 0) || undefined });
    const totalMs = (await drawnAfterParse(view)) - t0;
    await ownRaf();
    const frames = stop();
    return { bytes: r.bytes, totalMs, mbPerS: r.bytes / totalMs / 1000, parsedMs: r.consumedMs, frames };
  },

  // Stream: an item paced at a rate (2 MB/s by default) for some seconds.
  async stream() {
    const view = await newView();
    await nextDrawn(view, 300);
    const stop = frameRecorder(view);
    const rate = num("rate", 2e6);
    const seconds = num("seconds", 5);
    const r = await play(view, { item: ITEM, mode: "rate", rate, seconds });
    await drawnAfterParse(view);
    return { rate, seconds, bytes: r.bytes, frames: stop() };
  },

  // Replay: a recording at its recorded timing, with long pauses shortened.
  async replay() {
    const view = await newView();
    await nextDrawn(view, 300);
    const stop = frameRecorder(view);
    const t0 = performance.now();
    const r = await play(view, { item: ITEM, mode: "realtime", gapCap: num("gapCap", 0.5) });
    await drawnAfterParse(view);
    const realtime = { ms: performance.now() - t0, frames: stop() };
    // The same recording as fast as possible.
    view.reset();
    const t1 = performance.now();
    const m = await play(view, { item: ITEM, mode: "max" });
    await drawnAfterParse(view);
    return { bytes: r.bytes, realtime, maxSpeedMs: performance.now() - t1, maxSpeedBytes: m.bytes };
  },

  // Idle: after output stops, frames drawn while nothing changes.
  async idle() {
    const view = await newView();
    await play(view, { item: ITEM, mode: "rate", rate: 2e6, seconds: 1 });
    // The stream may stop inside a synchronized update; xterm.js draws it
    // after its 1 s timeout, which is not idle drawing.
    await sleep(1500);
    let drawn = 0;
    const off = benchHooks(view).onFrameDrawn?.(() => drawn++);
    const rafs0 = foreignRafs;
    const seconds = 3;
    await sleep(seconds * 1000);
    off?.();
    return {
      drawnPerSecond: off ? drawn / seconds : null,
      animationFrameRequestsPerSecond: (foreignRafs - rafs0) / seconds,
    };
  },

  // Input: synthetic keys through the view's encoder, the WebSocket and a PTY
  // echo program, until the echo is drawn. With load=1, a second terminal
  // streams agent output at 2 MB/s meanwhile.
  async input() {
    const view = await newView();
    const loadView = q.get("load") === "1" ? await newView("second") : null;
    const ws = new WebSocket(socketUrl("/ws/pty"));
    ws.binaryType = "arraybuffer";
    await new Promise<void>((r, j) => {
      ws.onopen = () => r();
      ws.onerror = () => j(new Error("pty socket failed"));
    });
    // Wait for the echo program to be ready.
    await new Promise<void>((r) => {
      ws.onmessage = (m) => {
        if (typeof m.data === "string" && JSON.parse(m.data).t === "ready") r();
      };
    });
    let tKey: number;
    let tInput = 0;
    let tEcho = 0;
    let pending: ((t: number) => void) | null = null;
    view.onInput((bytes) => {
      tInput = performance.now();
      ws.send(bytes);
    });
    ws.onmessage = (m) => {
      if (typeof m.data === "string") return;
      tEcho = performance.now();
      const bytes = new Uint8Array(m.data as ArrayBuffer);
      const drawn = nextDrawn(view, 500);
      view.write(bytes, () => {
        drawn.then((t) => pending?.(t));
      });
    };
    view.focus();
    await nextDrawn(view, 300);
    const stopLoad = loadView ? startLoad(loadView) : null;
    if (loadView) await sleep(500);
    const target = benchHooks(view).inputElement?.() ?? document.activeElement ?? document.body;
    const total: number[] = [];
    const encode: number[] = [];
    const roundTrip: number[] = [];
    const draw: number[] = [];
    const n = num("keys", 60);
    for (let i = 0; i < n; i++) {
      const ch = "abcdefghijklmnopqrstuvwxyz"[i % 26]!;
      const done = new Promise<number>((r) => (pending = r));
      tKey = performance.now();
      dispatchKey(target as HTMLElement, ch);
      const t1 = await Promise.race([done, sleep(1000).then(() => -1)]);
      pending = null;
      if (t1 > 0) {
        total.push(t1 - tKey);
        encode.push(tInput - tKey);
        roundTrip.push(tEcho - tInput);
        draw.push(t1 - tEcho);
      }
      await sleep(30 + Math.random() * 30);
    }
    const loadInfo = stopLoad ? await stopLoad() : null;
    ws.close();
    return {
      keys: n,
      echoed: total.length,
      latency: stats(total),
      keyToInput: stats(encode),
      wsAndPtyRoundTrip: stats(roundTrip),
      echoToDrawn: stats(draw),
      background: loadInfo,
    };
  },

  // Attach: a new view restores a snapshot of a terminal with 5,000 lines of
  // history, until its first frame is drawn.
  async attach() {
    const view = await newView();
    await nextDrawn(view, 300);
    const formats = formatsFor(view);
    const t0 = performance.now();
    const snap = await fetchSnapshot(q.get("snapshot") ?? "history-5000", formats);
    const t1 = performance.now();
    const drawn = nextDrawn(view);
    await view.restore(snap.format, snap.bytes);
    const t2 = await drawn;
    return {
      format: snap.format,
      bytes: snap.bytes.length,
      attachMs: t2 - t0,
      transferMs: t1 - t0,
      restoreToDrawnMs: t2 - t1,
    };
  },

  // Memory: three views each holding 5,000 lines of history.
  async memory() {
    const n = 3;
    await sleep(300);
    gc();
    const before = { rss: await sampleRss("before"), heap: jsHeapMiB(), wasm: 0 };
    const views: TerminalView[] = [];
    for (let i = 0; i < n; i++) {
      const view = await newView(`pool${i}`);
      const snap = await fetchSnapshot("history-5000", formatsFor(view));
      const drawn = nextDrawn(view);
      await view.restore(snap.format, snap.bytes);
      await drawn;
      views.push(view);
    }
    await sleep(500);
    gc();
    // each view has its own instance of the library, with its own memory
    const wasm = views.reduce((sum, v) => sum + (benchHooks(v).wasmMemoryBytes?.() ?? 0), 0) / 1048576;
    const after = { rss: await sampleRss("after"), heap: jsHeapMiB(), wasm };
    for (const v of views) v.dispose();
    await sleep(500);
    gc();
    const disposed = { rss: await sampleRss("disposed"), heap: jsHeapMiB() };
    const per = (a: number | null, b: number | null) => (a !== null && b !== null ? (a - b) / n : null);
    return {
      terminals: n,
      perTerminalRssMiB: per(after.rss, before.rss),
      perTerminalHeapMiB: per(after.heap, before.heap),
      wasmMemoryMiB: wasm ? wasm / n : null,
      before,
      after,
      disposed,
    };
  },

  // Switch: a pooled view, hidden while output arrives, shown again.
  async switch() {
    const view = await newView();
    const snap = await fetchSnapshot("history-5000", formatsFor(view));
    await view.restore(snap.format, snap.bytes);
    await nextDrawn(view);
    const host = document.getElementById("main")!;
    // About 256 KB of agent output, cut after a synchronized update ends.
    const tui = await fetchItem("synthetic/tui.bin");
    const syncEnd = new TextEncoder().encode("\x1b[?2026l");
    let cut = 256 * 1024;
    while (cut < tui.length && !syncEnd.every((b, i) => tui[cut - syncEnd.length + i] === b)) cut++;
    const chunk = tui.subarray(0, cut);
    // A renderer may keep drawing while hidden (xterm.js does) or wait until
    // shown (the ghostty adapter does). The switch ends when the browser has
    // painted the shown view and, if output arrived since its last frame, the
    // view has drawn it.
    // Frames are counted rather than timed: WebKit rounds performance.now()
    // to 1 ms, and a worker renderer reports the frame of the new bytes in
    // the same millisecond as their parse completion.
    let drawnCount = 0;
    benchHooks(view).onFrameDrawn?.(() => drawnCount++);
    const times: number[] = [];
    let drewWhileHidden = 0;
    let staleAfterShow = 0;
    for (let i = 0; i < 5; i++) {
      host.style.display = "none";
      // The view is idle here, so any frame from now on draws the new bytes.
      const before = drawnCount;
      await new Promise<void>((r) => view.write(chunk, r));
      await sleep(300);
      const current = drawnCount > before;
      if (current) drewWhileHidden++;
      const t0 = performance.now();
      // Without a frame within 1 s, the view shows stale output: counted as 1 s.
      const drawn = current ? Promise.resolve(0) : nextDrawn(view, 1000);
      host.style.display = "block";
      await ownRaf();
      await ownRaf();
      const painted = performance.now();
      const t = Math.max(painted, await drawn) - t0;
      if (t >= 1000) staleAfterShow++;
      times.push(Math.min(t, 1000));
      await sleep(200);
    }
    return { switchMs: stats(times), drewWhileHidden, staleAfterShow };
  },

  // Agents: several terminals stream agent output at once, as agents in
  // several sessions do, while only one is shown. The others are kept as
  // the window's pool keeps them: mounted, then taken out of the page.
  // With titles=1, each agent only spins its title 10 times a second, as
  // an agent waiting on a long tool call does.
  async agents() {
    const n = num("agents", 8);
    const titles = q.get("titles") === "1";
    const rate = num("rate", 50_000);
    const seconds = num("seconds", 10);
    const views: TerminalView[] = [];
    for (let i = 0; i < n; i++) {
      const host = i === 0 ? hostElement("main") : document.body.appendChild(document.createElement("div"));
      host.className = "host";
      host.style.display = "block";
      const view = renderer.create();
      await view.mount(host, OPTIONS);
      if (i > 0) host.remove();
      views.push(view);
    }
    lastView = views[0]!;
    await nextDrawn(views[0]!, 300);
    const drawn = views.map(() => 0);
    const offs = views.map((v, i) => benchHooks(v).onFrameDrawn?.(() => drawn[i]!++));
    const stop = frameRecorder();
    const drawMs = () => views.reduce((sum, v) => sum + (benchHooks(v).drawMs?.() ?? 0), 0);
    const draw0 = drawMs();
    const cpu0 = (await api("cpu")).cpuSeconds as number | null;
    const t0 = performance.now();
    const played = await Promise.all(
      views.map((v) => (titles ? spinTitle(v, seconds) : play(v, { item: ITEM, mode: "rate", rate, seconds }))),
    );
    await drawnAfterParse(views[0]!);
    const elapsed = (performance.now() - t0) / 1000;
    const cpu1 = (await api("cpu")).cpuSeconds as number | null;
    const drawn1 = drawMs();
    const frames = stop();
    offs.forEach((off) => off?.());
    const hooked = offs.every(Boolean);
    return {
      agents: n,
      rate,
      seconds: elapsed,
      bytes: played.reduce((a, p) => a + p.bytes, 0),
      shownDrawsPerSecond: hooked ? drawn[0]! / elapsed : null,
      hiddenDrawsPerSecond: hooked ? drawn.slice(1).reduce((a, b) => a + b, 0) / elapsed : null,
      drawMsPerSecond: views.every((v) => benchHooks(v).drawMs) ? (drawn1 - draw0) / elapsed : null,
      cpuPercent: cpu0 !== null && cpu1 !== null ? ((cpu1 - cpu0) / elapsed) * 100 : null,
      frames,
    };
  },

  // Screen: an item drawn for a screenshot, for correctness by eye.
  async screen() {
    const view = await newView();
    await play(view, { item: ITEM, mode: "max" });
    await drawnAfterParse(view);
    await sleep(300);
    const shot = await api(`snap?name=${encodeURIComponent(ITEM.replace(/\W+/g, "-"))}`);
    return { screenshot: shot.path ?? null };
  },
};

// Live: one view on a real PTY, for the input, selection and accessibility
// checks (check.mjs) and the hand checks (live.sh). `pty=shell` runs the login shell, otherwise `cat` in
// raw mode echoes what is typed. `sr=1` turns the screen reader mode on, and
// `item=` writes a corpus item first. The page never posts a result: a
// person uses it by hand, and runners/wkcheck.swift drives it through the
// globals below.
cases.live = async () => {
  const view = await newView();
  if (q.get("sr") === "1") view.setScreenReader(true);
  const ws = new WebSocket(socketUrl(`/ws/${q.get("pty") === "shell" ? "shell" : "pty"}`));
  ws.binaryType = "arraybuffer";
  await new Promise<void>((r, j) => {
    ws.onopen = () => r();
    ws.onerror = () => j(new Error("pty socket failed"));
  });
  const panel = document.createElement("pre");
  panel.id = "log";
  panel.style.cssText =
    "position:fixed;top:0;left:980px;right:0;bottom:0;margin:0;padding:8px;overflow:auto;color:#9a9a9a;font:11px Menlo,monospace;white-space:pre-wrap";
  document.body.append(panel);
  const note = (line: string) => {
    panel.textContent = `${line}\n${panel.textContent ?? ""}`.slice(0, 20000);
  };
  const hex = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join(" ");
  const live = {
    inputs: [] as { kind: string; hex: string; text: string }[],
    /** The emulator's replies, which go to the PTY but are not input. */
    replies: [] as string[],
    selections: [] as (string | null)[],
    links: [] as string[],
    titles: [] as string[],
    /** Uncaught errors and rejections on the page. */
    errors: [] as string[],
    write: (text: string) => new Promise<void>((r) => view.write(new TextEncoder().encode(text), () => r())),
    /** The grid's top left corner and a cell's size, in CSS pixels. */
    grid: () => {
      const host = document.getElementById("main")!;
      const { cellPx } = view.measure();
      const el = host.querySelector(".xterm-screen") ?? host.querySelector("canvas") ?? host;
      const r = el.getBoundingClientRect();
      return { x: r.left, y: r.top, w: cellPx[0], h: cellPx[1] };
    },
    selection: () => view.selection(),
    inputElement: () => {
      const el = benchHooks(view).inputElement?.();
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return { tag: el.tagName, x: r.left, y: r.top, w: r.width, h: r.height, focused: document.activeElement === el };
    },
    focus: () => view.focus(),
  };
  addEventListener("error", (e) => live.errors.push(String(e.error?.stack ?? e.message)));
  addEventListener("unhandledrejection", (e) => live.errors.push(String(e.reason?.stack ?? e.reason)));
  (window as unknown as { __live: typeof live }).__live = live;
  view.onInput((bytes, kind) => {
    const text = new TextDecoder().decode(bytes);
    live.inputs.push({ kind, hex: hex(bytes), text });
    note(`input ${kind}: ${hex(bytes)}  ${JSON.stringify(text)}`);
    ws.send(bytes);
  });
  view.onReply((bytes) => {
    live.replies.push(hex(bytes));
    note(`reply: ${hex(bytes)}`);
    ws.send(bytes);
  });
  view.onSelectionChange((t) => {
    live.selections.push(t);
    note(`selection: ${t === null ? "none" : `${t.length} chars, ${t.split("\n").length} lines`}`);
  });
  view.onLink((uri) => {
    live.links.push(uri);
    note(`link: ${uri}`);
  });
  view.onTitle((t) => {
    live.titles.push(t);
    note(`title: ${t}`);
  });
  ws.onmessage = (m) => {
    if (typeof m.data === "string") return;
    view.write(new Uint8Array(m.data as ArrayBuffer), () => {});
  };
  if (q.has("item")) {
    const bytes = await fetchItem(ITEM);
    await new Promise<void>((r) => view.write(bytes, () => r()));
  }
  view.focus();
  note(
    `${ADAPTER}, ${q.get("pty") === "shell" ? "login shell" : "cat in raw mode"}${q.get("sr") === "1" ? ", screen reader on" : ""}`,
  );
  document.title = "ready";
  return new Promise<never>(() => {});
};

/** Sets the view's title 10 times a second for some seconds, with a
 *  spinner before the text, as Claude Code does while it works. */
async function spinTitle(view: TerminalView, seconds: number): Promise<{ bytes: number }> {
  const spinner = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
  let bytes = 0;
  for (let i = 0; i < seconds * 10; i++) {
    const title = new TextEncoder().encode(`\x1b]2;${spinner[i % spinner.length]} Running the tests\x07`);
    bytes += title.length;
    view.write(title, () => {});
    await sleep(100);
  }
  return { bytes };
}

/** Streams tui output into a second view at 2 MB/s until stopped. */
function startLoad(view: TerminalView) {
  let stopped = false;
  const frames = frameRecorder();
  const run = (async () => {
    let bytes = 0;
    while (!stopped)
      bytes += (await play(view, { item: "synthetic/tui.bin", mode: "rate", rate: 2e6, seconds: 2 })).bytes;
    return bytes;
  })();
  return async () => {
    stopped = true;
    const bytes = await run;
    return { bytes, frames: frames() };
  };
}

function dispatchKey(target: HTMLElement, ch: string) {
  const code = `Key${ch.toUpperCase()}`;
  const init: KeyboardEventInit = { key: ch, code, bubbles: true, cancelable: true };
  const down = new KeyboardEvent("keydown", init);
  // xterm.js reads the legacy keyCode to recognise printable keys.
  Object.defineProperty(down, "keyCode", { get: () => ch.toUpperCase().charCodeAt(0) });
  Object.defineProperty(down, "which", { get: () => ch.toUpperCase().charCodeAt(0) });
  const handled = !target.dispatchEvent(down);
  if (!handled) {
    // A renderer that types on keypress or input, as a browser would deliver it.
    const press = new KeyboardEvent("keypress", { ...init, charCode: ch.charCodeAt(0) } as KeyboardEventInit);
    Object.defineProperty(press, "charCode", { get: () => ch.charCodeAt(0) });
    Object.defineProperty(press, "keyCode", { get: () => ch.charCodeAt(0) });
    Object.defineProperty(press, "which", { get: () => ch.charCodeAt(0) });
    target.dispatchEvent(press);
  }
  target.dispatchEvent(new KeyboardEvent("keyup", init));
}

function pageBytes(): number {
  const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
  return nav?.decodedBodySize ?? 0;
}

// ---------------------------------------------------------------- main

let loadMs = 0;
let loadedAt = 0;

async function main() {
  const result: Record<string, unknown> = {
    adapter: ADAPTER,
    case: CASE,
    item: ITEM,
    userAgent: navigator.userAgent,
    dpr: devicePixelRatio,
    cspEnforced,
  };
  try {
    const factory = RENDERERS[ADAPTER] ?? COMPARISONS[ADAPTER];
    if (!factory) throw new Error(`unknown adapter ${ADAPTER}`);
    const t0 = performance.now();
    renderer = await factory();
    await renderer.load();
    loadedAt = performance.now();
    loadMs = loadedAt - t0;
    const fn = cases[CASE];
    if (!fn) throw new Error(`unknown case ${CASE}`);
    Object.assign(result, await fn());
    if (lastView) result.backend = benchHooks(lastView).backend?.() ?? "unknown";
  } catch (e) {
    result.error = String((e as Error)?.stack ?? e).slice(0, 500);
  }
  if (cspViolations.length) result.cspViolations = cspViolations.slice(0, 10);
  log(JSON.stringify(result));
  // a runner that keeps one window open names the next case, or done
  const { next } = (await api("result", result)) as { next?: string };
  document.title = "done";
  if (next === "done") location.href = new URL("done", location.href).href;
  else if (next) location.href = next;
}

main();
