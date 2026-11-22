// A session whose agent Pinrail runs shows its terminal beside the summary.
// The runs list, the ticket and the attach socket are stood in for here, as
// the shell's core has no terminal host: the "program" records what it
// receives and echoes it back.

import { expect, test, type Page } from "../fixtures";
import { core, sessionId, settingsDialog, settingsSection } from "../helpers";

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
    session: {
      agent: "claude-code",
      id: await sessionId(page.request, "claude-code", session),
      agent_session: session,
    },
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
/** Waits until the terminal has the keyboard and its stream is live: what
 *  is typed while it still connects is dropped, as the app means it to be. */
async function terminalReady(page: Page) {
  await expect.poll(() => terminalFocused(page)).toBe(true);
  await expect(terminalBar(page).getByRole("status")).not.toHaveText(/Connecting|Reconnecting/);
}

test("Ctrl+` and MOD+Shift+T switch between Summary and Terminal, and always leave the terminal", async ({ page }) => {
  const { session, received } = await sessionWithRun(page);
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}`);
  const views = page.getByRole("tablist", { name: "Session view" });
  await expect(views.getByRole("tab", { name: "Summary" })).toHaveAttribute("aria-selected", "true");

  await page.keyboard.press("Control+Backquote");
  await expect(terminalBar(page)).toBeVisible();
  await expect(views.getByRole("tab", { name: "Terminal" })).toHaveAttribute("aria-selected", "true");
  // the terminal takes the summary's place, over it
  await expect(page.getByRole("tabpanel", { name: "Summary" })).toBeHidden();
  await expect(page.locator(".thread-main")).toHaveAttribute("aria-hidden", "true");
  await expect(page).toHaveURL(/view=terminal/);
  await terminalReady(page);
  await page.keyboard.type("ls");
  await expect.poll(() => received.join("")).toBe("ls");

  // from inside the terminal, the key leaves it and never reaches the program
  await page.keyboard.press("Control+Backquote");
  await expect(terminalBar(page)).toBeHidden();
  await expect(page).toHaveURL(/view=summary/);
  await expect.poll(() => terminalFocused(page)).toBe(false);

  await page.keyboard.press("ControlOrMeta+Shift+T");
  await expect(terminalBar(page)).toBeVisible();
  await terminalReady(page);
  await page.keyboard.press("ControlOrMeta+Shift+T");
  await expect(terminalBar(page)).toBeHidden();
  expect(received.join("")).toBe("ls");
});

test("the terminal keeps room from the left edge of its view", async ({ page }) => {
  const { session } = await sessionWithRun(page);
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=terminal`);
  const surface = page.locator(".thread-terminal .terminal-surface");
  await expect(surface).toBeVisible();
  const [view, drawn] = [await page.locator(".thread-terminal").boundingBox(), await surface.boundingBox()];
  expect(drawn!.x - view!.x).toBeGreaterThanOrEqual(8);
});

test("Settings › Terminal names the renderer the open terminal uses", async ({ page }) => {
  const { session } = await sessionWithRun(page);
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=terminal`);
  await expect(page.locator(".thread-terminal .terminal-surface")).toBeVisible();
  await page.keyboard.press("ControlOrMeta+,");
  await settingsSection(page, "Terminal").click();
  await expect(settingsDialog(page)).toContainText(/In use: (libghostty|xterm\.js)\./);
});

test("a session opened in the Terminal view shows its summary whole when switched to it", async ({ page }) => {
  const { session } = await sessionWithRun(page);
  const post = async (path: string, data: unknown) => {
    const response = await page.request.post(`${core}/api/v1/${path}`, { data });
    expect(response.status(), await response.text()).toBeLessThan(300);
  };
  const lines = Array.from({ length: 30 }, (_, i) => `Line ${i + 1} of the text.`).join("\n\n");
  await post(`sessions/claude-code/${session}/reports`, { event: "prompt", detail: { prompt: "Why did it fail?" } });
  await post(`sessions/claude-code/${session}/reports`, { event: "turn_ended", end: "done", detail: { reply: lines } });
  await post(`sessions/claude-code/${session}/status`, { kind: "note", text: lines });
  await post("reviews", {
    kind: "question",
    title: "Commit it?",
    requested_by: "claude-code",
    session,
    payload: { questions: [{ id: "q1", question: "Commit it?", answer: "yes-no" }] },
  });

  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=terminal`);
  await expect(terminalBar(page)).toBeVisible();
  await page.waitForTimeout(300);
  await page.getByRole("tab", { name: "Summary" }).click();

  // the comment box on the question has its height and its placeholder
  const comment = page.getByPlaceholder("Add a comment to the agent on these questions…");
  await expect(comment).toBeVisible();
  expect((await comment.boundingBox())!.height).toBeGreaterThan(20);
  const thread = page.getByTestId("thread");
  // the reply and the note folded, as when opened in the Summary view
  await expect(thread.locator(".thread-turn-more")).toHaveCount(1);
  await expect(thread.getByTestId("note").getByRole("button", { name: "Show more" })).toBeVisible();
});

