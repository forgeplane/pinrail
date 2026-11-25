// The checks of Pinrail's agent support that need no person, run for each
// runtime agent installed on this computer:
//
//   node scripts/agent-check/check.mjs                 # every installed agent
//   node scripts/agent-check/check.mjs claude-code     # only these
//   node scripts/agent-check/check.mjs --no-build      # the desktop binary as it is
//
// A Pinrail core of its own runs headless on a scratch data directory, with
// the terminal host it starts, so nothing reaches the person's Pinrail: the
// agents it runs report to it, as every run reports to the core that
// started it. The agents are the real ones, found by the person's login
// shell, signed in as the person, and they answer real prompts, which costs
// what a short prompt costs. An agent that is not installed or not signed
// in is skipped.

import { execFileSync, spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const desktop = path.join(root, "desktop");
const args = process.argv.slice(2);
const only = args.filter((a) => !a.startsWith("--"));
const scratch = path.join(os.tmpdir(), "pinrail-agent-check");
const data = path.join(scratch, "data");
const repo = path.join(scratch, "repo");

/** What each runtime agent shows that the checks answer or look for: its
 *  folder-trust question, and the keys that answer yes, Enter when none. */
const AGENTS = {
  "claude-code": { name: "Claude Code", trust: /trust this folder|Do you trust/i, yes: ["\x1bOB", "\r"] },
  codex: {
    name: "Codex",
    trust: /Trust and continue|trust the contents|Do you trust/i,
    // its own offer to update itself, answered with Skip: the check runs
    // the agent as it is installed
    update: /Update available/i,
    skipUpdate: ["\x1b[B", "\r"],
  },
  cursor: { name: "Cursor", trust: /Trust this workspace|Do you trust/i },
  antigravity: { name: "Antigravity", trust: /trust this folder|Do you trust/i },
  opencode: { name: "OpenCode", trust: /Do you trust/i },
  grok: { name: "Grok", trust: /trust this folder|Do you trust/i },
  pi: { name: "Pi", trust: /Do you trust/i },
  copilot: { name: "Copilot", trust: /trust the files|Do you trust/i },
};
/** What an agent shows when it cannot answer: not signed in, or out of the
 *  usage its plan allows. */
const CANNOT_ANSWER = [
  [
    /\b(log ?in|sign in|authenticate|API key)\b.*\b(to continue|required|first)\b|Select login method|Please log in/i,
    "not signed in",
  ],
  [/usage limit reached|quota exceeded|out of credits/i, "usage limit reached"],
];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const freePort = () =>
  new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });

/** Polls `test` until it returns something, or throws `what` after `ms`. */
async function until(test, what, ms = 120_000) {
  for (const end = Date.now() + ms; ;) {
    const got = await test();
    if (got) return got;
    if (Date.now() > end) throw new Error(`timed out: ${what}`);
    await sleep(500);
  }
}

