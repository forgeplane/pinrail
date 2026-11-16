#!/usr/bin/env node
// Runs every case for each of the window's renderers in each runner, prints
// the results as a Markdown table, writes them to results/ as JSON and
// Markdown, and exits with 1 when a budget of runtime-spec 8.6 fails.
//
//   node run.mjs [--runners webkit] [--adapters ghostty,xterm] [--cases burst,stream,...]
//                [--repeat 3] [--no-fail] [--no-build] [--tag name]
//                [--baseline <file>] [--write-baseline <file>] [--skip-budgets]
//
// With --baseline it also fails when a measure is more than 15 % and its
// margin worse than the baseline's (baseline.mjs), and --write-baseline
// writes this run's measures as a baseline. --skip-budgets checks the
// baseline only, for machines other than the reference ones, such as CI's.
//
// --adapters also takes ghostty-web, a renderer the app does not register,
// measured for comparison (adapters/ghostty-web.ts).
//
// Runners: webkit is Playwright's headless WebKit; wkwebview is macOS's
// system WKWebView in a window, as the app draws (runners/wkwebview.swift,
// compiled on first use); webkitgtk is WebKitGTK on Linux (see
// runners/webkitgtk/); external prints the URL of each case, to open in any
// other browser.

import { webkit } from "playwright";
import { execSync, spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import os from "node:os";
import { startCore, registerRun } from "./core/server.mjs";
import { checkBudgets, otherRows } from "./budgets.mjs";
import { regressions, summarise } from "./baseline.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const corpusDir = join(here, "corpus");
const resultsDir = join(here, "results");
const args = process.argv.slice(2);
const opt = (name, dflt) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : dflt;
};
const list = (name, dflt) => opt(name, dflt).split(",").filter(Boolean);

const RUNNERS = list("runners", "webkit");
// the renderers the window registers (ui/src/components/terminal/renderers)
const ADAPTERS = list("adapters", "ghostty,xterm");
const REPEAT = Number(opt("repeat", "1"));
const VIEWPORT = { width: 1940, height: 760 };

// ------------------------------------------------------------------- cases

function allCases() {
  const recordings = existsSync(join(corpusDir, "recordings"))
    ? readdirSync(join(corpusDir, "recordings"))
        .filter((f) => f.endsWith(".cast"))
        .map((f) => `recordings/${f}`)
    : [];
  return [
    { case: "load" },
    { case: "burst", item: "synthetic/tui.bin" },
    { case: "burst", item: "synthetic/styled.bin" },
    { case: "burst", item: "synthetic/plain.bin" },
    { case: "burst", item: "synthetic/alt.bin" },
    { case: "stream", item: "synthetic/tui.bin" },
    { case: "idle", item: "synthetic/tui.bin" },
    { case: "input" },
    { case: "input", load: "1" },
    { case: "attach" },
    { case: "attach", snapshot: "history-5000-styled" },
    { case: "attach", snapshot: "history-10000" },
    { case: "memory" },
    { case: "switch" },
    { case: "agents" },
    { case: "agents", titles: "1" },
    ...recordings.map((item) => ({ case: "replay", item })),
    { case: "replay", item: "synthetic/trickle.cast" },
    { case: "screen", item: "synthetic/unicode.bin" },
    { case: "screen", item: "synthetic/links.bin" },
    { case: "screen", item: "recordings/claude-code.cast" },
  ];
}
const wanted = opt("cases", null)?.split(",");
const CASES = allCases().filter((c) => !wanted || wanted.includes(c.case));

// Picks the best of several runs of one case: the smallest primary measure.
const primary = {
  load: (r) => r.loadMs,
  burst: (r) => r.totalMs,
  stream: (r) => r.frames?.p95,
  idle: (r) => r.drawnPerSecond ?? r.animationFrameRequestsPerSecond,
  input: (r) => r.latency?.p95,
  attach: (r) => r.attachMs,
  memory: (r) => r.perTerminalRssMiB ?? r.perTerminalHeapMiB,
  switch: (r) => r.switchMs?.max,
  replay: (r) => r.realtime?.frames?.p95,
  screen: () => 0,
};
function best(runs) {
  const ok = runs.filter((r) => !r.error);
  if (!ok.length) return runs[0];
  const key = primary[ok[0].case] ?? (() => 0);
  return ok.reduce((a, b) => ((key(b) ?? Infinity) < (key(a) ?? Infinity) ? b : a));
}

