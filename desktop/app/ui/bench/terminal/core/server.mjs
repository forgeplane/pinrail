// The fake core: what the window talks to in place of Pinrail's core.
//
// - Serves the bench page, the bundle and the corpus over HTTP.
// - /ws/replay plays a corpus item into the page as binary frames, with
//   credit-based flow control (the page returns credit once it has parsed
//   the bytes), as fast as credit allows, at a byte rate, or at a
//   recording's own timing. It also answers attach requests with a snapshot
//   of a terminal holding 5,000 lines of history.
// - /ws/pty connects the page to an echo program on a real PTY, for the
//   input probe.
// - /api/* lets the page post results and ask the runner for process memory
//   samples and screenshots.
//
// Run alone to open the page by hand: node core/server.mjs [--port 7357]

import { createServer } from "node:http";
import { readFileSync, existsSync } from "node:fs";
import { join, dirname, extname, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { WebSocketServer } from "ws";
import { policy } from "./csp.mjs";
import { snapshot, SOURCES } from "./snapshots.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const corpusDir = join(root, "corpus");
const cacheDir = join(root, ".cache");
// where the app serves libghostty-vt's WebAssembly from, which
// scripts/ghostty-wasm.sh builds
const publicDir = join(root, "..", "..", "public");
const workspace = join(root, "..", "..", "..", "..");

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".wasm": "application/wasm",
  ".map": "application/json",
  ".json": "application/json",
  ".cast": "application/x-asciicast",
};
const CHUNK = 16 * 1024;
const WINDOW = 1024 * 1024;

// ------------------------------------------------------------- corpus items

const items = new Map();

/** Loads a corpus item as a list of [seconds, bytes] events, plus resizes. */
export function loadItem(name) {
  if (items.has(name)) return items.get(name);
  const path = normalize(join(corpusDir, name));
  if (!path.startsWith(corpusDir) || !existsSync(path)) throw new Error(`no corpus item ${name}`);
  let item;
  if (name.endsWith(".cast")) {
    const lines = readFileSync(path, "utf8").split("\n").filter(Boolean);
    const header = JSON.parse(lines[0]);
    const enc = new TextEncoder();
    const events = [];
    for (const l of lines.slice(1)) {
      const [t, kind, data] = JSON.parse(l);
      if (kind === "o") events.push({ t, bytes: enc.encode(data) });
      else if (kind === "r") {
        const [cols, rows] = data.split("x").map(Number);
        events.push({ t, resize: { cols, rows } });
      }
    }
    const size = events.reduce((a, e) => a + (e.bytes?.length ?? 0), 0);
    item = { header, events, size, timed: true };
  } else {
    const bytes = new Uint8Array(readFileSync(path));
    item = { header: { width: 120, height: 40 }, events: [{ t: 0, bytes }], size: bytes.length, timed: false };
  }
  items.set(name, item);
  return item;
}

/** The item's bytes in order, split into chunks of at most CHUNK. */
function* chunks(item, limit = Infinity) {
  let sent = 0;
  for (const e of item.events) {
    if (!e.bytes) continue;
    for (let i = 0; i < e.bytes.length && sent < limit; i += CHUNK) {
      const c = e.bytes.subarray(i, Math.min(e.bytes.length, i + CHUNK, i + (limit - sent)));
      sent += c.length;
      yield c;
    }
  }
}

// ------------------------------------------------------------------- replay

function replay(ws, msg) {
  const item = loadItem(msg.item);
  let outstanding = 0;
  let sent = 0;
  let closed = false;
  let wake = null;
  ws.on("close", () => {
    closed = true;
    wake?.();
  });
  ws.on("message", (data, isBinary) => {
    if (isBinary) return;
    const m = JSON.parse(String(data));
    if (m.t === "credit") {
      outstanding -= m.n;
      wake?.();
    }
  });
  const waitCredit = async (n) => {
    while (!closed && outstanding + n > WINDOW) await new Promise((r) => (wake = r));
    wake = null;
  };
  const send = (bytes) => {
    outstanding += bytes.length;
    sent += bytes.length;
    ws.send(bytes, { binary: true });
  };
  const end = () => !closed && ws.send(JSON.stringify({ t: "end", bytes: sent }));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

  (async () => {
    if (msg.mode === "max") {
      for (const c of chunks(item, msg.maxBytes || Infinity)) {
        await waitCredit(c.length);
        if (closed) return;
        send(c);
      }
    } else if (msg.mode === "realtime" && item.timed) {
      const gapCap = msg.gapCap ?? 0.5;
      const t0 = performance.now();
      let virtual = 0;
      let prev = 0;
      for (const e of item.events) {
        virtual += Math.min(gapCap, e.t - prev);
        prev = e.t;
        const due = t0 + virtual * 1000 - performance.now();
        if (due > 1) await sleep(due);
        if (closed) return;
        if (e.resize) ws.send(JSON.stringify({ t: "resize", ...e.resize }));
        else {
          await waitCredit(e.bytes.length);
          send(e.bytes);
        }
      }
    } else {
      // A byte rate, in ticks of 8 ms, looping the item for `seconds`.
      const rate = msg.rate ?? 2e6;
      const seconds = msg.seconds ?? 5;
      const all = [...chunks(item)];
      let ci = 0;
      let carry = new Uint8Array(0);
      const t0 = performance.now();
      let due = 0;
      while (!closed && performance.now() - t0 < seconds * 1000) {
        const elapsed = (performance.now() - t0) / 1000;
        let want = Math.floor(elapsed * rate) - due;
        while (want > 0 && !closed) {
          if (!carry.length) {
            carry = all[ci];
            ci = (ci + 1) % all.length;
          }
          const n = Math.min(want, carry.length);
          if (outstanding + n > WINDOW) break; // the page is behind: skip, as a PTY would block
          send(carry.subarray(0, n));
          carry = carry.subarray(n);
          due += n;
          want -= n;
        }
        await sleep(8);
      }
    }
    end();
  })().catch((e) => !closed && ws.send(JSON.stringify({ t: "error", message: String(e) })));
}