/** The core: started, stopped, and called as the person. */
async function startCore() {
  if (!args.includes("--no-build")) {
    console.log("agent-check: building the desktop app");
    execFileSync("cargo", ["build", "--quiet", "-p", "pinrail-desktop"], { cwd: desktop, stdio: "inherit" });
  }
  const bin = path.join(desktop, "target", "debug", "Pinrail");
  const hostBin = path.join(desktop, "target", "debug", "pinrail-host");
  const stopHost = () => {
    try {
      execFileSync(hostBin, ["stop", "--data-dir", data], { stdio: "ignore", timeout: 20_000 });
    } catch {
      // none answered
    }
    try {
      execFileSync("pkill", ["-f", path.join(data, "host", "bin") + path.sep], { stdio: "ignore" });
    } catch {
      // none was left
    }
  };
  stopHost();
  fs.rmSync(data, { recursive: true, force: true });
  fs.mkdirSync(data, { recursive: true });
  fs.writeFileSync(
    path.join(data, "settings.json"),
    JSON.stringify({ welcome: { seen: true }, telemetry: { usage: false, crashes: false } }),
  );
  const port = await freePort();
  const core = `http://127.0.0.1:${port}`;
  const origin = "http://127.0.0.1:1";
  const token = randomBytes(32).toString("hex");
  const tokenFile = path.join(scratch, "person-token");
  fs.writeFileSync(tokenFile, token, { mode: 0o600 });
  // nothing of the person's own Pinrail, its server and its data, and
  // nothing of an agent this script runs in, which the agents it starts
  // could take as their own session's
  const inherited = /^(PINRAIL_|CLAUDE_|CLAUDECODE$|CODEX_|OPENCODE)/;
  const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => !inherited.test(k)));
  const server = spawn(
    bin,
    ["--headless", "--port", String(port), "--data-dir", data, "--person-token-file", tokenFile],
    { env: { ...env, PINRAIL_SHELL_ORIGIN: origin }, stdio: ["ignore", "ignore", "inherit"] },
  );
  const api = async (method, route, body) => {
    const res = await fetch(`${core}${route}`, {
      method,
      headers:
        method === "GET"
          ? { authorization: `Bearer ${token}` }
          : { "content-type": "application/json", origin, authorization: `Bearer ${token}` },
      body: body ? JSON.stringify(body) : undefined,
    });
    const text = await res.text();
    if (!res.ok) throw new Error(`${method} ${route}: ${res.status} ${text}`);
    return text ? JSON.parse(text) : null;
  };
  const stop = async () => {
    const exited = new Promise((r) => server.once("exit", r));
    server.kill("SIGTERM");
    await exited;
    stopHost();
  };
  try {
    await until(() => api("GET", "/api/v1/info").catch(() => null), "the core to answer", 60_000);
    await until(
      async () => (await api("GET", "/api/v1/runtime")).state === "ready",
      "the terminal host to start",
      60_000,
    );
  } catch (error) {
    await stop();
    throw error;
  }
  return { core, token, origin, api, stop };
}

/** A git repository to run the agents in, as a person's would be. */
function makeRepo() {
  fs.rmSync(repo, { recursive: true, force: true });
  fs.mkdirSync(repo, { recursive: true });
  fs.writeFileSync(path.join(repo, "README.md"), "# Agent check\n\nA scratch repository for Pinrail's agent checks.\n");
  fs.writeFileSync(path.join(repo, "app.js"), "export const add = (a, b) => a + b;\n");
  const git = (...a) => execFileSync("git", a, { cwd: repo, stdio: "ignore" });
  git("init", "-q");
  git("add", ".");
  git("-c", "user.name=Agent check", "-c", "user.email=check@example.com", "commit", "-qm", "Start");
}

/** Presses `keys` in a run's terminal, one after another, as the person
 *  would at its keyboard. */
async function press(app, run, keys) {
  const { ticket } = await app.api("POST", `/api/v1/terminals/${encodeURIComponent(run.terminal)}/tickets`);
  const url = `${app.core.replace("http", "ws")}/api/v1/terminals/${encodeURIComponent(run.terminal)}/attach?${new URLSearchParams(
    { ticket, cols: "120", rows: "40", formats: "" },
  )}`;
  const ws = new WebSocket(url, { headers: { origin: app.origin } });
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = () => reject(new Error("the terminal did not attach"));
  });
  await sleep(500);
  for (const key of keys) {
    const bytes = new TextEncoder().encode(key);
    const frame = new Uint8Array(bytes.length + 1);
    frame[0] = 0x10;
    frame.set(bytes, 1);
    ws.send(frame);
    await sleep(400);
  }
  ws.close();
}

/** Waits until the agent's screen has not changed for three seconds,
 *  answering its prompts on the way, so that what is typed next reaches
 *  its input. */
async function settled(app, ctx, run) {
  let last = "";
  let since = Date.now();
  await waitForScreen(
    app,
    ctx,
    run,
    ctx.agent,
    {
      test: (text) => {
        if (text !== last) [last, since] = [text, Date.now()];
        return text.trim() !== "" && Date.now() - since >= 3_000;
      },
    },
    "the agent's screen to settle",
    90_000,
  );
}

