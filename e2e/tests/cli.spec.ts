import { expect, test } from "@playwright/test";
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

test("plugins lists the built-in and the registered sample plugins", async () => {
  const plugins = wicketJson(["plugins"]);
  expect(plugins.plugins.map((p: any) => p.name)).toEqual(["email", "hello", "list", "review"]);
  expect(plugins.plugins.every((p: any) => p.usable)).toBe(true);
});
