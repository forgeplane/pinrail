import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { listPayload, startWaiter, submitListReview, tmpFile, pinrail, pinrailJson } from "../helpers/pinrail";

const decision = {
  decisions: [
    { id: 1, action: "accept" },
    { id: 2, action: "reject", note: "no" },
  ],
  undecided: [],
};

test("submit --wait blocks until a decision and writes the decision file", async () => {
  const out = path.join(path.dirname(tmpFile("x", "")), "mr-42.decisions.json");
  const waiter = submitListReview("review A", ["--decision-out", out]);
  const id = await waiter.reviewId;

  expect(pinrailJson(["show", id]).status).toBe("pending");
  pinrailJson(["decide", id, "--data", tmpFile("d.json", JSON.stringify(decision)), "--note", "ship it"]);

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

  const pending = pinrailJson(["list", "--status", "pending", "--ref", "42"]).map((g: any) => g.id);
  expect(pending).toEqual(expect.arrayContaining([idA, idB]));

  pinrailJson(["decide", idB, "--data", tmpFile("d.json", JSON.stringify(decision))]);
  const resB = await b.done;
  expect(resB.code).toBe(0);
  expect(JSON.parse(resB.stdout).id).toBe(idB);
  expect(pinrailJson(["show", idA]).status).toBe("pending");

  pinrailJson(["decide", idA, "--data", tmpFile("d.json", JSON.stringify(decision))]);
  const resA = await a.done;
  expect(resA.code).toBe(0);
  expect(JSON.parse(resA.stdout).id).toBe(idA);
});

test("withdraw unblocks a waiter with exit 3", async () => {
  const waiter = submitListReview("review C");
  const id = await waiter.reviewId;
  pinrailJson(["withdraw", id]);
  const result = await waiter.done;
  expect(result.code).toBe(3);
  expect(result.stderr).toContain("was withdrawn");
  expect(JSON.parse(result.stdout).status).toBe("withdrawn");
});

test("discard unblocks a waiter with exit 5 and the reason", async () => {
  const waiter = submitListReview("review C2");
  const id = await waiter.reviewId;
  const discarded = pinrailJson(["discard", id, "--reason", "not now", "--by", "pat"]);
  expect(discarded.status).toBe("discarded");
  expect(discarded.discarded_by).toBe("pat");
  const result = await waiter.done;
  expect(result.code).toBe(5);
  expect(result.stderr).toContain("discarded by pat: not now");
  expect(JSON.parse(result.stdout).discarded_reason).toBe("not now");
});

test("wait times out with exit 4 and the review stays pending", async () => {
  const review = pinrailJson([
    "submit",
    "list",
    "--title",
    "review D",
    "--data",
    tmpFile("p.json", JSON.stringify(listPayload)),
  ]);
  const result = await startWaiter(["wait", review.id, "--timeout", "1"]).done;
  expect(result.code).toBe(4);
  expect(result.stdout).toBe("");
  expect(pinrailJson(["show", review.id]).status).toBe("pending");
});

test("a refused request exits 2 with the violations on stderr", async () => {
  const r = pinrail(["submit", "list", "--title", "bad", "--data", tmpFile("p.json", '{"intro": 1}')]);
  expect(r.code).toBe(2);
  expect(r.stdout).toBe("");
  const body = JSON.parse(r.stderr.replace(/^pinrail: /, ""));
  expect(body.error).toBe("invalid");
  expect(body.violations.map((v: any) => v.path)).toEqual(["/payload", "/payload/intro"]);

  const bad = pinrail(["decide", "r_nope", "--data", tmpFile("d.json", "{}")]);
  expect(bad.code).toBe(2);
  expect(bad.stderr).toContain("not_found");
});

