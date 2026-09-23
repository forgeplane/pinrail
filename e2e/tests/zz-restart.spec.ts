import { expect, test } from "@playwright/test";
import { loadState, serverPid } from "../helpers/state";
import { submitListReview, tmpFile, pinrail, pinrailJson } from "../helpers/pinrail";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Last on purpose: it takes the shared server down and brings it back.
test("a waiter survives the server being killed and restarted", async () => {
  test.setTimeout(180_000);
  const waiter = submitListReview("restart", ["--timeout", "150"]);
  const id = await waiter.reviewId;

  const pid = serverPid();
  expect(pid).not.toBeNull();
  process.kill(pid!, "SIGTERM");
  for (let i = 0; i < 120; i++) {
    await sleep(250);
    try {
      process.kill(pid!, 0);
    } catch {
      break;
    }
  }

  const serve = pinrail(["serve"]);
  expect(serve.code, serve.stderr).toBe(0);
  expect(JSON.parse(serve.stdout).url).toBe(loadState().url);
  expect(serverPid()).not.toBe(pid);

  pinrailJson(["decide", id, "--data", tmpFile("d.json", JSON.stringify({ decisions: [], undecided: [1, 2] }))]);
  const result = await waiter.done;
  expect(result.code, result.stderr).toBe(0);
  expect(JSON.parse(result.stdout).status).toBe("decided");
});