test("a question clicked in the session's details from the Terminal view opens in the summary", async ({ page }) => {
  const { session } = await sessionWithRun(page);
  const post = async (path: string, data: unknown) => {
    const response = await page.request.post(`${core}/api/v1/${path}`, { data });
    expect(response.status(), await response.text()).toBeLessThan(300);
  };
  const { id } = await (
    await page.request.post(`${core}/api/v1/reviews`, {
      data: {
        kind: "question",
        title: "Commit it?",
        requested_by: "claude-code",
        session,
        payload: { questions: [{ id: "q1", question: "Commit it?", answer: "yes-no" }] },
      },
    })
  ).json();
  // enough after the question that it starts out of view
  const lines = Array.from({ length: 40 }, (_, i) => `Line ${i + 1} of the text.`).join("\n\n");
  for (let i = 0; i < 3; i++) {
    await post(`sessions/claude-code/${session}/status`, { kind: "note", text: lines });
  }

  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=terminal`);
  await expect(terminalBar(page)).toBeVisible();
  await page.getByTestId("waiting-item").filter({ hasText: "Commit it?" }).click();

  await expect(page).toHaveURL(/view=summary/);
  await expect(page.locator(`#set-${id}`)).toBeInViewport();
});

test("a session keeps its view across a reload, and opens in the last one used", async ({ page }) => {
  const { session } = await sessionWithRun(page);
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}`);
  await page.getByRole("tab", { name: "Terminal" }).click();
  await expect(terminalBar(page)).toBeVisible();

  await page.reload();
  await expect(terminalBar(page)).toBeVisible();
  // without the view in the address, the session opens in the last one
  await page.goto("/#/");
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}`);
  await expect(terminalBar(page)).toBeVisible();
  await expect(page.getByRole("tab", { name: "Terminal" })).toHaveAttribute("aria-selected", "true");
});

test("the split view shows the summary above the terminal, with a divider that keeps its place", async ({ page }) => {
  const { session, received } = await sessionWithRun(page);
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}`);
  await page.getByRole("tab", { name: "Split view" }).click();
  await expect(page).toHaveURL(/view=split/);

  // both show, the summary above, and the terminal is where the person types
  await expect(page.locator(".thread-items").getByText("Started: Fix the login redirect")).toBeVisible();
  await expect(terminalBar(page)).toBeVisible();
  const [top, bottom] = [await page.locator(".thread-main").boundingBox(), await terminalBar(page).boundingBox()];
  expect(top!.y + top!.height).toBeLessThanOrEqual(bottom!.y);
  await expect(page.getByRole("textbox", { name: "Message to the agent" })).toHaveCount(0);
  await page.locator(".terminal-surface").click();
  await terminalReady(page);
  await page.keyboard.type("ls");
  await expect.poll(() => received.join("")).toBe("ls");

  // the arrow keys move the divider, and its place is kept for every session
  const divider = page.getByRole("separator", { name: "Resize the summary" });
  await expect(divider).toHaveAttribute("aria-valuenow", "50");
  await divider.focus();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect(divider).toHaveAttribute("aria-valuenow", "60");
  const higher = (await page.locator(".thread-main").boundingBox())!.height;
  expect(higher).toBeGreaterThan(top!.height);
  await page.reload();
  await expect(page.getByRole("separator", { name: "Resize the summary" })).toHaveAttribute("aria-valuenow", "60");

  // Ctrl+` leaves the split for the summary
  await page.locator(".terminal-surface").click();
  await page.keyboard.press("Control+`");
  await expect(page.getByRole("tab", { name: "Summary" })).toHaveAttribute("aria-selected", "true");
  await expect(divider).toHaveCount(0);
});