test("markdown, the default, prints the decision as prose, and the decision file stays JSON", async () => {
  const waiter = submitListReview("markdown please", ["--markdown", "--decision-out", tmpFile("d.json", "")]);
  const id = await waiter.reviewId;
  pinrailJson([
    "decide",
    id,
    "--data",
    tmpFile(
      "d.json",
      JSON.stringify({
        decisions: [
          { id: 1, action: "accept" },
          { id: 2, action: "reject", note: "typo is fine" },
        ],
        undecided: [],
      }),
    ),
    "--note",
    "ship it",
  ]);
  const result = await waiter.done;
  expect(result.code).toBe(0);
  expect(result.stdout).toMatch(
    /^r_\w+ · decided · markdown please\nlist · acme · review · 42 · decided by [^\n]*\noutcome: 1 accepted, 1 rejected\n\n> ship it\n\n## lib\/acme\/tickets\.ex\n\n- \*\*#1 accepted\*\* — do_save dedups without reversing \(major\)\n- \*\*#2 rejected\*\* — moduledoc typo \(minor\)\n {2}> typo is fine\n$/,
  );

  const shown = pinrail(["show", id, "--markdown"]);
  expect(shown.code).toBe(0);
  expect(shown.stdout.startsWith(`${id} · decided · markdown please`)).toBe(true);
  const env = pinrail(["show", id]);
  expect(JSON.parse(env.stdout).status).toBe("decided");
});

test("plugins install stores a bundle, and a link serves the folder live", async () => {
  const hello = path.resolve(__dirname, "../../plugins/hello");
  const installed = pinrailJson(["plugins", "install", hello]);
  expect(installed.name).toBe("hello");
  expect(installed.name).toBe("hello");
  expect(installed.version).toBe("1.0.0");
  expect(installed.install.link).toBe(false);
  expect(installed.install.bundle).toMatch(/^[0-9a-f]{64}$/);
  expect(installed.path).toMatch(new RegExp(`plugins/bundles/${installed.install.bundle}$`));
  expect(fs.existsSync(path.join(installed.path, "manifest.json"))).toBe(true);

  const linked = pinrailJson(["plugins", "install", hello, "--link"]);
  expect(linked.install.link).toBe(true);
  expect(linked.path).toBe(hello);

  const refused = pinrail(["plugins", "install", path.resolve(__dirname, "..")]);
  expect(refused.code).toBe(2);
  expect(refused.stderr).toContain("not a plugin");
});

test("installing again replaces a plugin, and remove drops the record", async () => {
  const hello = path.resolve(__dirname, "../../plugins/hello");
  try {
    const installed = pinrailJson(["plugins", "install", hello]);
    expect(installed.install.link).toBe(false);
    expect(installed.version).toBe("1.0.0");

    const linked = pinrailJson(["plugins", "install", hello, "--link"]);
    expect(linked.install.link).toBe(true);

    const removed = pinrailJson(["plugins", "remove", "hello"]);
    expect(removed.removed).toBe("hello");
    expect(removed.link).toBe(true);
    const names = pinrailJson(["plugins"]).plugins.map((p: any) => p.name);
    expect(names).not.toContain("hello");
    const gone = pinrail(["plugins", "remove", "hello"]);
    expect(gone.code).toBe(2);
  } finally {
    // the sample stays registered for the tests after this one, whatever happened
    pinrail(["plugins", "install", hello, "--link"]);
  }
});

test("a plugin pinrail-plugin create wrote installs as a link and decides a review", async () => {
  try {
    const bin = path.resolve(__dirname, "../../pinrail-plugin/bin/pinrail-plugin.mjs");
    const dir = path.join(path.dirname(tmpFile("x", "")), "triage");
    execFileSync(process.execPath, [bin, "create", "triage", "--dir", dir], { stdio: "pipe" });

    const linked = pinrailJson(["plugins", "install", dir, "--link"]);
    expect(linked.name).toBe("triage");
    expect(linked.version).toBe("0.1.0");
    expect(pinrailJson(["plugins"]).plugins.find((p: any) => p.name === "triage").usable).toBe(true);

    const payload = tmpFile("payload.json", JSON.stringify({ message: "Push it?" }));
    const created = pinrailJson(["submit", "triage", "--title", "Push the branch?", "--data", payload]);
    pinrailJson(["decide", created.id, "--data", tmpFile("d.json", JSON.stringify({ ok: true, comment: "go" }))]);
    const shown = pinrailJson(["show", created.id]);
    expect(shown.status).toBe("decided");
    expect(shown.decision.data).toEqual({ ok: true, comment: "go" });

    const refused = pinrail([
      "submit",
      "triage",
      "--title",
      "Bad",
      "--data",
      tmpFile("bad.json", JSON.stringify({ msg: 1 })),
    ]);
    expect(refused.code).not.toBe(0);
  } finally {
    pinrail(["plugins", "remove", "triage"]);
  }
});