// ------------------------------------------------------------------ memory

function rssMiB(match) {
  const out = execSync("ps -axo pid=,rss=,command=", { maxBuffer: 1 << 24 }).toString();
  const sum = out
    .split("\n")
    .filter(match)
    .reduce((a, l) => a + Number(l.trim().split(/\s+/)[1]) / 1024, 0);
  return sum || null;
}

/** The CPU time the matching processes have used, in seconds, or null. */
function cpuSeconds(match) {
  const out = execSync("ps -axo pid=,time=,command=", { maxBuffer: 1 << 24 }).toString();
  const sum = out
    .split("\n")
    .filter(match)
    .reduce((a, l) => a + clock(l.trim().split(/\s+/)[1]), 0);
  return sum || null;
}

/** `ps`'s CPU time, `[[dd-]hh:]mm:ss.ss`, in seconds. */
function clock(text) {
  const [days, rest] = text.includes("-") ? text.split("-") : ["0", text];
  return Number(days) * 86400 + rest.split(":").reduce((a, part) => a * 60 + Number(part), 0);
}

// ----------------------------------------------------------------- runners

let core;
let runSeq = 0;
const shotsDir = join(resultsDir, "screens");

function caseQuery(c, adapter, id) {
  const p = new URLSearchParams({ adapter, case: c.case, run: id });
  if (c.item) p.set("item", c.item);
  if (c.load) p.set("load", c.load);
  if (c.snapshot) p.set("snapshot", c.snapshot);
  if (c.format) p.set("format", c.format);
  if (c.titles) p.set("titles", c.titles);
  return p;
}

function caseUrl(c, adapter, id) {
  return `${core.url}/page.html?${caseQuery(c, adapter, id)}`;
}

/** Waits for the page's result, with a timeout. */
function awaitResult(id, hooks, timeoutMs) {
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve({ error: `timed out after ${timeoutMs} ms` }), timeoutMs);
    registerRun(id, {
      ...hooks,
      result: (r) => {
        clearTimeout(timer);
        resolve(r);
      },
    });
  });
}

const webkitRunner = {
  name: "webkit",
  async run(c, adapter) {
    const id = `r${++runSeq}`;
    const browser = await webkit.launch({ headless: true });
    const page = await browser.newPage({ viewport: VIEWPORT });
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    const done = awaitResult(
      id,
      {
        sample: async () => rssMiB((l) => l.includes("ms-playwright/webkit") && l.includes("WebContent")),
        cpu: async () => cpuSeconds((l) => l.includes("ms-playwright/webkit") && l.includes("WebContent")),
        snap: async (name) => {
          mkdirSync(shotsDir, { recursive: true });
          const path = join(shotsDir, `webkit-${adapter}-${name}.png`);
          await page.locator("#main").screenshot({ path });
          return path;
        },
      },
      180000,
    );
    await page.goto(caseUrl(c, adapter, id));
    const r = await done;
    if (errors.length) r.pageErrors = errors.slice(0, 3);
    await browser.close();
    return r;
  },
};

function buildWkRunner() {
  const src = join(here, "runners", "wkwebview.swift");
  const bin = join(here, ".cache", "wkwebview-runner");
  if (!existsSync(bin) || statSync(bin).mtimeMs < statSync(src).mtimeMs) {
    mkdirSync(dirname(bin), { recursive: true });
    console.error("compiling the WKWebView runner…");
    const r = spawnSync("swiftc", ["-O", src, "-o", bin], { stdio: "inherit" });
    if (r.status !== 0) throw new Error("swiftc failed");
  }
  return bin;
}