for (const system of ["macOS", "Linux"] as const) {
  test(`keys in a terminal go to the program, except those the app keeps, on ${system}`, async ({ page }) => {
    const platform = system === "macOS" ? "MacIntel" : "Linux x86_64";
    await page.addInitScript((p) => Object.defineProperty(navigator, "platform", { get: () => p }), platform);
    const { session, received } = await sessionWithRun(page);
    await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=terminal`);
    await expect(terminalBar(page)).toBeVisible();
    await terminalReady(page);
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

    // other Ctrl combinations stay in it too, though Ctrl+1 goes to the
    // inbox on Linux elsewhere
    await page.keyboard.press("Control+1");
    await page.waitForTimeout(300);
    await expect(terminalBar(page)).toBeVisible();
    await expect(page).toHaveURL(new RegExp(await sessionId(page.request, "claude-code", session)));

    // the app keeps Cmd on macOS, and Ctrl+Shift on Linux
    const before = received.join("");
    await page.keyboard.press(system === "macOS" ? "Meta+k" : "Control+Shift+K");
    await expect(palette).toBeVisible();
    expect(received.join("")).toBe(before);
  });
}

/** Records the stops the window asks the core for. */
async function stops(page: Page) {
  const asked: string[] = [];
  await page.route(/\/api\/v1\/runs\/run_1\/stop$/, (route) => {
    asked.push(route.request().method());
    return route.fulfill({ status: 202, body: "" });
  });
  return asked;
}

test("Stop stops an idle agent at once, and asks first while it works", async ({ page }) => {
  const { session } = await sessionWithRun(page);
  const asked = await stops(page);
  // its turn finished: the agent waits for the person
  const report = (data: unknown) =>
    page.request.post(`${core}/api/v1/sessions/claude-code/${session}/reports`, { data });
  await report({ event: "prompt", detail: { prompt: "Fix it" } });
  await report({ event: "turn_ended", end: "done", detail: { reply: "Fixed." } });
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}`);
  await expect(page.getByTestId("thread").getByTestId("presence-chip")).toContainText("Finished");
  await page.getByRole("button", { name: "Stop Claude Code" }).click();
  await expect.poll(() => asked.length).toBe(1);

  // a turn begins: the agent works
  await report({ event: "prompt", detail: { prompt: "Fix it again" } });
  await expect(page.getByTestId("thread").getByTestId("presence-chip")).toContainText("Working");
  await page.getByRole("button", { name: "Stop Claude Code" }).click();
  const confirm = page.getByRole("dialog", { name: "Stop Claude Code?" });
  await expect(confirm).toContainText("Claude Code is working on a turn.");
  await confirm.getByRole("button", { name: "Keep it running" }).click();
  expect(asked).toHaveLength(1);
  await page.getByRole("button", { name: "Stop Claude Code" }).click();
  await confirm.getByRole("button", { name: "Stop", exact: true }).click();
  await expect.poll(() => asked.length).toBe(2);
  await expect(confirm).toBeHidden();
});