/** Types `prompt` into the agent's input and submits it once it shows. */
async function typePrompt(app, ctx, run, prompt) {
  await settled(app, ctx, run);
  await press(app, run, [prompt]);
  // the start of the prompt, which may wrap
  const start = prompt.slice(0, 30);
  const flat = (text) => text.replace(/\s+/g, " ");
  await until(
    async () => flat(await screen(app, run)).includes(start),
    "the typed prompt to show in the agent's input",
    15_000,
  );
  await press(app, run, ["\r"]);
}

/** The last lines the run's terminal shows. */
const screen = async (app, run, lines = 60) =>
  (await app.api("GET", `/api/v1/runs/${run.id}/output?lines=${lines}`)).text;

/** Waits for `want` on the run's screen, answering the agent's folder-trust
 *  question on the way, and noting it in `ctx.trusted` with the screen it
 *  showed; throws when the agent is not signed in. */
async function waitForScreen(app, ctx, run, agent, want, what, ms = 180_000) {
  let trusted = false;
  let skipped = false;
  return until(
    async () => {
      const text = await screen(app, run);
      for (const [shows, why] of CANNOT_ANSWER)
        if (shows.test(text)) throw Object.assign(new Error(why), { skip: true });
      if (!skipped && agent.update?.test(text)) {
        skipped = true;
        await press(app, run, agent.skipUpdate);
        return null;
      }
      if (!trusted && agent.trust.test(text)) {
        trusted = true;
        ctx.trusted.push({ run: run.id, screen: text });
        await press(app, run, agent.yes ?? ["\r"]);
        return null;
      }
      return want.test(text) ? text : null;
    },
    what,
    ms,
  );
}

/** The agent's answer to a prompt that ends with "and nothing else.": what
 *  its screen shows after the last copy of the prompt, which may wrap. */
const answer = (text) => {
  const at = text.lastIndexOf("nothing else.");
  return at < 0 ? "" : text.slice(at + "nothing else.".length);
};

/** Waits for the run to end, and fails when it ends with an error. */
async function ended(app, run) {
  return until(
    async () => {
      const now = await app.api("GET", `/api/v1/runs/${run.id}`);
      return now.state === "ended" ? now : null;
    },
    "the run to end",
    60_000,
  );
}

/** A new session with a first task starts the agent in Pinrail's
 *  terminal with the task, and the session takes the agent's own id. */
async function newSession(app, ctx) {
  const { run } = await app.api("POST", "/api/v1/runs", {
    agent: ctx.id,
    folder: repo,
    task: "Reply with only the word READY, in capitals, and nothing else.",
    cols: 120,
    rows: 40,
  });
  ctx.run = run;
  await waitForScreen(
    app,
    ctx,
    run,
    ctx.agent,
    { test: (text) => /\bREADY\b/.test(answer(text)) },
    "the agent to answer READY",
  );
  const named = await until(
    async () => {
      const { session } = await app.api("GET", `/api/v1/sessions/${run.session.agent}/${run.session.id}`);
      return session.agent_session ? session : null;
    },
    "the session to take the agent's id",
    60_000,
  );
  ctx.session = named;
  return `session ${named.id} is the agent's ${named.agent_session}`;
}

/** Stopped, then resumed, the agent continues the same conversation. */
async function resume(app, ctx) {
  if (!ctx.session) throw Object.assign(new Error("needs a new session"), { needs: true });
  await app.api("POST", `/api/v1/runs/${ctx.run.id}/stop`, {});
  await ended(app, ctx.run);
  const { run } = await app.api("POST", `/api/v1/sessions/${ctx.session.agent}/${ctx.session.id}/resume`, {
    cols: 120,
    rows: 40,
  });
  ctx.run = run;
  await typePrompt(
    app,
    ctx,
    run,
    "Which word did you reply with before? Answer with it in lowercase, and nothing else.",
  );
  await waitForScreen(
    app,
    ctx,
    run,
    ctx.agent,
    { test: (text) => /\bready\b/.test(answer(text)) },
    "the agent to remember its answer",
  );
  const now = await app.api("GET", `/api/v1/runs/${run.id}`);
  if (now.session?.agent_session !== ctx.session.agent_session)
    throw new Error(`the resumed run is the agent's ${now.session?.agent_session}, not ${ctx.session.agent_session}`);
  return "remembered its answer in the same session";
}

