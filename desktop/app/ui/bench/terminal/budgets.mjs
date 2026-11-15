// The window budgets of runtime-spec 8.6, checked against one adapter's
// results in one runner. Each check names the case that measures it.

const fmt = (x, d = 1) => (x === null || x === undefined || Number.isNaN(x) ? "–" : Number(x).toFixed(d));

/** Finds the result of a case (and item) in a list of results. */
export function find(results, kase, item, extra = {}) {
  return results.find(
    (r) => r.case === kase && (!item || r.item === item) && Object.entries(extra).every(([k, v]) => r[k] === v),
  );
}

export function checkBudgets(results) {
  const out = [];
  const add = (name, budget, value, pass, measuredBy) => out.push({ name, budget, value, pass, measuredBy });

  const input = find(results, "input", null, { load: undefined });
  if (input && !input.error) {
    const l = input.latency;
    add(
      "Keystroke to echo drawn, p50 / p95 / p99",
      "≤ 5 / 12 / 25 ms",
      `${fmt(l.p50)} / ${fmt(l.p95)} / ${fmt(l.p99)} ms`,
      l.n > 0 && l.p50 <= 5 && l.p95 <= 12 && l.p99 <= 25,
      "input",
    );
  }
  const inputLoad = find(results, "input", null, { load: "1" });
  if (inputLoad && !inputLoad.error) {
    const l = inputLoad.latency;
    add(
      "The same while another terminal streams 2 MB/s, p95",
      "≤ 25 ms",
      `${fmt(l.p95)} ms`,
      l.n > 0 && l.p95 <= 25,
      "input load=1",
    );
  }
  for (const w of ["tui", "styled"]) {
    const b = find(results, "burst", `synthetic/${w}.bin`);
    if (b && !b.error) {
      const longest = Math.max(b.frames.max, b.frames.longTasks?.max ?? 0);
      add(
        `Longest main-thread task or frame, 8 MB ${w} burst`,
        "≤ 50 ms",
        `${fmt(longest)} ms`,
        longest <= 50,
        `burst ${w}`,
      );
    }
  }
  const stream = find(results, "stream", "synthetic/tui.bin");
  if (stream && !stream.error) {
    const f = stream.frames;
    add(
      "Frame time streaming 2 MB/s, p95 / max",
      "≤ 20 / 50 ms",
      `${fmt(f.p95)} / ${fmt(f.max)} ms`,
      f.p95 <= 20 && f.max <= 50,
      "stream tui",
    );
  }
  const tui = find(results, "burst", "synthetic/tui.bin");
  if (tui && !tui.error)
    add("8 MB tui burst until drawn", "≤ 500 ms", `${fmt(tui.totalMs, 0)} ms`, tui.totalMs <= 500, "burst tui");
  const attach = find(results, "attach", null, { snapshot: undefined });
  if (attach && !attach.error)
    add(
      "Attach with 5,000 lines to first frame",
      "≤ 150 ms",
      `${fmt(attach.attachMs)} ms`,
      attach.attachMs <= 150,
      "attach",
    );
  const sw = find(results, "switch");
  if (sw && !sw.error)
    add(
      "Switch to a pooled terminal, max of 5",
      "≤ 50 ms",
      `${fmt(sw.switchMs.max)} ms`,
      sw.switchMs.max <= 50,
      "switch",
    );
  const mem = find(results, "memory");
  if (mem && !mem.error) {
    // The larger of the process and JS heap growth, which overlap.
    const v = Math.max(mem.perTerminalRssMiB ?? -Infinity, mem.perTerminalHeapMiB ?? -Infinity);
    add(
      "Memory per attached terminal at 5,000 lines",
      "≤ 30 MiB",
      `${fmt(v)} MiB`,
      // not measured where the runner cannot sample the page's process
      Number.isFinite(v) ? v <= 30 : null,
      "memory",
    );
  }
  const idle = find(results, "idle");
  if (idle && !idle.error) {
    const v = idle.drawnPerSecond ?? idle.animationFrameRequestsPerSecond;
    add("Idle terminal, frames drawn per second", "0", fmt(v), v === 0, "idle");
  }
  return out;
}

