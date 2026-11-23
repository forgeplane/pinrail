// Resume, for a session whose agent stopped. The shell's core has no
// terminal host, so the runs and the resume itself are stood in for; the
// session and what its hooks reported are the real core's.

import { expect, test, type Page } from "../fixtures";
import { core, sessionId } from "../helpers";

let n = 0;
const fresh = () => `s-resume-${Date.now()}-${++n}`;

test.beforeEach(async ({ page }) => {
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: true } } });
});
test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: false } } });
});

/** A session of `agent` whose hooks reported that it started and then
 *  ended, so that it is stopped. */
async function stoppedSession(page: Page, agent = "claude-code") {
  const session = fresh();
  for (const event of ["started", "ended"]) {
    const response = await page.request.post(`${core}/api/v1/sessions/${agent}/${session}/reports`, {
      data: { event, at: new Date().toISOString(), origin: { folder: "/work/api" } },
    });
    expect(response.status(), await response.text()).toBeLessThan(300);
  }
  return sessionId(page.request, agent, session);
}

/** A session of `agent` whose hooks report that it works, in a terminal
 *  outside Pinrail. */
async function runningSession(page: Page, agent = "claude-code") {
  const session = fresh();
  for (const event of ["started", "prompt"]) {
    const response = await page.request.post(`${core}/api/v1/sessions/${agent}/${session}/reports`, {
      data: { event, at: new Date().toISOString(), origin: { folder: "/work/api" } },
    });
    expect(response.status(), await response.text()).toBeLessThan(300);
  }
  return sessionId(page.request, agent, session);
}

function run(session: string, fields: Record<string, unknown>) {
  return {
    id: "run_1",
    host: "local",
    agent: "claude-code",
    kind: "start",
    state: "ended",
    terminal: "local:t_1",
    folder: "/work/api",
    started_at: new Date(Date.now() - 60_000).toISOString(),
    ended_at: new Date().toISOString(),
    end_reason: "exited",
    exit_code: 0,
    exit_signal: null,
    session: { agent: "claude-code", id: session, agent_session: `agent-${session}` },
    ...fields,
  };
}

/** Stands in for the runs, the ended run's screen and the resume, which
 *  answers with `answer`; returns what was posted to resume. */
async function standIn(
  page: Page,
  session: string,
  runs: Record<string, unknown>[],
  answer?: { status: number; json: unknown },
) {
  const posted: Record<string, unknown>[] = [];
  let listed = runs;
  await page.route(/\/api\/v1\/runs(\?|$)/, (route) => route.fulfill({ json: { runs: listed } }));
  await page.route(/\/api\/v1\/runs\/[^/]+\/screen/, (route) =>
    route.fulfill({ json: { paint: "agent done\r\n", cols: 80, rows: 24 } }),
  );
  await page.route(/\/api\/v1\/runs\/[^/]+\/output/, (route) =>
    route.fulfill({ json: { text: "Error: no conversation found" } }),
  );
  await page.route(/\/api\/v1\/runs\/[^/]+$/, (route) => route.fulfill({ json: runs[0] }));
  await page.route(/\/api\/v1\/terminals\/[^/]+\/tickets/, (route) => route.fulfill({ json: { ticket: "t" } }));
  await page.routeWebSocket(/\/api\/v1\/terminals\/[^/]+\/attach/, (ws) => {
    ws.send(JSON.stringify({ t: "hello", v: 1, role: "observer", cols: 80, rows: 24 }));
    ws.send(JSON.stringify({ t: "snapshot_begin", format: "vt", cols: 80, rows: 24, cut: 0 }));
    ws.send(Buffer.concat([Buffer.from([0x02]), Buffer.from("resumed\r\n")]));
    ws.send(JSON.stringify({ t: "snapshot_end" }));
  });
  await page.route(new RegExp(`/api/v1/sessions/[^/]+/${session}/resume$`), async (route) => {
    posted.push(route.request().postDataJSON());
    const resumed = run(session, {
      id: "run_2",
      kind: "resume",
      state: "running",
      terminal: "local:t_2",
      ended_at: null,
      end_reason: null,
      exit_code: null,
    });
    if (!answer || answer.status === 201) listed = [resumed, ...runs];
    return route.fulfill(answer ?? { status: 201, json: { run: resumed, session: resumed.session } });
  });
  return posted;
}