const wkRunner = {
  name: "wkwebview",
  async run(c, adapter) {
    const bin = buildWkRunner();
    const id = `r${++runSeq}`;
    const child = spawn(bin, [caseUrl(c, adapter, id), String(VIEWPORT.width), String(VIEWPORT.height)], {
      stdio: ["pipe", "pipe", "inherit"],
    });
    let pid = null;
    const waiting = new Map();
    let buf = "";
    child.stdout.on("data", (d) => {
      buf += d;
      let i;
      while ((i = buf.indexOf("\n")) >= 0) {
        const line = buf.slice(0, i);
        buf = buf.slice(i + 1);
        const [word, ...rest] = line.split(" ");
        if (word === "pid") pid = Number(rest[0]);
        else if (word === "snapped") waiting.get(rest.join(" "))?.();
      }
    });
    const done = awaitResult(
      id,
      {
        sample: async () => (pid ? Number(execSync(`ps -o rss= -p ${pid}`).toString().trim()) / 1024 : null),
        cpu: async () => (pid ? clock(execSync(`ps -o time= -p ${pid}`).toString().trim()) : null),
        snap: async (name) => {
          mkdirSync(shotsDir, { recursive: true });
          const path = join(shotsDir, `wkwebview-${adapter}-${name}.png`);
          await new Promise((resolve) => {
            waiting.set(path, resolve);
            child.stdin.write(`snap ${path}\n`);
            setTimeout(resolve, 10000);
          });
          return path;
        },
      },
      180000,
    );
    const r = await done;
    child.kill();
    return r;
  },
};

// WebKitGTK on Linux: runners/webkitgtk/runner.c, built by the caller (see
// runners/webkitgtk/run-in-docker.sh) and named by WEBKITGTK_RUNNER.
const gtkRunner = {
  name: "webkitgtk",
  async run(c, adapter) {
    const bin = process.env.WEBKITGTK_RUNNER;
    if (!bin) throw new Error("set WEBKITGTK_RUNNER to the built runners/webkitgtk/runner.c");
    const id = `r${++runSeq}`;
    const child = spawn(bin, [caseUrl(c, adapter, id), String(VIEWPORT.width), String(VIEWPORT.height)], {
      stdio: ["pipe", "pipe", "inherit"],
    });
    const waiting = new Map();
    let buf = "";
    child.stdout.on("data", (d) => {
      buf += d;
      let i;
      while ((i = buf.indexOf("\n")) >= 0) {
        const [word, ...rest] = buf.slice(0, i).split(" ");
        buf = buf.slice(i + 1);
        if (word === "snapped") waiting.get(rest.join(" "))?.();
      }
    });
    const done = awaitResult(
      id,
      {
        sample: async () => rssMiB((l) => l.includes("WebKitWebProcess")),
        cpu: async () => cpuSeconds((l) => l.includes("WebKitWebProcess")),
        snap: async (name) => {
          mkdirSync(shotsDir, { recursive: true });
          const path = join(shotsDir, `webkitgtk-${adapter}-${name}.png`);
          await new Promise((resolve) => {
            waiting.set(path, resolve);
            child.stdin.write(`snap ${path}\n`);
            setTimeout(resolve, 10000);
          });
          return path;
        },
      },
      240000,
    );
    const r = await done;
    child.kill();
    return r;
  },
};

// The app's own window, in the running dev app (src-tauri/src/bench.rs):
// the page comes from its Vite server, so `mise run dev:desktop` must be
// running. One window runs every case, as the fake core answers each result
// with the next case's URL, and closes at the end. The app's web content
// process is shared with its main window, so memory is not sampled.
const APP_PAGE = "http://localhost:5173/bench/terminal/app.html";
const APP_BINARY = join(here, "..", "..", "..", "..", "target", "debug", "Pinrail");
/** Answers the last result with the next case's URL, or with "done". */
let appNext = null;

const appRunner = {
  name: "app",
  async run(c, adapter) {
    const id = `r${++runSeq}`;
    const query = caseQuery(c, adapter, id);
    query.set("core", core.url);
    const url = `${APP_PAGE}?${query}`;
    const done = new Promise((resolve) => {
      const timer = setTimeout(() => {
        appNext = null;
        resolve({ error: "timed out after 180000 ms" });
      }, 180000);
      registerRun(id, {
        result: (r) => {
          clearTimeout(timer);
          resolve(r);
          return new Promise((next) => (appNext = next));
        },
      });
    });
    if (appNext) {
      appNext(url);
      appNext = null;
    } else await openAppWindow(url);
    return done;
  },
  finish() {
    appNext?.("done");
    appNext = null;
  },
};

/** Opens the bench window in the running dev app, which a second launch
 *  of its binary hands the URL to. */
