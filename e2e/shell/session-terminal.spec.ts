// A session whose agent Pinrail runs shows its terminal beside the summary.
// The runs list, the ticket and the attach socket are stood in for here, as
// the shell's core has no terminal host: the "program" records what it
// receives and echoes it back.

import { expect, test, type Page } from "./fixtures";
import { core } from "./helpers";

let n = 0;
const fresh = () => `s-term-${Date.now()}-${++n}`;

test.beforeEach(async ({ page }) => {
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: true } } });
});
test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: false } } });
});

/** A session with a live run, and what its program receives. */
async function sessionWithRun(page: Page) {
  const session = fresh();
  const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: "Fix the login redirect", origin: { repo: "acme/api" } },
  });
  expect(response.status(), await response.text()).toBeLessThan(300);
  const run = {
    id: "run_1",
    host: "local",
    agent: "claude-code",
    state: "running",
    terminal: "local:t_1",
    folder: "/work",
    started_at: new Date().toISOString(),
    ended_at: null,
    end_reason: null,
    exit_code: null,
    session: { agent: "claude-code", id: session },
  };
  await page.route(/\/api\/v1\/runs\?/, (route) => route.fulfill({ json: { runs: [run] } }));
  await page.route(/\/api\/v1\/terminals\/[^/]+\/tickets/, (route) => route.fulfill({ json: { ticket: "t" } }));
  const received: string[] = [];
  await page.routeWebSocket(/\/api\/v1\/terminals\/[^/]+\/attach/, (ws) => {
    let offset = 0;
    const output = (text: string) => {
      const bytes = Buffer.from(text);
      const message = Buffer.alloc(9 + bytes.length);
      message[0] = 0x01;
      message.writeBigUInt64LE(BigInt(offset), 1);
      bytes.copy(message, 9);
      offset += bytes.length;
      ws.send(message);
    };
    ws.send(JSON.stringify({ t: "hello", v: 1, role: "observer", cols: 80, rows: 24 }));
    ws.send(JSON.stringify({ t: "snapshot_begin", format: "vt", cols: 80, rows: 24, cut: 0 }));
    ws.send(Buffer.concat([Buffer.from([0x02]), Buffer.from("agent ready\r\n")]));
    ws.send(JSON.stringify({ t: "snapshot_end" }));
    ws.onMessage((message) => {
      if (typeof message === "string") return;
      if (message[0] !== 0x10) return;
      const text = message.subarray(1).toString();
      received.push(text);
      output(text);
    });
  });
  return { session, received };
}

const terminalBar = (page: Page) => page.getByTestId("terminal-bar");
const terminalFocused = (page: Page) => page.evaluate(() => !!document.activeElement?.closest(".terminal-surface"));

test("Ctrl+` and MOD+Shift+T switch between Summary and Terminal, and always leave the terminal", async ({ page }) => {
  const { session, received } = await sessionWithRun(page);
  await page.goto(`/#/sessions/claude-code/${session}`);
  const views = page.getByRole("tablist", { name: "Session view" });
  await expect(views.getByRole("tab", { name: "Summary" })).toHaveAttribute("aria-selected", "true");

  await page.keyboard.press("Control+Backquote");
  await expect(terminalBar(page)).toBeVisible();
  await expect(views.getByRole("tab", { name: "Terminal" })).toHaveAttribute("aria-selected", "true");
  // the terminal takes the summary's place
  await expect(page.getByRole("tabpanel", { name: "Summary" })).toBeHidden();
  await expect(page.locator(".thread-footer")).toBeHidden();
  await expect(page).toHaveURL(/view=terminal/);
  await expect.poll(() => terminalFocused(page)).toBe(true);
  await page.keyboard.type("ls");
  await expect.poll(() => received.join("")).toBe("ls");

  // from inside the terminal, the key leaves it and never reaches the program
  await page.keyboard.press("Control+Backquote");
  await expect(terminalBar(page)).toBeHidden();
  await expect(page).toHaveURL(/view=summary/);
  await expect.poll(() => terminalFocused(page)).toBe(false);

  await page.keyboard.press("ControlOrMeta+Shift+T");
  await expect(terminalBar(page)).toBeVisible();
  await expect.poll(() => terminalFocused(page)).toBe(true);
  await page.keyboard.press("ControlOrMeta+Shift+T");
  await expect(terminalBar(page)).toBeHidden();
  expect(received.join("")).toBe("ls");
});

test("a session keeps its view across a reload, and opens in the last one used", async ({ page }) => {
  const { session } = await sessionWithRun(page);
  await page.goto(`/#/sessions/claude-code/${session}`);
  await page.getByRole("tab", { name: "Terminal" }).click();
  await expect(terminalBar(page)).toBeVisible();

  await page.reload();
  await expect(terminalBar(page)).toBeVisible();
  // without the view in the address, the session opens in the last one
  await page.goto("/#/");
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(terminalBar(page)).toBeVisible();
  await expect(page.getByRole("tab", { name: "Terminal" })).toHaveAttribute("aria-selected", "true");
});

for (const system of ["macOS", "Linux"] as const) {
  test(`keys in a terminal go to the program, except those the app keeps, on ${system}`, async ({ page }) => {
    const platform = system === "macOS" ? "MacIntel" : "Linux x86_64";
    await page.addInitScript((p) => Object.defineProperty(navigator, "platform", { get: () => p }), platform);
    const { session, received } = await sessionWithRun(page);
    await page.goto(`/#/sessions/claude-code/${session}?view=terminal`);
    await expect(terminalBar(page)).toBeVisible();
    await expect.poll(() => terminalFocused(page)).toBe(true);
    const palette = page.getByRole("dialog", { name: "Search" });

    // Esc goes to the agent, and the page stays
    await page.keyboard.press("Escape");
    await expect.poll(() => received.join("")).toBe("\x1b");
    await expect(terminalBar(page)).toBeVisible();

    // Ctrl+K is the program's on both systems, though it searches on Linux
    // elsewhere
    await page.keyboard.press("Control+k");
    await expect.poll(() => received.join("")).toBe("\x1b\x0b");
    await expect(palette).toBeHidden();

    // a Ctrl combination the terminal sends nothing for stays in it too,
    // though it goes to the inbox on Linux elsewhere
    await page.keyboard.press("Control+1");
    await page.waitForTimeout(300);
    await expect(terminalBar(page)).toBeVisible();
    await expect(page).toHaveURL(new RegExp(session));

    // the app keeps Cmd on macOS, and Ctrl+Shift on Linux
    await page.keyboard.press(system === "macOS" ? "Meta+k" : "Control+Shift+K");
    await expect(palette).toBeVisible();
    expect(received.join("")).toBe("\x1b\x0b");
  });
}