const header = (page: Page) => page.getByTestId("thread").locator("header").first();

test("Resume in the header resumes a stopped session and shows its terminal", async ({ page }) => {
  const session = await stoppedSession(page);
  const posted = await standIn(page, session, [run(session, {})]);
  await page.goto(`/#/sessions/claude-code/${session}?view=summary`);

  const resume = header(page).getByRole("button", { name: "Resume Claude Code" });
  // one line, wide enough for its label and icon
  const box = (await resume.boundingBox())!;
  expect(box.height).toBeLessThanOrEqual(26);
  expect(box.width).toBeGreaterThan(box.height * 2);
  await resume.click();
  await expect(page.getByTestId("terminal-bar")).toBeVisible();
  const stop = header(page).getByRole("button", { name: "Stop Claude Code" });
  await expect(stop).toBeVisible();
  // Stop too has room for its icon and its label
  const stopBox = (await stop.boundingBox())!;
  expect(stopBox.width).toBeGreaterThan(stopBox.height * 2);
  await expect(header(page).getByRole("button", { name: "Resume Claude Code" })).toHaveCount(0);
  expect(posted).toHaveLength(1);
  expect(posted[0]).toMatchObject({ cols: expect.any(Number), rows: expect.any(Number) });
  expect(posted[0].local_id).toEqual(expect.any(String));
});

test("the ended run's screen offers Resume", async ({ page }) => {
  const session = await stoppedSession(page);
  const posted = await standIn(page, session, [run(session, {})]);
  await page.goto(`/#/sessions/claude-code/${session}?view=terminal`);

  const ended = page.getByTestId("ended-run");
  await expect(ended.getByRole("heading", { name: "Claude Code exited with code 0" })).toBeVisible();
  // the header leaves Resume to the ended run
  await expect(header(page).getByRole("button", { name: "Resume Claude Code" })).toHaveCount(0);
  await ended.getByRole("button", { name: "Resume" }).click();
  await expect(page.getByTestId("terminal-bar")).toBeVisible();
  expect(posted).toHaveLength(1);
});

test("Resume is offered only for a stopped session whose agent can resume", async ({ page }) => {
  // the agent runs
  const running = await stoppedSession(page);
  await standIn(page, running, [run(running, { state: "running", ended_at: null, end_reason: null })]);
  await page.goto(`/#/sessions/claude-code/${running}?view=summary`);
  await expect(header(page).getByRole("button", { name: "Stop Claude Code" })).toBeVisible();
  await expect(header(page).getByRole("button", { name: /^Resume/ })).toHaveCount(0);
  await page.unrouteAll({ behavior: "ignoreErrors" });

  // Pinrail cannot resume Gemini CLI
  const gemini = await stoppedSession(page, "gemini");
  await standIn(page, gemini, []);
  await page.goto(`/#/sessions/gemini/${gemini}?view=summary`);
  await expect(page.getByTestId("thread")).toBeVisible();
  await expect(header(page).getByRole("button", { name: /^Resume/ })).toHaveCount(0);
});

test("a resume that cannot happen says why", async ({ page }) => {
  const session = await stoppedSession(page);
  await standIn(page, session, [run(session, {})], {
    status: 422,
    json: { error: "folder_missing", message: "/work/api does not exist", violations: [] },
  });
  await page.goto(`/#/sessions/claude-code/${session}?view=summary`);
  await header(page).getByRole("button", { name: "Resume Claude Code" }).click();
  await expect(page.getByRole("alert")).toHaveText(
    "The folder /work/api no longer exists, so the session cannot be resumed here.",
  );

  await page.unrouteAll({ behavior: "ignoreErrors" });
  await standIn(page, session, [run(session, {})], {
    status: 409,
    json: {
      error: "already_running",
      message: "This session is already running in another terminal.",
      violations: [],
      run: null,
    },
  });
  await page.reload();
  await header(page).getByRole("button", { name: "Resume Claude Code" }).click();
  await expect(page.getByRole("alert")).toHaveText("This session is already running in another terminal.");
});