async function openAppWindow(url) {
  const page = await fetch(APP_PAGE).catch(() => null);
  if (!page?.ok) throw new Error(`the dev app's page does not load from ${APP_PAGE}: run mise run dev:desktop first`);
  if (!existsSync(APP_BINARY)) throw new Error(`no dev build at ${APP_BINARY}: run mise run dev:desktop first`);
  const r = spawnSync(APP_BINARY, ["--bench", url], { stdio: "ignore", timeout: 30000 });
  if (r.status !== 0) throw new Error(`${APP_BINARY} --bench exited with ${r.status}`);
}

const externalRunner = {
  name: "external",
  async run(c, adapter) {
    const id = `r${++runSeq}`;
    const done = awaitResult(id, {}, 600000);
    console.error(`open in the browser under test: ${caseUrl(c, adapter, id)}`);
    return done;
  },
};

const RUNNER_IMPLS = {
  webkit: webkitRunner,
  wkwebview: wkRunner,
  app: appRunner,
  webkitgtk: gtkRunner,
  external: externalRunner,
};

// --------------------------------------------------------------------- main

function machine() {
  let model = os.cpus()[0]?.model ?? "unknown";
  if (process.platform === "darwin") {
    try {
      model = execSync("sysctl -n machdep.cpu.brand_string").toString().trim();
    } catch {
      // the generic model name stays
    }
  }
  return {
    os: `${os.type()} ${os.release()}`,
    platform: process.platform,
    arch: os.arch(),
    cpu: model,
    cores: os.cpus().length,
    memoryGiB: Math.round(os.totalmem() / 2 ** 30),
  };
}

function markdown(report) {
  const lines = [];
  lines.push(`# Terminal renderer benchmark, ${report.date}`);
  lines.push("");
  const m = report.machine;
  lines.push(
    `Machine: ${m.cpu}, ${m.cores} cores, ${m.memoryGiB} GiB, ${m.os}. Runs per case: ${report.repeat} (best kept). Terminal: 120×40, 5,000 lines of scrollback.`,
  );
  lines.push("");
  for (const runner of report.runners) {
    const byAdapter = report.adapters.map((a) => report.results.filter((r) => r.runner === runner && r.adapter === a));
    if (!byAdapter.some((x) => x.length)) continue;
    const ua = byAdapter.flat().find((r) => r.userAgent)?.userAgent ?? "";
    lines.push(`## ${runner}`);
    lines.push("");
    if (ua) lines.push(`User agent: \`${ua}\``);
    const backends = report.adapters.map((a, i) => `${a}: ${byAdapter[i].find((r) => r.backend)?.backend ?? "–"}`);
    lines.push(`Drawing: ${backends.join("; ")}`);
    lines.push("");
    lines.push(`| Budget (spec 8.6) | Limit | ${report.adapters.join(" | ")} |`);
    lines.push(`| --- | --- | ${report.adapters.map(() => "---").join(" | ")} |`);
    const checks = byAdapter.map((rs) => checkBudgets(rs));
    const names = [...new Set(checks.flat().map((c) => c.name))];
    for (const name of names) {
      const first = checks.flat().find((c) => c.name === name);
      const cells = checks.map((cs) => {
        const c = cs.find((x) => x.name === name);
        if (!c) return "–";
        return c.pass === null ? `not measured` : `${c.pass ? "pass" : "**FAIL**"} ${c.value}`;
      });
      lines.push(`| ${name} | ${first.budget} | ${cells.join(" | ")} |`);
    }
    lines.push("");
    lines.push(`| Other measure | ${report.adapters.join(" | ")} |`);
    lines.push(`| --- | ${report.adapters.map(() => "---").join(" | ")} |`);
    const others = byAdapter.map((rs) => otherRows(rs));
    const otherNames = [...new Set(others.flat().map((r) => r[0]))];
    for (const name of otherNames)
      lines.push(`| ${name} | ${others.map((rs) => rs.find((r) => r[0] === name)?.[1] ?? "–").join(" | ")} |`);
    lines.push("");
  }
  lines.push("Notes:");
  lines.push("");
  lines.push(
    "- Frame times are the gaps between animation frames. WebKit has no Long Tasks API, so a long task shows as a long frame.",
  );
  lines.push(
    "- Process memory is the resident size of the web content process (WebContent). GPU memory is not counted.",
  );
  lines.push(
    "- Input latency runs from a synthetic key event to the frame that draws the echo, through the WebSocket and a PTY running `cat` in raw mode.",
  );
  lines.push(
    "- Headless Playwright browsers do not wait for the display, so their frame times are not tied to the display refresh.",
  );
  return lines.join("\n") + "\n";
}

