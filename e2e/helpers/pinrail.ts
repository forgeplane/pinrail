import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { cliEnv, loadState } from "./state";

export type Run = { code: number | null; stdout: string; stderr: string };

// the CLI runs outside any git checkout, so no review takes this
// repository's origin and stderr carries only what a test expects
const cwd = os.tmpdir();

/** Runs the CLI to completion. */
export function pinrail(args: string[], opts: { input?: string } = {}): Run {
  const state = loadState();
  const r = spawnSync(state.cli, args, { cwd, env: cliEnv(state), encoding: "utf8", input: opts.input });
  return { code: r.status, stdout: r.stdout, stderr: r.stderr };
}

/** Runs the CLI and parses its stdout as JSON, failing loudly otherwise. */
export function pinrailJson(args: string[], opts: { input?: string } = {}): any {
  const r = pinrail(args, opts);
  if (r.code !== 0) throw new Error(`pinrail ${args.join(" ")} exited ${r.code}\n${r.stderr}`);
  return JSON.parse(r.stdout);
}

export type Waiter = {
  proc: ChildProcess;
  /** the review id, parsed from the "review r_…: url" line on stderr */
  reviewId: Promise<string>;
  done: Promise<Run>;
};

/** Starts `pinrail submit … --wait` (or `pinrail wait`) in the background. */
export function startWaiter(args: string[]): Waiter {
  const state = loadState();
  const proc = spawn(state.cli, args, { cwd, env: cliEnv(state) });
  let stdout = "";
  let stderr = "";
  proc.stdout!.on("data", (d) => (stdout += d));
  // `submit` announces the review on stderr; `wait` already knows it
  const reviewId = new Promise<string>((resolve) => {
    if (args[0] !== "submit") return resolve(args[1]);
    proc.stderr!.on("data", (d) => {
      stderr += d;
      const m = stderr.match(/review (r_[0-9A-Z]+):/);
      if (m) resolve(m[1]);
    });
  });
  if (args[0] !== "submit") proc.stderr!.on("data", (d) => (stderr += d));
  // "close", not "exit": a child's output can still be arriving when it exits,
  // and a waiter's whole answer is on stdout
  const done = new Promise<Run>((resolve) => proc.on("close", (code) => resolve({ code, stdout, stderr })));
  return { proc, reviewId, done };
}

export function tmpFile(name: string, content: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pinrail-e2e-"));
  const file = path.join(dir, name);
  fs.writeFileSync(file, content);
  return file;
}

export const listPayload = {
  intro: "Two proposals from **round 1**.",
  groups: [
    {
      title: "lib/acme/tickets.ex",
      items: [
        { id: 1, severity: "major", title: "do_save dedups without reversing", body: "Reverse after dedup.", meta: { line: 149 } },
        { id: 2, severity: "minor", title: "moduledoc typo" },
      ],
    },
  ],
};

export function submitListReview(title: string, extra: string[] = []): Waiter {
  const payload = tmpFile("payload.json", JSON.stringify(listPayload));
  return startWaiter(["submit", "list", "--title", title, "--origin", "repo=acme,workflow=review,ref=42", "--data", payload, "--wait", ...extra]);
}

export function reviewUrl(id: string): string {
  return `${loadState().url}/reviews/${id}`;
}
