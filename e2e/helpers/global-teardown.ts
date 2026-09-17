import { loadState, serverPid } from "./state";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export default async function globalTeardown() {
  const state = loadState();
  const pid = serverPid(state);
  if (!pid) return;
  try {
    process.kill(pid, "SIGTERM");
  } catch {
    return;
  }
  // give the server a few seconds to finish its requests and go down
  for (let i = 0; i < 40; i++) {
    await sleep(250);
    try {
      process.kill(pid, 0);
    } catch {
      return;
    }
  }
  try {
    process.kill(pid, "SIGKILL");
  } catch {
    // already gone
  }
}