async function main() {
  if (!existsSync(join(corpusDir, "synthetic", "tui.bin"))) {
    console.error("generating the synthetic corpus…");
    spawnSync(process.execPath, [join(corpusDir, "tools", "gen.mjs")], { stdio: "inherit" });
  }
  if (!args.includes("--no-build")) await (await import("./build.mjs")).buildBench({ quiet: true });
  core = await startCore();
  console.error(`fake core on ${core.url}`);
  const results = [];
  const started = new Date();
  for (const runnerName of RUNNERS) {
    const runner = RUNNER_IMPLS[runnerName];
    if (!runner) throw new Error(`unknown runner ${runnerName}`);
    for (const adapter of ADAPTERS) {
      for (const c of CASES) {
        const runs = [];
        for (let i = 0; i < (c.case === "screen" ? 1 : REPEAT); i++) {
          const t0 = Date.now();
          let r;
          try {
            r = await runner.run(c, adapter);
          } catch (e) {
            r = { error: String(e?.stack ?? e).slice(0, 400) };
          }
          r = { ...r, ...c, runner: runner.name, adapter, wallMs: Date.now() - t0 };
          runs.push(r);
        }
        const b = best(runs);
        results.push(b);
        console.error(
          `${runner.name.padEnd(9)} ${adapter.padEnd(16)} ${c.case.padEnd(7)} ${(c.item ?? "").padEnd(32)} ${c.load ? "load " : ""}${c.snapshot ?? ""} ${b.error ? "ERROR " + b.error.split("\n")[0].slice(0, 90) : "ok"}${b.cspViolations ? ` CSP: ${b.cspViolations.join("; ").slice(0, 120)}` : ""}`,
        );
      }
    }
  }
  for (const runner of Object.values(RUNNER_IMPLS)) runner.finish?.();
  await core.close();

  const tag = opt("tag", "");
  const stamp = started.toISOString().replace(/[:.]/g, "-").slice(0, 19) + (tag ? `-${tag}` : "");
  const latest = tag ? `latest-${tag}` : "latest";
  const report = {
    date: started.toISOString(),
    machine: machine(),
    runners: RUNNERS,
    adapters: ADAPTERS,
    repeat: REPEAT,
    results,
  };
  report.budgets = {};
  for (const runner of RUNNERS)
    for (const adapter of ADAPTERS)
      report.budgets[`${runner}/${adapter}`] = checkBudgets(
        results.filter((r) => r.runner === runner && r.adapter === adapter),
      );
  mkdirSync(resultsDir, { recursive: true });
  const md = markdown(report);
  writeFileSync(join(resultsDir, `${stamp}.json`), JSON.stringify(report, null, 1));
  writeFileSync(join(resultsDir, `${stamp}.md`), md);
  writeFileSync(join(resultsDir, `${latest}.json`), JSON.stringify(report, null, 1));
  writeFileSync(join(resultsDir, `${latest}.md`), md);
  console.log(md);
  console.error(`wrote ${join(resultsDir, stamp)}.{json,md} and ${latest}.{json,md}`);
  const failed = args.includes("--skip-budgets")
    ? []
    : Object.values(report.budgets)
        .flat()
        .filter((b) => b.pass === false)
        .map((b) => `${b.name}: ${b.value}, over the budget of ${b.budget}`);
  const measures = summarise(results);
  const baselineFile = opt("baseline", null);
  if (baselineFile) {
    const worse = regressions(measures, JSON.parse(readFileSync(baselineFile, "utf8")));
    console.error(worse.length ? `worse than the baseline:\n  ${worse.join("\n  ")}` : "within the baseline");
    failed.push(...worse);
  }
  const writeBaseline = opt("write-baseline", null);
  if (writeBaseline) writeFileSync(writeBaseline, JSON.stringify(measures, null, 1));
  if (failed.length && !args.includes("--no-fail")) process.exit(1);
  process.exit(0);
}

main().catch((e) => {
  console.error(e);
  process.exit(2);
});