test("a session that ran outside Pinrail offers Resume in Pinrail in its terminal view", async ({ page }) => {
  const session = await stoppedSession(page);
  const posted = await standIn(page, session, []);
  await page.goto(`/#/sessions/claude-code/${session}?view=terminal`);

  const outside = page.getByTestId("outside-run");
  await expect(outside.getByRole("heading", { name: "Claude Code ran in another terminal" })).toBeVisible();
  await expect(outside).toContainText("This session ran in a terminal outside Pinrail.");
  await outside.getByRole("button", { name: "Resume in Pinrail" }).click();
  await expect(page.getByTestId("terminal-bar")).toBeVisible();
  expect(posted).toHaveLength(1);
});

test("a resume that fails at once shows why and offers a new session in its folder", async ({ page }) => {
  const session = await stoppedSession(page);
  const failed = run(session, {
    id: "run_failed",
    kind: "resume",
    started_at: new Date(Date.now() - 1_000).toISOString(),
    end_reason: "resume_failed",
    exit_code: 1,
  });
  await standIn(page, session, [failed]);
  const started: Record<string, unknown>[] = [];
  await page.route(/\/api\/v1\/runs$/, (route) => {
    if (route.request().method() !== "POST") return route.fallback();
    started.push(route.request().postDataJSON());
    return route.fulfill({
      status: 201,
      json: { run: run("s-new", { id: "run_3", state: "running" }), session: { agent: "claude-code", id: "s-new" } },
    });
  });
  await page.goto(`/#/sessions/claude-code/${session}?view=terminal`);

  const panel = page.getByTestId("failed-start");
  await expect(panel).toContainText("Claude Code exited right after resuming (code 1).");
  await expect(panel.getByLabel("Claude Code's last output")).toHaveText("Error: no conversation found");
  await panel.getByRole("button", { name: "Start a new session in this folder" }).click();
  await expect(page).toHaveURL(/\/sessions\/claude-code\/s-new/);
  expect(started[0]).toMatchObject({ agent: "claude-code", folder: "/work/api" });
  expect(started[0].task).toBeUndefined();
});

test("a session running in another terminal opens in Pinrail after a warning", async ({ page }) => {
  const session = await runningSession(page);
  const posted = await standIn(page, session, []);
  await page.goto(`/#/sessions/claude-code/${session}?view=summary`);

  const open = header(page).getByRole("button", { name: "Open Claude Code in Pinrail" });
  // one line, as tall as the header's other buttons
  const box = (await open.boundingBox())!;
  expect(box.height).toBeLessThanOrEqual(26);
  expect(box.width).toBeGreaterThan(box.height * 3);
  await open.click();
  const warning = page.getByRole("dialog", { name: "Open Claude Code in Pinrail?" });
  await expect(warning).toContainText(
    "Claude Code is still running in another terminal. Opening it in Pinrail starts a second Claude Code on this session, and both answer it.",
  );
  await warning.getByRole("button", { name: "Cancel" }).click();
  await expect(warning).toBeHidden();
  expect(posted).toHaveLength(0);

  await open.click();
  await warning.getByRole("button", { name: "Open anyway" }).click();
  await expect(page.getByTestId("terminal-bar")).toBeVisible();
  expect(posted).toHaveLength(1);
  expect(posted[0]).toMatchObject({ anyway: true });
});

test("a session whose hooks never reported opens in Pinrail after a warning that it may still run", async ({
  page,
}) => {
  // only a status, from a session older than its hooks: nothing tells
  // whether it still runs
  const reported = fresh();
  const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${reported}/status`, {
    data: { kind: "start", text: "Make a thumbnail", origin: { folder: "/work/api" } },
  });
  expect(response.status(), await response.text()).toBeLessThan(300);
  const session = await sessionId(page.request, "claude-code", reported);
  const posted = await standIn(page, session, []);
  await page.goto(`/#/sessions/claude-code/${session}?view=terminal`);

  const outside = page.getByTestId("outside-run");
  await expect(outside.getByRole("heading", { name: "Claude Code may run in another terminal" })).toBeVisible();
  await header(page).getByRole("button", { name: "Open Claude Code in Pinrail" }).click();
  const warning = page.getByRole("dialog", { name: "Open Claude Code in Pinrail?" });
  await expect(warning).toContainText("Pinrail cannot tell whether Claude Code still runs in another terminal.");
  await warning.getByRole("button", { name: "Open anyway" }).click();
  await expect(page.getByTestId("terminal-bar")).toBeVisible();
  expect(posted).toHaveLength(1);
  expect(posted[0]).toMatchObject({ anyway: true });
});

