// The desktop app in dev mode with usage data on, in a data folder of its
// own: prints each batch, or sends it to api.pinrail.dev with --send.
// `mise run dev:telemetry` passes its flags as usage_send and usage_fresh.
//
//   node scripts/dev-telemetry.mjs [--send] [--fresh]

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const flag = (name) => process.argv.includes(`--${name}`) || process.env[`usage_${name}`] === "true";
const app = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "desktop", "app");
const data = path.join(os.tmpdir(), "pinrail-telemetry");

if (flag("fresh")) fs.rmSync(data, { recursive: true, force: true });
fs.mkdirSync(data, { recursive: true });
// the notice counts as shown, so events are recorded from the start, and
// the setup is skipped, as on a computer in use
const settings = path.join(data, "settings.json");
if (!fs.existsSync(settings)) {
  const shown = new Date().toISOString().replace(/\.\d+Z$/, "Z");
  fs.writeFileSync(settings, JSON.stringify({ telemetry: { notice_shown: shown }, welcome: { seen: true } }) + "\n");
}

const env = { ...process.env, PINRAIL_DATA_DIR: data, PINRAIL_PORT: "4848" };
env.PINRAIL_TELEMETRY_URL = "https://api.pinrail.dev/v1/events";
if (flag("send")) delete env.PINRAIL_TELEMETRY_DEBUG;
else env.PINRAIL_TELEMETRY_DEBUG = "1";
delete env.DO_NOT_TRACK;
delete env.PINRAIL_TELEMETRY;

console.log(`Data in ${data}. What waits and what was sent:`);
console.log("  curl -s http://127.0.0.1:4848/api/v1/telemetry | jq");
const run = spawnSync("npx", ["tauri", "dev"], {
  cwd: app,
  env,
  stdio: "inherit",
  shell: process.platform === "win32",
});
process.exit(run.status ?? 1);
