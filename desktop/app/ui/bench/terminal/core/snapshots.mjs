// The snapshots of the attach and memory cases: a terminal of 120 columns
// and 40 rows holding 5,000 lines of history, made by the host's own
// terminal (desktop/screen/examples/bench_snapshot.rs), in both the formats
// the host sends: its `vt` first paint and libghostty-vt's binary snapshot.
// They are cached until a source changes.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

/** The corpus items each snapshot is made from. */
export const SOURCES = {
  // agent output: the tui workload, then the diff workload
  "history-5000": ["synthetic/tui.bin", "synthetic/diff.bin"],
  // dense colour: truecolor on every word, with wide characters
  "history-5000-styled": ["synthetic/styled.bin"],
};

/** A snapshot's bytes by format, made by the host if not cached. */
export function snapshot(name, { corpus, cache, workspace }) {
  const sources = SOURCES[name].map((item) => join(corpus, item));
  const vt = join(cache, `${name}.vt`);
  const bin = join(cache, `${name}.bin`);
  const formatFile = join(cache, `${name}.format`);
  const fresh = (file) => existsSync(file) && sources.every((s) => statSync(s).mtimeMs < statSync(file).mtimeMs);
  if (!fresh(vt) || !fresh(bin) || !fresh(formatFile)) {
    console.error(`making snapshot ${name} with the host's terminal…`);
    const format = execFileSync(
      "cargo",
      ["run", "-q", "--release", "-p", "pinrail-screen", "--example", "bench_snapshot", "--", cache, name, ...sources],
      { cwd: workspace, stdio: ["ignore", "pipe", "inherit"] },
    )
      .toString()
      .trim();
    execFileSync("sh", ["-c", 'printf %s "$1" > "$2"', "sh", format, formatFile]);
  }
  const format = readFileSync(formatFile, "utf8");
  return { vt: new Uint8Array(readFileSync(vt)), [format]: new Uint8Array(readFileSync(bin)) };
}