for (const system of ["macOS", "Linux"] as const) {
  test(`the stop key asks to stop the agent, from the terminal too, on ${system}`, async ({ page }) => {
    const platform = system === "macOS" ? "MacIntel" : "Linux x86_64";
    await page.addInitScript((p) => Object.defineProperty(navigator, "platform", { get: () => p }), platform);
    const { session, received } = await sessionWithRun(page);
    const asked = await stops(page);
    await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=terminal`);
    await terminalReady(page);
    await page.keyboard.press(system === "macOS" ? "Meta+Shift+X" : "Control+Shift+X");
    const confirm = page.getByRole("dialog", { name: "Stop Claude Code?" });
    await expect(confirm).toBeVisible();
    expect(received.join("")).toBe("");
    await confirm.getByRole("button", { name: "Stop", exact: true }).click();
    await expect.poll(() => asked.length).toBe(1);
  });
}

test("the summary shows a question the agent asks in its terminal, and opens the terminal to answer it", async ({
  page,
}) => {
  const { session } = await sessionWithRun(page);
  const report = (data: unknown) =>
    page.request.post(`${core}/api/v1/sessions/claude-code/${session}/reports`, { data });
  await report({ event: "prompt", detail: { prompt: "Ask me something" } });
  await report({ event: "needs_input", wait: "question", detail: { summary: "What would you like to do today?" } });
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=summary`);

  const waiting = page.getByTestId("thread").getByTestId("waiting-line");
  await expect(waiting).toContainText("Claude Code asks");
  await expect(waiting).toContainText("What would you like to do today?");
  await waiting.getByRole("button", { name: "Open the terminal" }).click();
  await expect(page.getByTestId("terminal-bar")).toBeVisible();
});

test("the summary shows what an agent in a terminal of its own waits for, and where to answer it", async ({ page }) => {
  // a session Pinrail knows only from its hooks
  const session = `s-outside-${Date.now()}`;
  const report = (data: unknown) =>
    page.request.post(`${core}/api/v1/sessions/claude-code/${session}/reports`, { data });
  await report({ event: "prompt", detail: { prompt: "Commit it" } });
  await report({ event: "needs_input", wait: "permission", detail: { tool: "Bash", summary: "git commit -F -" } });
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=summary`);

  const waiting = page.getByTestId("thread").getByTestId("waiting-line");
  await expect(waiting).toContainText("Claude Code needs permission");
  await expect(waiting).toContainText("Bash · git commit -F -");
  await expect(waiting).toContainText("Answer it in the terminal where Claude Code runs");
  await expect(waiting.getByRole("button", { name: "Open the terminal" })).toHaveCount(0);

  // allowed there, it goes on, and the line goes
  await report({ event: "working" });
  await expect(page.getByTestId("thread").getByTestId("waiting-line")).toHaveCount(0);
});

test("the summary shows turns, where the run started, and what it waits for", async ({ page }) => {
  const { session } = await sessionWithRun(page);
  const report = (data: unknown) =>
    page.request.post(`${core}/api/v1/sessions/claude-code/${session}/reports`, { data });
  await report({ event: "prompt", detail: { prompt: "Fix the login redirect" } });
  await report({ event: "turn_ended", end: "done", detail: { reply: "Fixed it in auth.ts." } });
  await report({ event: "prompt", detail: { prompt: "Now run the tests" } });
  await report({ event: "needs_input", wait: "permission", detail: { tool: "Bash", summary: "npm test" } });
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=summary`);

  const thread = page.getByTestId("thread");
  await expect(thread.getByTestId("run-line")).toHaveText("Started in Pinrail");
  // the first turn's prompt and its reply, then the second turn's prompt
  const turns = thread.getByTestId("turn");
  await expect(turns).toHaveCount(3);
  // what the person asked, on the right, as their messages are
  const asked = turns.first().getByLabel("You asked");
  await expect(asked).toHaveText("Fix the login redirect");
  const [bubble, items] = [
    await asked.locator(".thread-message-bubble").boundingBox(),
    await thread.locator(".thread-items").boundingBox(),
  ];
  expect(bubble!.x + bubble!.width).toBeGreaterThan(items!.x + items!.width - 80);
  // and what Claude Code replied, under its icon
  const reply = turns.nth(1).getByTestId("turn-reply");
  await expect(reply).toContainText("Claude Code replied");
  await expect(reply).toContainText("Fixed it in auth.ts.");
  // waiting for the person, it is not shown working
  await expect(thread.getByTestId("turn-working")).toHaveCount(0);

  const waiting = thread.getByTestId("waiting-line");
  await expect(waiting).toContainText("Claude Code needs permission");
  await expect(waiting).toContainText("Bash · npm test");
  await waiting.getByRole("button", { name: "Open the terminal" }).click();
  await expect(page.getByTestId("terminal-bar")).toBeVisible();

  // allowed, it works again: Pinrail's mark and what it is doing, at the
  // end of the thread
  await report({ event: "working" });
  await page.getByRole("tab", { name: "Summary" }).click();
  const working = thread.getByRole("status", { name: "Claude Code is working" });
  await expect(working).toHaveText(/…$/);
  await expect(working.locator(".wait-mark")).toBeVisible();
  await expect(thread.getByTestId("waiting-line")).toHaveCount(0);
});