test("the Terminal view of a session running in another terminal offers to open it", async ({ page }) => {
  const session = await runningSession(page);
  const posted = await standIn(page, session, []);
  await page.goto(`/#/sessions/claude-code/${session}?view=terminal`);

  const outside = page.getByTestId("outside-run");
  await expect(outside.getByRole("heading", { name: "Claude Code runs in another terminal" })).toBeVisible();
  await expect(outside).toContainText("close the other terminal first");
  // in the middle of the view, as an empty page is
  const [view, button] = [
    await page.locator(".thread-terminal").boundingBox(),
    await outside.getByRole("button", { name: "Open in Pinrail" }).boundingBox(),
  ];
  expect(Math.abs(button!.x + button!.width / 2 - (view!.x + view!.width / 2))).toBeLessThan(4);
  expect(Math.abs(button!.y - (view!.y + view!.height / 2))).toBeLessThan(view!.height / 4);
  await outside.getByRole("button", { name: "Open in Pinrail" }).click();
  await page
    .getByRole("dialog", { name: "Open Claude Code in Pinrail?" })
    .getByRole("button", { name: "Open anyway" })
    .click();
  await expect(page.getByTestId("terminal-bar")).toBeVisible();
  expect(posted[0]).toMatchObject({ anyway: true });
});

test("a Codex session running in another terminal is not offered to open", async ({ page }) => {
  const session = await runningSession(page, "codex");
  await standIn(page, session, []);
  await page.goto(`/#/sessions/codex/${session}?view=summary`);
  await expect(page.getByTestId("thread")).toBeVisible();
  await expect(header(page).getByRole("button", { name: /in Pinrail$/ })).toHaveCount(0);
  await expect(header(page).getByRole("button", { name: /^Resume/ })).toHaveCount(0);
});

test("the Terminal view of a Codex session running in another terminal says how to stop it", async ({ page }) => {
  const session = await runningSession(page, "codex");
  await standIn(page, session, []);
  await page.goto(`/#/sessions/codex/${session}?view=terminal`);

  const outside = page.getByTestId("outside-run");
  await expect(outside.getByRole("heading", { name: "Codex runs in another terminal" })).toBeVisible();
  await expect(outside).toContainText(
    "This session runs in a terminal outside Pinrail, so Pinrail cannot show its terminal or stop it. To stop Codex, use the terminal it runs in. Once it stops, you can resume it in Pinrail.",
  );
  await expect(outside.getByRole("button")).toHaveCount(0);
});

test("the Terminal view of a stopped session that cannot be resumed says so", async ({ page }) => {
  const session = await stoppedSession(page, "goose");
  await standIn(page, session, []);
  await page.goto(`/#/sessions/goose/${session}?view=terminal`);

  const outside = page.getByTestId("outside-run");
  await expect(outside.getByRole("heading", { name: "Goose ran in another terminal" })).toBeVisible();
  await expect(outside).toContainText(
    "This session ran in a terminal outside Pinrail. Pinrail cannot resume Goose sessions.",
  );
  await expect(outside.getByRole("button")).toHaveCount(0);
});

test("the Terminal view of a session known only from its statuses says why it has no Stop", async ({ page }) => {
  const reported = fresh();
  const response = await page.request.post(`${core}/api/v1/sessions/goose/${reported}/status`, {
    data: { kind: "start", text: "Make a thumbnail", origin: { folder: "/work/api" } },
  });
  expect(response.status(), await response.text()).toBeLessThan(300);
  const session = await sessionId(page.request, "goose", reported);
  await standIn(page, session, []);
  await page.goto(`/#/sessions/goose/${session}?view=terminal`);

  const outside = page.getByTestId("outside-run");
  await expect(outside.getByRole("heading", { name: "Goose runs outside Pinrail" })).toBeVisible();
  await expect(outside).toContainText(
    "Pinrail knows this session only from the statuses, questions and reviews that Goose sends. Pinrail did not start it, so it cannot show its terminal or stop it.",
  );
});