/** Other measurements worth a row in the table, without a budget. */
export function otherRows(results) {
  const rows = [];
  const load = find(results, "load");
  if (load && !load.error) {
    rows.push(["Load: code and wasm ready", `${fmt(load.loadMs)} ms`]);
    rows.push(["Load: first frame since navigation", `${fmt(load.firstFrameSinceNavigationMs)} ms`]);
    rows.push(["Load: page, code and wasm size", `${fmt(load.codeBytes / 1024, 0)} KiB`]);
  }
  for (const w of ["tui", "styled", "plain", "alt"]) {
    const b = find(results, "burst", `synthetic/${w}.bin`);
    if (b && !b.error)
      rows.push([
        `Burst ${w}: 8 MB until drawn (throughput, frames over 50 ms)`,
        `${fmt(b.totalMs, 0)} ms (${fmt(b.mbPerS)} MB/s, ${b.frames.over50})`,
      ]);
  }
  const stream = find(results, "stream", "synthetic/tui.bin");
  if (stream && !stream.error)
    rows.push([
      "Stream 2 MB/s: frames over 16 / 33 / 50 ms",
      `${stream.frames.over16} / ${stream.frames.over33} / ${stream.frames.over50} of ${stream.frames.frames}`,
    ]);
  const input = find(results, "input", null, { load: undefined });
  if (input && !input.error) {
    rows.push(["Input: key to encoded input, p50", `${fmt(input.keyToInput.p50, 2)} ms`]);
    rows.push(["Input: WebSocket and PTY round trip, p50", `${fmt(input.wsAndPtyRoundTrip.p50, 2)} ms`]);
    rows.push(["Input: echo received to drawn, p50", `${fmt(input.echoToDrawn.p50, 2)} ms`]);
  }
  const sw = find(results, "switch");
  if (sw && !sw.error)
    rows.push([
      "Switch: drew while hidden / stale 1 s after shown, of 5",
      `${sw.drewWhileHidden} / ${sw.staleAfterShow}`,
    ]);
  const styled = find(results, "attach", null, { snapshot: "history-5000-styled" });
  if (styled && !styled.error)
    rows.push([
      "Attach, 5,000 styled lines: size, to first frame",
      `${fmt(styled.bytes / 1048576, 2)} MiB, ${fmt(styled.attachMs)} ms`,
    ]);
  const attach = find(results, "attach", null, { snapshot: undefined });
  if (attach && !attach.error)
    rows.push([
      "Attach: snapshot size, transfer, restore to drawn",
      `${fmt(attach.bytes / 1048576, 2)} MiB, ${fmt(attach.transferMs)} ms, ${fmt(attach.restoreToDrawnMs)} ms`,
    ]);
  const mem = find(results, "memory");
  if (mem && !mem.error)
    rows.push([
      "Memory per terminal: process / JS heap / wasm total",
      `${fmt(mem.perTerminalRssMiB)} / ${fmt(mem.perTerminalHeapMiB)} / ${fmt(mem.wasmMemoryMiB)} MiB`,
    ]);
  const agents = find(results, "agents", null, { titles: undefined });
  if (agents && !agents.error)
    rows.push([
      `Agents: ${agents.agents} streaming, one shown: draws/s shown / hidden, drawing ms/s, CPU`,
      `${fmt(agents.shownDrawsPerSecond)} / ${fmt(agents.hiddenDrawsPerSecond)}, ${fmt(agents.drawMsPerSecond)}, ${fmt(agents.cpuPercent, 0)} %`,
    ]);
  const spinning = find(results, "agents", null, { titles: "1" });
  if (spinning && !spinning.error)
    rows.push([
      `Agents: ${spinning.agents} spinning their titles, one shown: draws/s shown, drawing ms/s`,
      `${fmt(spinning.shownDrawsPerSecond)}, ${fmt(spinning.drawMsPerSecond)}`,
    ]);
  for (const r of results.filter((r) => r.case === "replay" && !r.error)) {
    const name = r.item.split("/").pop().replace(".cast", "");
    rows.push([
      `Replay ${name}: p95 / max frame, max speed`,
      `${fmt(r.realtime.frames.p95)} / ${fmt(r.realtime.frames.max)} ms, ${fmt(r.maxSpeedMs, 0)} ms`,
    ]);
  }
  const csp = [...new Set(results.flatMap((r) => r.cspViolations ?? []))];
  rows.push(["CSP violations under the app's policy", csp.length ? csp.join("; ").slice(0, 160) : "none"]);
  for (const r of results.filter((r) => r.error))
    rows.push([`Error in ${r.case} ${r.item ?? ""}`, r.error.split("\n")[0].slice(0, 80)]);
  return rows;
}