test("the summary says how the run ended, and keeps turns without their texts when told to", async ({ page }) => {
  const session = `s-ended-${Date.now()}`;
  await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: "Fix the login redirect", origin: { repo: "acme/api" } },
  });
  await page.request.patch(`${core}/api/v1/settings`, { data: { runtime: { record_turns: false } } });
  try {
    const report = (data: unknown) =>
      page.request.post(`${core}/api/v1/sessions/claude-code/${session}/reports`, { data });
    await report({ event: "prompt", detail: { prompt: "a secret prompt" } });
    await report({ event: "turn_ended", end: "interrupted", detail: { reply: "a secret reply" } });
    const run = {
      id: "run_9",
      host: "local",
      agent: "claude-code",
      kind: "start",
      state: "ended",
      terminal: "local:t_9",
      folder: "/work",
      started_at: new Date(Date.now() - 60_000).toISOString(),
      ended_at: new Date().toISOString(),
      end_reason: "stopped",
      exit_code: null,
      exit_signal: "HUP",
      session: {
        agent: "claude-code",
        id: await sessionId(page.request, "claude-code", session),
        agent_session: session,
      },
    };
    await page.route(/\/api\/v1\/runs(\?|$)/, (route) => route.fulfill({ json: { runs: [run] } }));
    await page.route(/\/api\/v1\/runs\/run_9\/output/, (route) => route.fulfill({ json: { text: "" } }));
    await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=summary`);

    const thread = page.getByTestId("thread");
    await expect(thread.getByTestId("run-line")).toHaveText(["Started in Pinrail", "Stopped by you"]);
    const turn = thread.getByTestId("turn");
    // how it ended, as a line across the thread, not as Claude Code's message
    const ended = turn.getByTestId("turn-state");
    await expect(ended).toContainText("Claude Code stopped after");
    await expect(ended.locator(".msg-avatar")).toHaveCount(0);
    await expect(thread).not.toContainText("a secret");
  } finally {
    await page.request.patch(`${core}/api/v1/settings`, { data: { runtime: { record_turns: true } } });
  }
});

test("Cmd+F finds text in the terminal, and Escape gives the terminal the keyboard again", async ({ page }) => {
  const { session, received } = await sessionWithRun(page);
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=terminal`);
  await expect(terminalBar(page)).toBeVisible();
  await terminalReady(page);
  await page.keyboard.type("echo ready");
  await expect.poll(() => received.join("")).toBe("echo ready");

  // Cmd+F on macOS, Ctrl+Shift+F elsewhere, as the window reports its platform
  const mac = await page.evaluate(() => /Mac/i.test(navigator.platform));
  await page.keyboard.press(mac ? "Meta+f" : "Control+Shift+f");
  const bar = page.getByRole("search", { name: "Find in the terminal" });
  await expect(bar).toBeVisible();
  await bar.getByRole("textbox").fill("ready");
  await expect(bar.getByRole("status")).toHaveText("2 matches");
  await bar.getByRole("textbox").fill("nowhere");
  await expect(bar.getByRole("status")).toHaveText("No matches");

  await page.keyboard.press("Escape");
  await expect(bar).toHaveCount(0);
  await terminalReady(page);
  // nothing of it reached the program
  expect(received.join("")).toBe("echo ready");
});