test("files sent with --attach travel with the review and come back byte for byte", async () => {
  try {
    const root = path.join(path.dirname(tmpFile("x", "")), "files-plugin");
    fs.mkdirSync(root, { recursive: true });
    fs.mkdirSync(path.join(root, "view"), { recursive: true });
    fs.writeFileSync(path.join(root, "view", "index.html"), "<html></html>");
    fs.mkdirSync(path.join(root, "schemas"), { recursive: true });
    fs.writeFileSync(path.join(root, "schemas", "payload.schema.json"), "{}");
    fs.writeFileSync(path.join(root, "schemas", "decision.schema.json"), "{}");
    fs.writeFileSync(
      path.join(root, "manifest.json"),
      JSON.stringify({ name: "files", version: "1.0.0", attachments: { accept: [".glb"] } }),
    );
    pinrailJson(["plugins", "install", root, "--link"]);

    // not text: every byte value, so nothing is decoded on the way
    const bytes = Buffer.from(Array.from({ length: 300_000 }, (_, i) => (i * 7) % 256));
    const model = tmpFile("pivot.glb", "");
    fs.writeFileSync(model, bytes);
    const payload = tmpFile("files.json", JSON.stringify({ file: { $attachment: "Pivot lamp.glb" } }));
    const created = pinrailJson([
      "submit",
      "files",
      "--title",
      "One lamp",
      "--data",
      payload,
      "--attach",
      `${model}=Pivot lamp.glb`,
    ]);
    expect(created.attachments).toEqual([
      {
        name: "Pivot lamp.glb",
        size: bytes.length,
        media_type: "model/gltf-binary",
        sha256: expect.stringMatching(/^[0-9a-f]{64}$/),
      },
    ]);
    expect(pinrailJson(["attachments", "list", created.id])).toEqual(created.attachments);

    const saved = path.join(path.dirname(model), "saved.glb");
    const got = pinrail(["attachments", "get", created.id, "Pivot lamp.glb", "-o", saved]);
    expect(got.code, got.stderr).toBe(0);
    expect(fs.readFileSync(saved).equals(bytes)).toBe(true);

    // a reference to a file that was not sent is refused before any upload
    const refused = pinrail([
      "submit",
      "files",
      "--title",
      "Two lamps",
      "--data",
      tmpFile("f2.json", JSON.stringify({ file: { $attachment: "column.glb" } })),
      "--attach",
      model,
    ]);
    expect(refused.code).toBe(2);
    expect(refused.stderr).toContain('no attachment \\"column.glb\\" on this review');
  } finally {
    pinrail(["plugins", "remove", "files"]);
  }
});

test("plugins lists the built-in and the installed sample plugins", async () => {
  const plugins = pinrailJson(["plugins"]);
  // the samples this suite installs, and the built-in ones that are always there
  expect(plugins.plugins.map((p: any) => p.name)).toEqual(
    expect.arrayContaining(["artifact", "email", "feedback", "hello", "list", "review"]),
  );
  // a plugin only installs if it loads, so every one of them is usable
  for (const p of plugins.plugins) expect(p.usable, `${p.name}: ${p.error}`).toBe(true);
  // the three that are developed in place are links; artifact was built and copied
  const kinds = Object.fromEntries(plugins.plugins.map((p: any) => [p.name, p.install?.link]));
  expect(kinds).toMatchObject({ email: true, hello: true, review: true, artifact: false });
});
