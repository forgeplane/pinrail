import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { listPayload, startWaiter, submitListReview, tmpFile, wicket, wicketJson } from "../helpers/wicket";

const decision = { decisions: [{ id: 1, action: "accept" }, { id: 2, action: "reject", note: "no" }], undecided: [] };

test("submit --wait blocks until a decision and writes the decision file", async () => {
  const out = path.join(path.dirname(tmpFile("x", "")), "mr-42.decisions.json");
  const waiter = submitListReview("review A", ["--decision-out", out]);
  const id = await waiter.reviewId;

  expect(wicketJson(["show", id]).status).toBe("pending");
  wicketJson(["decide", id, "--data", tmpFile("d.json", JSON.stringify(decision)), "--note", "ship it"]);

  const result = await waiter.done;
  expect(result.code, result.stderr).toBe(0);
  const envelope = JSON.parse(result.stdout);
  expect(envelope.status).toBe("decided");
  expect(envelope.agent_note).toBe("ship it");
  expect(envelope.payload).toEqual(listPayload);
  expect(JSON.parse(fs.readFileSync(out, "utf8"))).toEqual(decision);
  expect(fs.readFileSync(out, "utf8").endsWith("\n")).toBe(true);
});

test("two pending reviews decided in reverse order each wake their own waiter", async () => {
  const a = submitListReview("review B1");
  const b = submitListReview("review B2");
  const [idA, idB] = await Promise.all([a.reviewId, b.reviewId]);

  const pending = wicketJson(["list", "--status", "pending", "--ref", "42"]).map((g: any) => g.id);
  expect(pending).toEqual(expect.arrayContaining([idA, idB]));

  wicketJson(["decide", idB, "--data", tmpFile("d.json", JSON.stringify(decision))]);
  const resB = await b.done;
  expect(resB.code).toBe(0);
  expect(JSON.parse(resB.stdout).id).toBe(idB);
  expect(wicketJson(["show", idA]).status).toBe("pending");

  wicketJson(["decide", idA, "--data", tmpFile("d.json", JSON.stringify(decision))]);
  const resA = await a.done;
  expect(resA.code).toBe(0);
  expect(JSON.parse(resA.stdout).id).toBe(idA);
});

test("withdraw unblocks a waiter with exit 3", async () => {
  const waiter = submitListReview("review C");
  const id = await waiter.reviewId;
  wicketJson(["withdraw", id]);
  const result = await waiter.done;
  expect(result.code).toBe(3);
  expect(result.stderr).toContain("was withdrawn");
  expect(JSON.parse(result.stdout).status).toBe("withdrawn");
});

test("discard unblocks a waiter with exit 5 and the reason", async () => {
  const waiter = submitListReview("review C2");
  const id = await waiter.reviewId;
  const discarded = wicketJson(["discard", id, "--reason", "not now", "--by", "pat"]);
  expect(discarded.status).toBe("discarded");
  expect(discarded.discarded_by).toBe("pat");
  const result = await waiter.done;
  expect(result.code).toBe(5);
  expect(result.stderr).toContain("discarded by pat: not now");
  expect(JSON.parse(result.stdout).discarded_reason).toBe("not now");
});

test("wait times out with exit 4 and the review stays pending", async () => {
  const review = wicketJson(["submit", "list", "--title", "review D", "--data", tmpFile("p.json", JSON.stringify(listPayload))]);
  const result = await startWaiter(["wait", review.id, "--timeout", "1"]).done;
  expect(result.code).toBe(4);
  expect(result.stdout).toBe("");
  expect(wicketJson(["show", review.id]).status).toBe("pending");
});

test("a refused request exits 2 with the violations on stderr", async () => {
  const r = wicket(["submit", "list", "--title", "bad", "--data", tmpFile("p.json", '{"intro": 1}')]);
  expect(r.code).toBe(2);
  expect(r.stdout).toBe("");
  const body = JSON.parse(r.stderr.replace(/^wicket: /, ""));
  expect(body.error).toBe("invalid");
  expect(body.violations.map((v: any) => v.path)).toEqual(["/payload", "/payload/intro"]);

  const bad = wicket(["decide", "r_nope", "--data", tmpFile("d.json", "{}")]);
  expect(bad.code).toBe(2);
  expect(bad.stderr).toContain("not_found");
});