// ------------------------------------------------------------------- server

/** Per-run hooks the runner registers: memory samples, screenshots, results. */
const runs = new Map();

export function registerRun(id, hooks) {
  runs.set(id, hooks);
  return () => runs.delete(id);
}

export async function startCore({ port = 0, host = "127.0.0.1" } = {}) {
  // Each snapshot in every format the core can make, by format.
  const snapshots = {};
  for (const name of Object.keys(SOURCES))
    snapshots[name] = snapshot(name, { corpus: corpusDir, cache: cacheDir, workspace });
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, "http://x");
    const path = url.pathname;
    try {
      if (path.startsWith("/api/")) {
        const run = runs.get(url.searchParams.get("run"));
        const reply = (obj) => {
          res.writeHead(200, { "content-type": "application/json" });
          res.end(JSON.stringify(obj));
        };
        if (path === "/api/result") {
          let body = "";
          for await (const c of req) body += c;
          run?.result?.(JSON.parse(body));
          return reply({ ok: true });
        }
        if (path === "/api/sample")
          return reply({ rssMiB: (await run?.sample?.(url.searchParams.get("label"))) ?? null });
        if (path === "/api/snap") return reply({ path: (await run?.snap?.(url.searchParams.get("name"))) ?? null });
        res.writeHead(404);
        return res.end();
      }
      let file;
      if (path === "/" || path === "/page.html") file = join(root, "page.html");
      else if (path.startsWith("/dist/")) file = normalize(join(root, path));
      else if (path.startsWith("/corpus/")) file = normalize(join(corpusDir, path.slice("/corpus/".length)));
      else if (path.startsWith("/ghostty/")) file = normalize(join(publicDir, path));
      if (!file || !(file.startsWith(root) || file.startsWith(join(publicDir, "ghostty"))) || !existsSync(file)) {
        res.writeHead(404);
        return res.end("not found");
      }
      const headers = {
        "content-type": TYPES[extname(file)] ?? "application/octet-stream",
        "cache-control": "no-store",
      };
      if (extname(file) === ".html") {
        const csp = policy(url.searchParams.get("csp") ?? "app");
        if (csp) headers["content-security-policy"] = csp;
      }
      res.writeHead(200, headers);
      res.end(readFileSync(file));
    } catch (e) {
      res.writeHead(500);
      res.end(String(e));
    }
  });

  const wss = new WebSocketServer({ noServer: true, perMessageDeflate: false });
  server.on("upgrade", (req, socket, head) => {
    const path = new URL(req.url, "http://x").pathname;
    wss.handleUpgrade(req, socket, head, (ws) => {
      ws._socket?.setNoDelay?.(true);
      if (path === "/ws/replay") {
        ws.once("message", (data) => {
          const msg = JSON.parse(String(data));
          if (msg.t === "play") replay(ws, msg);
          else if (msg.t === "attach") {
            const forms = snapshots[msg.snapshot];
            if (!forms) return ws.send(JSON.stringify({ t: "error", message: `no snapshot ${msg.snapshot}` }));
            // The first format the view reads that the core has, as the host would choose.
            const format = (msg.formats ?? ["vt"]).find((f) => forms[f]) ?? "vt";
            ws.send(JSON.stringify({ t: "snapshot", format, bytes: forms[format].length }));
            ws.send(forms[format], { binary: true });
          }
        });
      } else if (path === "/ws/pty") {
        const child = spawn("python3", ["-u", join(here, "pty-echo.py")], { stdio: ["pipe", "pipe", "inherit"] });
        child.stdout.on("data", (d) => ws.readyState === 1 && ws.send(d, { binary: true }));
        ws.on("message", (d, isBinary) => isBinary && child.stdin.write(d));
        ws.on("close", () => child.kill());
        setTimeout(() => ws.readyState === 1 && ws.send(JSON.stringify({ t: "ready" })), 300);
      } else ws.close();
    });
  });

  await new Promise((r) => server.listen(port, host, r));
  const actual = server.address().port;
  return {
    port: actual,
    url: `http://${host}:${actual}`,
    close: () =>
      new Promise((r) => {
        wss.clients.forEach((c) => c.terminate());
        server.close(r);
      }),
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const i = process.argv.indexOf("--port");
  const host = process.argv.includes("--public") ? "0.0.0.0" : "127.0.0.1";
  const core = await startCore({ port: i > 0 ? Number(process.argv[i + 1]) : 7357, host });
  console.log(`fake core on ${core.url}/page.html?adapter=ghostty&case=burst&item=synthetic/tui.bin`);
}
