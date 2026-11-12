// Work items from the CLI, against the real core: the agent's loop from a
// breakdown to done, keys and numbers inside and outside a checkout, files,
// comments that reach the agent, and reviews that belong to an item.

import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { pinrail, pinrailJson, tmpFile, type Run } from "../helpers/pinrail";
import { scratch } from "../helpers/scratch";
import { loadState } from "../helpers/state";

/** A git checkout of its own repository, so that its items number from 1. */
function checkout(): { dir: string; repo: string } {
  const name = `items-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
  const dir = scratch("pinrail-items-");
  execFileSync("git", ["init", "-q"], { cwd: dir });
  execFileSync("git", ["remote", "add", "origin", `git@github.com:e2e/${name}.git`], { cwd: dir });
  return { dir, repo: `e2e/${name}` };
}

/** An agent's session, as Claude Code sets it. */
const session = (id: string) => ({ CLAUDECODE: "1", CLAUDE_CODE_SESSION_ID: id });

function ok(r: Run): Run {
  expect(r.code, r.stderr).toBe(0);
  return r;
}

/** A request as the app's window makes it: the person's. */
async function asPerson(method: string, apiPath: string, body?: unknown) {
  const response = await fetch(`${loadState().url}${apiPath}`, {
    method,
    headers: { origin: "tauri://localhost", "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  expect(response.ok, await response.clone().text()).toBe(true);
  return response.status === 204 ? null : response.json();
}

test("an agent works through a breakdown from next to done", async () => {
  const { dir, repo } = checkout();
  const env = session("loop-1");
  const plan = tmpFile(
    "plan.json",
    JSON.stringify({
      items: [
        { ref: "themes", title: "Custom themes" },
        { ref: "format", parent: "themes", title: "Theme file format" },
        { ref: "load", parent: "themes", title: "Load themes", depends_on: ["format"] },
        { ref: "picker", parent: "themes", title: "Theme picker", depends_on: ["load"] },
      ],
    }),
  );
  const added = ok(pinrail(["items", "add", "--file", plan, "--markdown"], { cwd: dir, env }));
  const keys = [...added.stdout.matchAll(/: ([A-Z]+-\d+) /g)].map((m) => m[1]);
  expect(keys).toHaveLength(4);
  const [themes, format, load, picker] = keys;
  expect(added.stdout).toContain(`Added 4 work items in ${repo}:`);
  expect(added.stdout).toContain(`- load: ${load} Load themes · depends on ${format} · part of ${themes}`);

  // the first ready item, claimed
  const first = ok(pinrail(["items", "next", "--claim", "--markdown"], { cwd: dir, env }));
  expect(first.stdout.split("\n")[0]).toBe(`${format} Theme file format`);
  expect(first.stdout).toContain("in progress · claimed by claude-code (session loop-1)");
  expect(first.stdout).toContain(`You hold ${format} now`);
  const closed = ok(
    pinrail(["items", "done", format, "--comment", "TOML with a [colors] table.", "--markdown"], { cwd: dir, env }),
  );
  expect(closed.stdout).toBe(`Done: ${format} Theme file format.\nNow ready: ${load} Load themes\n`);

  // the next reads what the one before it left
  const second = ok(pinrail(["items", "next", "--claim", "--markdown"], { cwd: dir, env }));
  expect(second.stdout.split("\n")[0]).toBe(`${load} Load themes`);
  expect(second.stdout).toContain(`- ${format} Theme file format, done by claude-code: "TOML with a [colors] table."`);
  expect(second.stdout).toContain(`part of ${themes} Custom themes (1 of 3 done)`);

  // a review sent while it holds the item belongs to it
  const data = tmpFile("payload.json", JSON.stringify({ groups: [{ title: "g", items: [{ id: 1, title: "x" }] }] }));
  const review = pinrailJson(["submit", "list", "--title", "Theme loader", "--data", data], { cwd: dir, env });
  expect(review.item.key).toBe(load);

  // not done while the review waits
  const refused = pinrail(["items", "done", load, "--comment", "Loader in core", "--markdown"], { cwd: dir, env });
  expect(refused.code).toBe(2);
  expect(refused.stderr).toContain(`${load} is not done: "Theme loader" (${review.id}) still waits on the person`);
  expect(refused.stderr).toContain("pinrail wait <id>");
  ok(
    pinrail(
      [
        "decide",
        review.id,
        "--data",
        tmpFile("d.json", JSON.stringify({ decisions: [{ id: 1, action: "accept" }], undecided: [] })),
      ],
      { cwd: dir },
    ),
  );
  const done = ok(pinrail(["items", "done", load, "--comment", "Loader in core", "--markdown"], { cwd: dir, env }));
  expect(done.stdout).toBe(`Done: ${load} Load themes.\nNow ready: ${picker} Theme picker\n`);
  const shown = ok(pinrail(["items", "show", load, "--markdown"], { cwd: dir }));
  expect(shown.stdout).toContain(`- ${review.id} "Theme loader" (list), decided`);

  // a cycle is refused, with the circle
  const cycle = pinrail(["items", "edit", format, "--depends-on", picker, "--markdown"], { cwd: dir, env });
  expect(cycle.code).toBe(2);
  expect(cycle.stderr).toContain(
    `this would make ${format} wait for itself: ${format} → ${picker} → ${load} → ${format}`,
  );

  // the last sub-item done leaves its parent open, and says so
  ok(pinrail(["items", "edit", picker, "--claim"], { cwd: dir, env }));
  const last = ok(pinrail(["items", "done", picker, "--markdown"], { cwd: dir, env }));
  expect(last.stdout).toContain(`${themes} Custom themes has no open sub-item left but stays open`);
  const parent = pinrailJson(["items", "show", themes], { cwd: dir });
  expect(parent.status).toBe("open");
  expect(parent.ready).toBe(true);
});

test("an item is named by its number inside its checkout and by its key anywhere", async () => {
  const { dir, repo } = checkout();
  const env = session("keys-1");
  const added = pinrailJson(["items", "add", "Numbered"], { cwd: dir, env });
  expect(added.repo).toBe(repo);
  expect(added.number).toBe(1);
  expect(pinrailJson(["items", "show", "1"], { cwd: dir }).key).toBe(added.key);
  expect(pinrailJson(["items", "show", "#1"], { cwd: dir }).key).toBe(added.key);
  // outside the checkout, the key, or the number with --repo
  expect(pinrailJson(["items", "show", added.key]).id).toBe(added.id);
  expect(pinrailJson(["items", "show", "1", "--repo", repo]).id).toBe(added.id);
  const alone = pinrail(["items", "show", "1", "--markdown"]);
  expect(alone.code).toBe(2);
  expect(alone.stderr).toContain("give the item's key");
});

test("outside a checkout, adding and taking the next item need --repo", async () => {
  const repo = `e2e/elsewhere-${Date.now()}`;
  const env = session("repo-1");
  const missing = pinrail(["items", "add", "No repository", "--markdown"], { env });
  expect(missing.code).toBe(1);
  expect(missing.stderr).toContain("--repo");
  expect(pinrail(["items", "next", "--markdown"], { env }).code).toBe(1);
  const added = pinrailJson(["items", "add", "Launch checklist", "--repo", repo], { env });
  expect(added.repo).toBe(repo);
  expect(pinrailJson(["items", "next", "--repo", repo], { env }).item.key).toBe(added.key);
  // list outside a checkout shows every repository, with each item's
  const listed = ok(pinrail(["items", "list", "--markdown"], { env }));
  expect(listed.stdout).toContain(`- ${added.key} Launch checklist · open · ready · ${repo}`);
});

test("a description comes inline, from a file or from stdin, and shows its files", async () => {
  const { dir } = checkout();
  const env = session("body-1");
  const inline = pinrailJson(["items", "add", "Inline", "--body", "Said here."], { cwd: dir, env });
  expect(inline.body).toBe("Said here.");
  const file = tmpFile("body.md", "## Goal\n\nFrom a file.\n");
  expect(pinrailJson(["items", "add", "From a file", "--body-file", file], { cwd: dir, env }).body).toBe(
    "## Goal\n\nFrom a file.\n",
  );
  expect(
    pinrailJson(["items", "add", "From stdin", "--body-file", "-"], { cwd: dir, env, input: "Piped in." }).body,
  ).toBe("Piped in.");
  expect(pinrailJson(["items", "add", "Empty"], { cwd: dir, env }).body).toBe("");

  const shot = path.join(scratch("pinrail-files-"), "dialog.png");
  fs.writeFileSync(shot, "PNG pretend");
  const withFile = pinrailJson(
    ["items", "add", "The dialog", "--body", "![The error](attachment:dialog.png)", "--attach", shot],
    { cwd: dir, env },
  );
  expect(withFile.files.map((f: any) => f.name)).toEqual(["dialog.png"]);
  const shown = ok(pinrail(["items", "show", withFile.key, "--markdown"], { cwd: dir }));
  expect(shown.stdout).toContain("## Files\n- dialog.png (11 bytes)");
  // a description that shows a file it does not carry is refused
  const refused = pinrail(["items", "add", "Missing", "--body", "![x](attachment:none.png)"], { cwd: dir, env });
  expect(refused.code).toBe(2);
});

test("comments come from agents and the person, and the person's reach the agent holding the item", async () => {
  const { dir } = checkout();
  const env = session("comment-1");
  const item = pinrailJson(["items", "add", "Watch the folder"], { cwd: dir, env });
  ok(pinrail(["status", "--start", "Watching the folder"], { cwd: dir, env }));
  ok(pinrail(["items", "edit", item.key, "--claim"], { cwd: dir, env }));

  const said = ok(
    pinrail(["items", "comment", item.key, "Renames are missed on Linux.", "--markdown"], { cwd: dir, env }),
  );
  expect(said.stdout).toBe(`Commented on ${item.key}.\n`);
  await asPerson("POST", `/api/v1/items/${item.key}/comments`, { body: "Poll there every 2 s, then." });

  const status = ok(pinrail(["status", "Back to it"], { cwd: dir, env }));
  expect(status.stdout).toContain(`Comment on ${item.key} Watch the folder:`);
  expect(status.stdout).toContain("Poll there every 2 s, then.");
  // each once
  expect(ok(pinrail(["status", "Still at it"], { cwd: dir, env })).stdout).not.toContain("Poll there");

  const shown = ok(pinrail(["items", "show", item.key, "--markdown"], { cwd: dir }));
  expect(shown.stdout).toContain("- claude-code, ");
  expect(shown.stdout).toContain(": Renames are missed on Linux.");
  expect(shown.stdout).toContain("- the person, ");
  expect(shown.stdout).toContain("## Sessions\n- claude-code comment-1");
});

test("reviews and questions name their item, or none, and the review list narrows to one item", async () => {
  const { dir } = checkout();
  const env = session("reviews-1");
  const one = pinrailJson(["items", "add", "One"], { cwd: dir, env });
  const two = pinrailJson(["items", "add", "Two"], { cwd: dir, env });
  const data = tmpFile("payload.json", JSON.stringify({ groups: [{ title: "g", items: [{ id: 1, title: "x" }] }] }));
  const named = pinrailJson(["submit", "list", "--title", "For two", "--data", data, "--item", "2"], { cwd: dir, env });
  expect(named.item.key).toBe(two.key);
  ok(pinrail(["items", "edit", one.key, "--claim"], { cwd: dir, env }));
  const held = pinrailJson(["submit", "list", "--title", "For one", "--data", data], { cwd: dir, env });
  expect(held.item.key).toBe(one.key);
  const none = pinrailJson(["submit", "list", "--title", "For none", "--data", data, "--no-item"], { cwd: dir, env });
  expect(none.item).toBeNull();
  const listed = pinrailJson(["list", "--item", one.key, "--all"], { cwd: dir });
  expect(listed.map((r: any) => r.title)).toEqual(["For one"]);
  // a question names its item too; it waits for the answer, so it times out
  const asked = pinrail(["ask", "Watch or poll?", "--item", two.key, "--timeout", "1"], { cwd: dir, env });
  expect(asked.code, asked.stderr).toBe(4);
  const forTwo = pinrailJson(["list", "--item", two.key, "--all"], { cwd: dir });
  expect(forTwo.map((r: any) => r.title).sort()).toEqual(["For two", "Watch or poll?"]);
});

test("the JSON of each command is the app's", async () => {
  const { dir } = checkout();
  const env = session("json-1");
  const added = pinrailJson(["items", "add", "Plain"], { cwd: dir, env });
  expect(Object.keys(added)).toEqual(expect.arrayContaining(["id", "key", "status", "timeline", "depends_on"]));
  const listed = pinrailJson(["items", "list"], { cwd: dir });
  expect(listed.items.map((i: any) => i.key)).toContain(added.key);
  const next = pinrailJson(["items", "next"], { cwd: dir, env });
  expect(next.item.key).toBe(added.key);
  const done = pinrailJson(["items", "done", added.key, "--comment", "Nothing to it"], { cwd: dir, env });
  expect(done.item.status).toBe("done");
  expect(done.now_ready).toEqual([]);
});

/** The commands of a page's first `sh` block of work items, each as its arguments,
 *  with `PIN-42` standing for `key`. */
function examples(page: string, key: string): string[][] {
  const text = fs.readFileSync(path.join(__dirname, "..", "..", page), "utf8");
  const block =
    [...text.matchAll(/```sh\n([\s\S]*?)```/g)].map((m) => m[1]).find((b) => b.includes("pinrail items ")) ?? "";
  return block
    .split("\n")
    .map((line) => line.replace(/\s+#.*$/, "").trim())
    .filter((line) => line.startsWith("pinrail items "))
    .map((line) =>
      [...line.slice("pinrail ".length).matchAll(/"([^"]*)"|(\S+)/g)].map((m) => (m[1] ?? m[2]).replace("PIN-42", key)),
    );
}

for (const page of ["docs/using/work-items.md", "skill/SKILL.md"]) {
  test(`the examples in ${page} run as written`, async () => {
    const { dir } = checkout();
    const env = session(`examples-${page.length}`);
    const added = pinrailJson(["items", "add", "Fix the cache"], { cwd: dir, env });
    const commands = examples(page, added.key);
    expect(commands.map((c) => c[1])).toEqual(expect.arrayContaining(["next", "show", "done"]));
    for (const args of commands) ok(pinrail(args, { cwd: dir, env }));
    const item = pinrailJson(["items", "show", added.key], { cwd: dir });
    expect(item.status).toBe("done");
    // the closing comment says what was done
    const comments = item.timeline.filter((e: { type: string }) => e.type === "comment");
    expect(comments.at(-1).body).toBe(commands.find((c) => c[1] === "done")?.at(-1));
  });
}
