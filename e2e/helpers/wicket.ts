import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { cliEnv, loadState } from "./state";

export type Run = { code: number | null; stdout: string; stderr: string };

/** Runs the CLI to completion. */
export function wicket(args: string[], opts: { input?: string } = {}): Run {
  const state = loadState();
  const r = spawnSync(state.cli, args, { env: cliEnv(state), encoding: "utf8", input: opts.input });
  return { code: r.status, stdout: r.stdout, stderr: r.stderr };
}

/** Runs the CLI and parses its stdout as JSON, failing loudly otherwise. */
export function wicketJson(args: string[], opts: { input?: string } = {}): any {
  const r = wicket(args, opts);
  if (r.code !== 0) throw new Error(`wicket ${args.join(" ")} exited ${r.code}\n${r.stderr}`);
  return JSON.parse(r.stdout);
}

export type Waiter = {
  proc: ChildProcess;
  /** the gate id, parsed from the "gate g_…: url" line on stderr */
  gateId: Promise<string>;
  done: Promise<Run>;
};

/** Starts `wicket create … --wait` (or `wicket wait`) in the background. */
export function startWaiter(args: string[]): Waiter {
  const state = loadState();
  const proc = spawn(state.cli, args, { env: cliEnv(state) });
  let stdout = "";
  let stderr = "";
  proc.stdout!.on("data", (d) => (stdout += d));
  // `create` announces the gate on stderr; `wait` already knows it
  const gateId = new Promise<string>((resolve) => {
    if (args[0] !== "create") return resolve(args[1]);
    proc.stderr!.on("data", (d) => {
      stderr += d;
      const m = stderr.match(/gate (g_[0-9A-Z]+):/);
      if (m) resolve(m[1]);
    });
  });
  if (args[0] !== "create") proc.stderr!.on("data", (d) => (stderr += d));
  const done = new Promise<Run>((resolve) => proc.on("exit", (code) => resolve({ code, stdout, stderr })));
  return { proc, gateId, done };
}

export function tmpFile(name: string, content: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "wicket-e2e-"));
  const file = path.join(dir, name);
  fs.writeFileSync(file, content);
  return file;
}

export const listPayload = {
  intro: "Two proposals from **round 1**.",
  allow_additions: true,
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

export function createListGate(title: string, extra: string[] = []): Waiter {
  const payload = tmpFile("payload.json", JSON.stringify(listPayload));
  return startWaiter(["create", "list", "--title", title, "--source", "repo=acme,workflow=review,ref=42", "--data", payload, "--wait", ...extra]);
}

export function gateUrl(id: string): string {
  return `${loadState().url}/gates/${id}`;
}