test("--format markdown prints the decision as prose, and the decision file stays JSON", async () => {
  const waiter = submitListReview("markdown please", ["--format", "markdown", "--decision-out", tmpFile("d.json", "")]);
  const id = await waiter.reviewId;
  wicketJson(["decide", id, "--data", tmpFile("d.json", JSON.stringify({ decisions: [{ id: 1, action: "accept" }, { id: 2, action: "reject", note: "typo is fine" }], undecided: [] })), "--note", "ship it"]);
  const result = await waiter.done;
  expect(result.code).toBe(0);
  expect(result.stdout).toMatch(/^# markdown please\n\nlist · acme · review · 42\nDecided by .* · 1 accepted, 1 rejected\n\n> ship it\n\n## Decisions\n\n- \*\*#1\*\* \*\*accepted\*\*\n- \*\*#2\*\* \*\*rejected\*\*\n  > typo is fine\n$/);

  const shown = wicket(["show", id, "--format", "md"]);
  expect(shown.code).toBe(0);
  expect(shown.stdout.startsWith("# markdown please")).toBe(true);
  const env = wicket(["show", id]);
  expect(JSON.parse(env.stdout).status).toBe("decided");
});

test("plugins install places a copy in the store, and a link serves the folder live", async () => {
  const hello = path.resolve(__dirname, "../../plugins/hello");
  const installed = wicketJson(["plugins", "install", hello]);
  expect(installed.name).toBe("hello");
  expect(installed.release).toBe("1.0.0");
  expect(installed.install.linked).toBe(false);
  expect(installed.install.hash).toMatch(/^[0-9a-f]{64}$/);
  expect(installed.path).toMatch(/plugins\/store\/hello\/1$/);
  expect(fs.existsSync(path.join(installed.path, "manifest.json"))).toBe(true);

  const linked = wicketJson(["plugins", "install", hello, "--link"]);
  expect(linked.install.linked).toBe(true);
  expect(linked.path).toBe(hello);

  const refused = wicket(["plugins", "install", path.resolve(__dirname, "..")]);
  expect(refused.code).toBe(2);
  expect(refused.stderr).toContain("not a plugin");
});

test("plugins update says when there is nothing new, and remove drops the record", async () => {
  const hello = path.resolve(__dirname, "../../plugins/hello");
  const installed = wicketJson(["plugins", "install", hello]);
  expect(installed.install.linked).toBe(false);

  const same = wicketJson(["plugins", "update", "hello"]);
  expect(same.state).toBe("up_to_date");
  expect(same.version).toBe("1.0.0");

  const linked = wicketJson(["plugins", "install", hello, "--link"]);
  expect(linked.install.linked).toBe(true);
  const refused = wicket(["plugins", "update", "hello"]);
  expect(refused.code).toBe(2);
  expect(refused.stderr).toContain("is a link");

  const removed = wicketJson(["plugins", "remove", "hello"]);
  expect(removed.removed).toBe("hello");
  expect(removed.linked).toBe(true);
  const names = wicketJson(["plugins"]).plugins.map((p: any) => p.name);
  expect(names).not.toContain("hello");
  const gone = wicket(["plugins", "remove", "hello"]);
  expect(gone.code).toBe(2);

  // the sample stays registered for the tests after this one
  wicketJson(["plugins", "install", hello, "--link"]);
});

test("a plugin wicket-plugin create wrote installs as a link and decides a review", async () => {
  const bin = path.resolve(__dirname, "../../wicket-plugin/bin/wicket-plugin.mjs");
  const dir = path.join(path.dirname(tmpFile("x", "")), "triage");
  execFileSync(process.execPath, [bin, "create", "triage", "--dir", dir], { stdio: "pipe" });

  const linked = wicketJson(["plugins", "install", dir, "--link"]);
  expect(linked.name).toBe("triage");
  expect(linked.release).toBe("0.1.0");
  expect(wicketJson(["plugins"]).plugins.find((p: any) => p.name === "triage").usable).toBe(true);

  const payload = tmpFile("payload.json", JSON.stringify({ message: "Push it?" }));
  const created = wicketJson(["create", "triage", "--title", "Push the branch?", "--data", payload]);
  wicketJson(["decide", created.id, "--data", tmpFile("d.json", JSON.stringify({ ok: true, comment: "go" }))]);
  const shown = wicketJson(["show", created.id]);
  expect(shown.status).toBe("decided");
  expect(shown.decision.data).toEqual({ ok: true, comment: "go" });

  const refused = wicket(["create", "triage", "--title", "Bad", "--data", tmpFile("bad.json", JSON.stringify({ msg: 1 }))]);
  expect(refused.code).not.toBe(0);

  wicketJson(["plugins", "remove", "triage"]);
});

test("plugins lists the built-in and the installed sample plugins", async () => {
  const plugins = wicketJson(["plugins"]);
  // the samples this suite installs, and the built-in that is always there
  expect(plugins.plugins.map((p: any) => p.name)).toEqual(["email", "hello", "list", "review"]);
  // a plugin only installs if it loads, so every one of them is usable
  for (const p of plugins.plugins) expect(p.usable, `${p.name}: ${p.error}`).toBe(true);
});