/** No prompt asks to trust Pinrail's files folder, at start or resume: a
 *  trust question names the repository, or comes at the first start. */
async function noFilesFolderTrust(app, ctx) {
  if (!ctx.session) throw Object.assign(new Error("needs a new session"), { needs: true });
  // by a part of each path, as an agent may show /var as /private/var
  const names = (screen, folder) => screen.replace(/\s+/g, "").includes(path.relative(os.tmpdir(), folder));
  const wrong = ctx.trusted.filter((t, i) => names(t.screen, data) || (i > 0 && !names(t.screen, repo)));
  if (wrong.length) {
    const last = wrong[0].screen
      .split("\n")
      .filter((l) => l.trim())
      .slice(-8)
      .join(" ⏎ ");
    throw new Error(`asked to trust a folder other than the repository: ${last}`);
  }
  return ctx.trusted.length ? "asked only about the repository" : "asked nothing";
}

/** A solid red PNG, for an agent to say the colour of. */
function redPng(size = 64) {
  const chunk = (type, body) => {
    const out = Buffer.alloc(body.length + 12);
    out.writeUInt32BE(body.length, 0);
    out.write(type, 4, "ascii");
    body.copy(out, 8);
    out.writeUInt32BE(zlib.crc32(out.subarray(4, 8 + body.length)), 8 + body.length);
    return out;
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header.set([8, 2, 0, 0, 0], 8);
  const row = Buffer.concat([Buffer.from([0]), Buffer.alloc(size * 3, Buffer.from([220, 20, 20]))]);
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", header),
    chunk("IDAT", zlib.deflateSync(Buffer.concat(Array(size).fill(row)))),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

/** The session's thread: its messages and turns. */
const thread = (app, ctx) => app.api("GET", `/api/v1/sessions/${ctx.session.agent}/${ctx.session.id}`);

/** Sends a message from the summary, as the message box does, with `files`
 *  saved in the session's files folder and their paths on lines of their
 *  own after the text, and waits until it is delivered. */
async function send(app, ctx, text, files = {}) {
  const folder = path.join(data, "sessions", ctx.session.agent, ctx.session.id, "files");
  fs.mkdirSync(folder, { recursive: true });
  const paths = Object.entries(files).map(([name, bytes]) => {
    fs.writeFileSync(path.join(folder, name), bytes);
    return path.join(folder, name);
  });
  const sent = await app.api("POST", `/api/v1/sessions/${ctx.session.agent}/${ctx.session.id}/messages`, {
    text: [text, ...paths].filter(Boolean).join("\n\n"),
  });
  const message = await until(
    async () => {
      const m = (await thread(app, ctx)).messages.find((m) => m.id === sent.id);
      return m?.state === "delivered" ? m : null;
    },
    "the message to be delivered",
    180_000,
  );
  return message;
}

/** The turns that began with `message` as their prompt: as the app matches
 *  them, those that began within 5 seconds after its delivery with a prompt
 *  that starts with its text, or for files alone, that names its files. */
function turnsOf(turns, message, text, files = []) {
  const delivered = Date.parse(message.delivered_at);
  return turns.filter((t) => {
    // the report that confirms the delivery begins the turn: an earlier
    // message's turn began before it
    const after = Date.parse(t.started_at) - delivered;
    if (after < -1_000 || after > 5_000) return false;
    // an agent that attaches an image shows `[Image #N]` for its path
    const prompt = (t.prompt ?? "").replace(/\[Image #\d+\]/g, "").trim();
    const named = (f) => prompt.includes(f) || (/\.png$/.test(f) && /\[Image #\d+\]/.test(t.prompt ?? ""));
    return text ? prompt.startsWith(text) : files.every(named);
  });
}

/** Fails unless the thread shows `message` once: it was typed into the
 *  terminal, and exactly one turn began with it, whose prompt the thread
 *  then leaves out. */
async function shownOnce(app, ctx, message, text, files = []) {
  if (message.route !== "type") throw new Error(`the message went by ${message.route}, not typed into the terminal`);
  const turns = await until(
    async () => {
      const { turns } = await thread(app, ctx);
      return turnsOf(turns, message, text, files).length ? turns : null;
    },
    "a turn to begin with the message",
    30_000,
  ).catch(async (error) => {
    const { turns } = await thread(app, ctx);
    const last = turns.at(-1);
    throw new Error(`${error.message}; the last turn's prompt: ${JSON.stringify((last?.prompt ?? "").slice(0, 160))}`);
  });
  // a message submitted twice begins a second turn at once
  const count = turnsOf(turns, message, text, files).length;
  if (count !== 1) throw new Error(`the message is the prompt of ${count} turns`);
}

/** A message sent while the agent is idle is typed, submitted once and
 *  delivered, and the thread shows it once. */
async function idleMessage(app, ctx) {
  if (!ctx.session) throw Object.assign(new Error("needs a new session"), { needs: true });
  const text = "Reply with only the word BLUE, in capitals, and nothing else.";
  const message = await send(app, ctx, text);
  await waitForScreen(
    app,
    ctx,
    ctx.run,
    ctx.agent,
    { test: (t) => /\bBLUE\b/.test(answer(t)) },
    "the agent to answer BLUE",
  );
  await shownOnce(app, ctx, message, text);
  return "delivered and answered once";
}

/** A message sent while the agent works waits for the turn to end. */
async function busyMessage(app, ctx) {
  if (!ctx.session) throw Object.assign(new Error("needs a new session"), { needs: true });
  await typePrompt(app, ctx, ctx.run, "Write the numbers from 1 to 150, one on each line, and then the word COUNTED.");
  // the turn has begun
  await until(
    async () =>
      (await thread(app, ctx)).turns.some((t) => (t.prompt ?? "").startsWith("Write the numbers") && !t.ended_at),
    "the counting turn to begin",
    60_000,
  );
  const text = "Reply with only the word GREEN, in capitals, and nothing else.";
  const message = await send(app, ctx, text);
  const { turns } = await thread(app, ctx);
  const counting = turns.find((t) => (t.prompt ?? "").startsWith("Write the numbers"));
  if (!counting?.ended_at) throw new Error("delivered while the counting turn had not ended");
  if (Date.parse(message.delivered_at) < Date.parse(counting.ended_at))
    throw new Error("delivered before the counting turn ended");
  await waitForScreen(
    app,
    ctx,
    ctx.run,
    ctx.agent,
    { test: (t) => /\bGREEN\b/.test(answer(t)) },
    "the agent to answer GREEN",
  );
  await shownOnce(app, ctx, message, text);
  return "waited for the turn to end";
}

/** The files a message attaches. */
const FILES = () => ({ "notes v2.md": "# Notes\n\nThe code word is LANTERN.\n", "shot.png": redPng() });

/** A message with a text file and an image: the agent reads both
 *  without asking, three times in a row, each submitted the first time. */
async function filesMessage(app, ctx) {
  if (!ctx.session) throw Object.assign(new Error("needs a new session"), { needs: true });
  const text =
    "Reply with the code word in the attached notes file and the colour of the attached image, as two words in capitals, and nothing else.";
  for (let n = 1; n <= 3; n++) {
    const message = await send(app, ctx, text, FILES());
    await waitForScreen(
      app,
      ctx,
      ctx.run,
      ctx.agent,
      { test: (t) => /\bLANTERN\b[\s\S]*\bRED\b/.test(answer(t)) },
      `the agent to answer LANTERN RED, message ${n}`,
    );
    await shownOnce(app, ctx, message, text).catch((error) => {
      throw new Error(`message ${n}: ${error.message}`);
    });
  }
  return "read both files, three times";
}

/** A message of files alone is delivered too. */
async function filesAlone(app, ctx) {
  if (!ctx.session) throw Object.assign(new Error("needs a new session"), { needs: true });
  const before = (await thread(app, ctx)).turns.length;
  const message = await send(app, ctx, "", FILES());
  await until(async () => (await thread(app, ctx)).turns.length > before, "a turn to begin with the files", 60_000);
  await shownOnce(app, ctx, message, "", ["notes v2.md", "shot.png"]);
  return "delivered";
}

/** The checks, in order: a check may use what an earlier one left in `ctx`. */
const CHECKS = [
  ["New session", newSession],
  ["Resume", resume],
  ["No files folder trust", noFilesFolderTrust],
  ["Message when idle", idleMessage],
  ["Message while working", busyMessage],
  ["Message with files", filesMessage],
  ["Files alone", filesAlone],
];

const app = await startCore();
const results = [];
try {
  makeRepo();
  const startable = (await app.api("GET", "/api/v1/agents/startable?refresh=true")).agents ?? [];
  for (const [id, agent] of Object.entries(AGENTS)) {
    if (only.length && !only.includes(id)) continue;
    const found = startable.find((a) => a.id === id);
    const row = { id, name: agent.name, version: found?.version ?? "", checks: {} };
    results.push(row);
    if (!found?.installed) {
      row.skipped = "not installed";
      continue;
    }
    console.log(`agent-check: ${agent.name} ${row.version}`);
    const ctx = { id, agent, trusted: [] };
    try {
      for (const [name, check] of CHECKS) {
        try {
          row.checks[name] = { mark: "✅", note: await check(app, ctx) };
        } catch (error) {
          if (error.skip) {
            row.skipped = error.message;
            break;
          }
          // the screen of a check that ran, in full in a file, and its
          // last lines in the note
          let tail = "";
          if (ctx.run && !error.needs) {
            const seen = (await screen(app, ctx.run, 200).catch(() => "")).trimEnd();
            const file = path.join(scratch, `${id}-${name.toLowerCase().replace(/\W+/g, "-")}.txt`);
            fs.writeFileSync(file, seen);
            const last = seen
              .split("\n")
              .filter((l) => l.trim())
              .slice(-6)
              .join(" ⏎ ");
            tail = `\n    screen: ${last}\n    full screen: ${file}`;
          }
          row.checks[name] = { mark: "❌", note: `${error.message}${tail}` };
        }
      }
    } finally {
      if (ctx.run) await app.api("POST", `/api/v1/runs/${ctx.run.id}/stop`, { force: true }).catch(() => {});
    }
  }
} finally {
  await app.stop();
}

// the results table, a column for each agent, then a line for each failure
const names = CHECKS.map(([n]) => n);
const cols = results;
console.log(`\n| Check | ${cols.map((r) => r.name).join(" | ")} |`);
console.log(`| --- | ${cols.map(() => "---").join(" | ")} |`);
console.log(`| Version | ${cols.map((r) => r.version || "–").join(" | ")} |`);
for (const n of names)
  console.log(`| ${n} | ${cols.map((r) => (r.skipped ? "–" : (r.checks[n]?.mark ?? "–"))).join(" | ")} |`);
console.log("");
for (const r of cols) {
  if (r.skipped) console.log(`- ${r.name}: skipped, ${r.skipped}`);
  for (const [n, c] of Object.entries(r.checks)) if (c.mark !== "✅") console.log(`- ${r.name}, ${n}: ${c.note}`);
}
process.exitCode = cols.some((r) => Object.values(r.checks).some((c) => c.mark === "❌")) ? 1 : 0;
