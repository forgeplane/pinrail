// The terminal benchmark against a baseline: the results of an earlier run
// on the same kind of machine, such as main's last run in CI. CI's machines
// are not the reference machines, so it checks for regressions rather than
// the budgets. A result fails when it is more than 15 % worse than the
// baseline's and also worse by more than its margin: WebKit's timer has a
// resolution of 1 ms and a frame lasts about 16 ms, so a ratio of small
// values means little.
import { find } from "./budgets.mjs";

/** How much worse than the baseline a result may be. */
export const TOLERANCE = 0.15;

/** The measures compared, all of them better when lower: a name, how to
 *  read it from one renderer's results, its unit, and its margin. */
export const MEASURES = [
  ["load_ms", (r) => find(r, "load")?.loadMs, "ms", 20],
  ...["tui", "styled", "plain", "alt"].flatMap((w) => [
    [`burst_${w}_ms`, (r) => find(r, "burst", `synthetic/${w}.bin`)?.totalMs, "ms", 50],
    [`burst_${w}_longest_frame_ms`, (r) => find(r, "burst", `synthetic/${w}.bin`)?.frames?.max, "ms", 17],
  ]),
  ["stream_p95_ms", (r) => find(r, "stream", "synthetic/tui.bin")?.frames?.p95, "ms", 5],
  ["stream_longest_frame_ms", (r) => find(r, "stream", "synthetic/tui.bin")?.frames?.max, "ms", 17],
  ["idle_frames_per_s", (r) => drawnPerSecond(find(r, "idle")), "frames/s", 0],
  ["input_p95_ms", (r) => find(r, "input", null, { load: undefined })?.latency?.p95, "ms", 5],
  ["input_loaded_p95_ms", (r) => find(r, "input", null, { load: "1" })?.latency?.p95, "ms", 8],
  ["attach_ms", (r) => find(r, "attach", null, { snapshot: undefined })?.attachMs, "ms", 20],
  ["attach_styled_ms", (r) => find(r, "attach", null, { snapshot: "history-5000-styled" })?.attachMs, "ms", 30],
  ["switch_max_ms", (r) => find(r, "switch")?.switchMs?.max, "ms", 17],
  ["memory_mib", (r) => find(r, "memory")?.perTerminalRssMiB, "MiB", 3],
  ["agents_draws_per_s", (r) => agentDraws(find(r, "agents")), "draws/s", 5],
  ["agents_draw_ms_per_s", (r) => find(r, "agents")?.drawMsPerSecond, "ms/s", 5],
  ["agents_cpu_percent", (r) => find(r, "agents")?.cpuPercent, "%", 10],
];

function agentDraws(agents) {
  return agents ? agents.shownDrawsPerSecond + agents.hiddenDrawsPerSecond : undefined;
}

function drawnPerSecond(idle) {
  return idle ? (idle.drawnPerSecond ?? idle.animationFrameRequestsPerSecond) : undefined;
}

/** A run's measures, as `<runner>/<renderer>` → name → value: the shape of
 *  a baseline file. */
export function summarise(results) {
  const out = {};
  for (const r of results) {
    const key = `${r.runner}/${r.adapter}`;
    if (out[key]) continue;
    const mine = results.filter((x) => x.runner === r.runner && x.adapter === r.adapter && !x.error);
    out[key] = {};
    for (const [name, read] of MEASURES) {
      const value = read(mine);
      if (Number.isFinite(value)) out[key][name] = value;
    }
  }
  return out;
}

/** The regressions of `current` against `baseline`, both from summarise(),
 *  as sentences; none when every measure is within its limit. A measure
 *  missing from either side is not compared. */
export function regressions(current, baseline) {
  const out = [];
  for (const [key, measures] of Object.entries(current)) {
    for (const [name, , unit, margin] of MEASURES) {
      const value = measures[name];
      const base = baseline[key]?.[name];
      if (!Number.isFinite(value) || !Number.isFinite(base)) continue;
      const limit = base * (1 + TOLERANCE) + margin;
      if (value > limit) {
        out.push(
          `${key} ${name}: ${round(value)} ${unit} is worse than the baseline's ${round(base)} ${unit} by more than 15 % and ${margin} ${unit}`,
        );
      }
    }
  }
  return out;
}

const round = (x) => Math.round(x * 10) / 10;
