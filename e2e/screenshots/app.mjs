// The app for a screenshot run: the desktop core headless on a scratch data
// directory, and the shell from vite against it, as e2e/shell.config.ts runs
// them. Seeding goes through the real API; then `pin` stops the core and
// rewrites every time and id in the database to fixed values, so the same
// fixtures give the same pictures on any machine, on any day.

import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";

export const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

/** The moment every screenshot is taken at. Fixture ages count back from it. */
export const NOW = Date.parse("2026-09-22T10:00:00Z");

/** Who decides, in every picture: the person the fixtures write as. */
export const PERSON = "maya";

const freePort = () =>
  new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
    srv.on("error", reject);
  });

async function waitFor(url, what, timeout = 120_000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`${what} did not come up at ${url}`);
}

export async function startApp({ build = true } = {}) {
  const desktop = path.join(root, "desktop");
  const bin = path.join(desktop, "target", "debug", "Pinrail");
  if (build) {
    console.log("screenshots: building the desktop app");
    execFileSync("cargo", ["build", "--quiet", "-p", "pinrail-desktop"], { cwd: desktop, stdio: "inherit" });
  }
  const data = path.join(root, "e2e", ".state", "screenshots-data");
  fs.rmSync(data, { recursive: true, force: true });
  fs.mkdirSync(data, { recursive: true });
  // a returning person's data: the setup already seen, so its dialog does
  // not cover the screens
  fs.writeFileSync(path.join(data, "settings.json"), JSON.stringify({ welcome: { seen: true } }));

  const corePort = await freePort();
  const uiPort = await freePort();
  const core = `http://127.0.0.1:${corePort}`;
  const ui = `http://127.0.0.1:${uiPort}`;

  let server = null;
  const startCore = async () => {
    server = spawn(
      bin,
      [
        "--headless",
        "--port",
        String(corePort),
        "--data-dir",
        data,
        "--sdk-dir",
        path.join(desktop, "app", "sdk", "v1"),
      ],
      {
        env: { ...process.env, PINRAIL_SHELL_ORIGIN: ui },
        stdio: ["ignore", "ignore", "inherit"],
      },
    );
    await waitFor(`${core}/api/v1/info`, "the core");
  };
  const stopCore = async () => {
    if (!server) return;
    const exited = new Promise((r) => server.once("exit", r));
    server.kill("SIGTERM");
    await exited;
    server = null;
  };

  await startCore();
  execFileSync("npm", ["run", "--silent", "sdk:build"], { cwd: path.join(desktop, "app"), stdio: "inherit" });
  const vite = spawn("npx", ["vite", "--host", "127.0.0.1", "--port", String(uiPort), "--strictPort"], {
    cwd: path.join(desktop, "app"),
    env: { ...process.env, VITE_PINRAIL_URL: core },
    stdio: ["ignore", "ignore", "inherit"],
  });
  await waitFor(ui, "the shell");

  const api = async (method, route, body) => {
    const res = await fetch(`${core}${route}`, {
      method,
      headers: method === "GET" ? {} : { "content-type": "application/json" },
      body: body ? JSON.stringify(body) : undefined,
    });
    const text = await res.text();
    if (!res.ok) throw new Error(`${method} ${route}: ${res.status} ${text}`);
    return text ? JSON.parse(text) : null;
  };

  return {
    root,
    /** where a scene keeps folders a person would have in ~/code */
    code: path.join(root, "e2e", ".state", "code"),
    core,
    ui,
    data,
    api,
    /** Stops the core, rewrites times and ids to `plan`, starts it again. */
    async pin(plan) {
      await stopCore();
      const ids = pinDatabase(path.join(data, "pinrail.db"), plan);
      await startCore();
      return ids;
    },
    async stop() {
      await stopCore();
      vite.kill("SIGTERM");
    },
  };
}

const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** A review id as the core mints them: the time, then 80 bits that look
 * random and are the same for the same sequence number every run. */
export function reviewId(ms, n) {
  const entropy = BigInt("0x" + createHash("sha256").update(`pinrail-screenshots-${n}`).digest("hex").slice(0, 20));
  let bits = (BigInt(ms) << 80n) | entropy;
  let out = "";
  for (let i = 0; i < 26; i++) {
    out = ALPHABET[Number(bits & 31n)] + out;
    bits >>= 5n;
  }
  return `r_${out}`;
}

const iso = (ms) => new Date(ms).toISOString().replace(/\.\d{3}Z$/, ".000Z");

/**
 * `plan` maps each seeded review id to `{ created, outcome }`, both in
 * milliseconds before NOW (outcome only when it ended). Every row takes the
 * pinned time; events keep their order, spaced a second apart from the
 * review's creation; installed plugins are dated an hour before the oldest
 * review; whoever decided or discarded is PERSON. Returns the old id → new
 * id map.
 */
function pinDatabase(file, plan) {
  const db = new DatabaseSync(file);
  db.exec("PRAGMA foreign_keys = OFF; BEGIN");
  const ids = new Map();
  const entries = Object.entries(plan).sort((a, b) => b[1].created - a[1].created);
  entries.forEach(([old, times], n) => {
    const created = NOW - times.created;
    const id = reviewId(created, n);
    ids.set(old, id);
    db.prepare("UPDATE reviews SET id = ?, created_at = ? WHERE id = ?").run(id, iso(created), old);
    db.prepare("UPDATE reviews SET revises = ? WHERE revises = ?").run(id, old);
    db.prepare("UPDATE outcomes SET review_id = ? WHERE review_id = ?").run(id, old);
    db.prepare("UPDATE review_payloads SET review_id = ? WHERE review_id = ?").run(id, old);
    db.prepare("UPDATE events SET review_id = ? WHERE review_id = ?").run(id, old);
    db.prepare("UPDATE review_attachments SET review_id = ? WHERE review_id = ?").run(id, old);
    if (times.outcome !== undefined)
      db.prepare("UPDATE outcomes SET at = ? WHERE review_id = ?").run(iso(NOW - times.outcome), id);
    const events = db.prepare("SELECT id FROM events WHERE review_id = ? ORDER BY id").all(id);
    events.forEach((e, i) => {
      const last = i === events.length - 1 && times.outcome !== undefined;
      db.prepare("UPDATE events SET at = ? WHERE id = ?").run(
        iso(last ? NOW - times.outcome : created + i * 1000),
        e.id,
      );
    });
  });
  const oldest = Math.max(0, ...entries.map(([, t]) => t.created));
  const installedAt = iso(NOW - oldest - 3_600_000);
  db.prepare("UPDATE plugin_installs SET installed_at = ?, updated_at = ?").run(installedAt, installedAt);
  // installed from the zip a reader downloads, not copied from this checkout;
  // the official plugins keep their source
  for (const { name, version } of db
    .prepare(
      "SELECT i.name, b.version FROM plugin_installs i JOIN plugin_bundles b ON b.hash = i.bundle WHERE i.source_kind <> 'index'",
    )
    .all()) {
    db.prepare("UPDATE plugin_installs SET source_kind = 'archive', source = ? WHERE name = ?").run(
      `/Users/maya/Downloads/${name}-${version}.zip`,
      name,
    );
  }
  // the person deciding is the fixtures' person, not whoever runs this
  db.prepare("UPDATE outcomes SET by = ? WHERE kind IN ('decided', 'discarded')").run(PERSON);
  db.prepare("UPDATE events SET at = ? WHERE review_id IS NULL").run(iso(NOW - oldest - 3_600_000));
  db.exec("COMMIT");
  db.close();
  return ids;
}