test("a link Cmd-clicked in the terminal opens", async ({ page }) => {
  const { session, received } = await sessionWithRun(page);
  // outside the app's window, a link opens in a new tab
  await page.addInitScript(() => {
    const opened: string[] = [];
    Object.assign(window, { opened_: opened });
    window.open = (url?: string | URL) => (opened.push(String(url)), null);
  });
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=terminal`);
  await terminalReady(page);
  await page.keyboard.type("see https://example.com/docs");
  await expect.poll(() => received.join("")).toBe("see https://example.com/docs");

  // the second row, after "see ", with the pointer resting on it first
  const canvas = page.locator(".terminal-host canvas").first();
  const box = (await canvas.boundingBox())!;
  const { cols, rows } = await page.evaluate(() => {
    const text = document.querySelector(".terminal-size")?.textContent ?? "80 × 24";
    const [c, r] = text.split("×").map((n) => Number(n.trim()));
    return { cols: c, rows: r };
  });
  const x = box.x + (box.width / cols) * 8.5;
  const y = box.y + (box.height / rows) * 1.5;
  await page.mouse.move(x, y);
  await page.waitForTimeout(200);
  const mac = await page.evaluate(() => /Mac/i.test(navigator.platform));
  await page.keyboard.down(mac ? "Meta" : "Control");
  await page.mouse.click(x, y);
  await page.keyboard.up(mac ? "Meta" : "Control");
  await expect
    .poll(() => page.evaluate(() => (window as unknown as { opened_: string[] }).opened_))
    .toEqual(["https://example.com/docs"]);
});

test("while a session's runs load, the terminal's place shows that it loads, then the terminal", async ({ page }) => {
  const { session } = await sessionWithRun(page);
  // the runs answer only once let go
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route(/\/api\/v1\/runs\?/, async (route) => {
    await held;
    await route.fallback();
  });
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=terminal`);
  await expect(page.getByTestId("terminal-loading")).toBeVisible();
  await expect(page.getByTestId("outside-run")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Resume|Open in Pinrail/ })).toHaveCount(0);

  release();
  await expect(terminalBar(page)).toBeVisible();
  await expect(page.getByTestId("terminal-loading")).toHaveCount(0);
});

test("a session's menu in the sidebar stops its agent", async ({ page }) => {
  const { session } = await sessionWithRun(page);
  const asked = await stops(page);
  await page.goto("/#/");
  const id = await sessionId(page.request, "claude-code", session);
  const row = page.locator(`[data-testid="sidebar-session"][href$="/${id}"]`);
  await row.click({ button: "right" });
  await page.getByRole("menu").getByRole("menuitem", { name: "Stop Claude Code" }).click();
  await expect.poll(() => asked).toEqual(["POST"]);
});

test("a session whose agent went on in a new session links to it", async ({ page }) => {
  const left = fresh();
  const next = fresh();
  for (const session of [left, next]) {
    const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
      data: { kind: "start", text: `Work in ${session}`, origin: { repo: "acme/api" } },
    });
    expect(response.status(), await response.text()).toBeLessThan(300);
  }
  const nextId = await sessionId(page.request, "claude-code", next);
  // the run was in the session left, and goes on in the next one, as after /clear
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
    session: { agent: "claude-code", id: nextId, agent_session: next },
  };
  await page.route(/\/api\/v1\/runs\?/, (route) => route.fulfill({ json: { runs: [run] } }));
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", left)}`);
  const moved = page.getByTestId("run-moved");
  await expect(moved).toContainText("The agent went on in a new session");
  await moved.getByRole("link", { name: "Open the new session" }).click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${nextId}`));
});

test("the terminal view of a session whose agent went on in a new session links to it", async ({ page }) => {
  // the session left was working when the agent went on, as Claude Code's
  // was before /clear
  const left = fresh();
  for (const event of ["started", "prompt"]) {
    const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${left}/reports`, {
      data: { event, at: new Date().toISOString(), origin: { folder: "/work" } },
    });
    expect(response.status(), await response.text()).toBeLessThan(300);
  }
  const next = fresh();
  const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${next}/status`, {
    data: { kind: "start", text: `Work in ${next}`, origin: { repo: "acme/api" } },
  });
  expect(response.status(), await response.text()).toBeLessThan(300);
  const nextId = await sessionId(page.request, "claude-code", next);
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
    session: { agent: "claude-code", id: nextId, agent_session: next },
  };
  await page.route(/\/api\/v1\/runs\?/, (route) => route.fulfill({ json: { runs: [run] } }));
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", left)}?view=terminal`);

  const moved = page.getByTestId("moved-run");
  await expect(moved).toContainText("Claude Code went on in a new session");
  await expect(page.getByTestId("outside-run")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Open Claude Code in Pinrail" })).toHaveCount(0);
  await moved.getByRole("link", { name: "Open the new session" }).click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${nextId}`));
});
