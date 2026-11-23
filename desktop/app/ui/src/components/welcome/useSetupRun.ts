// The setup itself: installing the pinrail command, connecting the chosen
// agents with Report status on, installing the chosen plugins and asking for
// notifications, one part after another. Each part has its own state, and a
// part that fails does not stop the others.

import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, api, inTauri } from "../../api/client";
import type { CatalogEntry } from "../../api/types";
import type { NotificationStatus } from "../../state/notifications";
import type { AgentStatus } from "../settings/AgentsSection";
import { tilde } from "../../lib/cliPaths";
import type { CliStatus } from "../settings/CliRow";

/** What the person chose on the welcome, as the run takes it. */
export type SetupPlan = {
  /** the agents to connect */
  agents: AgentStatus[];
  /** no agent was found on this computer */
  noAgents: boolean;
  /** the plugins to install */
  plugins: CatalogEntry[];
  /** notifications are on in the settings */
  notifications: boolean;
};

export type PartState = "waiting" | "working" | "done" | "failed" | "skipped";

/** What the person can do about a part that failed, besides trying again. */
export type Fix = "path" | "check";

export type Part = {
  key: string;
  kind: "command" | "agent" | "agents" | "plugins" | "notifications";
  /** the agents of an agent part */
  agents?: AgentStatus[];
  /** the plugins of the plugins part */
  plugins?: CatalogEntry[];
  state: PartState;
  /** a word or two on the part's line, such as Installed */
  text?: string;
  /** why it failed */
  reason?: string;
  fix?: Fix;
  /** for the command off the PATH: the file Pinrail can add the PATH line
   *  to, as the person reads it, when it knows the shell */
  profile?: string;
  /** the PATH line in the shell's syntax */
  line?: string;
  /** done, with something left for the person: to open a new terminal */
  newTerminal?: boolean;
};

type Outcome = Pick<Part, "state" | "text" | "reason" | "fix" | "profile" | "line" | "newTerminal">;

/** Up to this many agents have a line each; more share one. */
const AGENT_LINES = 4;

const invoke = <T>(command: string, args?: Record<string, unknown>) =>
  import("@tauri-apps/api/core").then(({ invoke }) => invoke<T>(command, args));

const message = (e: unknown) =>
  e instanceof ApiError
    ? (e.violations[0]?.message ?? e.message)
    : e instanceof Error
      ? e.message
      : typeof e === "string"
        ? e
        : "It did not work";

/** The parts of a plan, each waiting. */
function partsOf(plan: SetupPlan): Part[] {
  const parts: Part[] = [{ key: "command", kind: "command", state: "waiting" }];
  if (plan.noAgents) parts.push({ key: "agents", kind: "agents", agents: [], state: "skipped", text: "None found" });
  else if (plan.agents.length > AGENT_LINES)
    parts.push({ key: "agents", kind: "agents", agents: plan.agents, state: "waiting" });
  else for (const a of plan.agents) parts.push({ key: `agent-${a.id}`, kind: "agent", agents: [a], state: "waiting" });
  if (plan.plugins.length) parts.push({ key: "plugins", kind: "plugins", plugins: plan.plugins, state: "waiting" });
  if (plan.notifications && inTauri()) parts.push({ key: "notifications", kind: "notifications", state: "waiting" });
  return parts;
}

/** Where the command stands, from what a new terminal finds for `pinrail`. */
function commandOutcome(status: CliStatus): Outcome {
  const done: Outcome = { state: "done", text: "Installed" };
  if (status.runs) {
    if (status.installed && status.runs === status.link) return done;
    // a development build carries none, so any pinrail on the PATH is the one
    if (!status.bundled) return done;
    return {
      state: "failed",
      text: "Another one runs first",
      reason: `A new terminal runs ${status.runs} first. Put the folder of ${status.link} earlier on your PATH.`,
      fix: "check",
    };
  }
  if (status.installed) {
    if (status.dir_on_path === false)
      return {
        state: "failed",
        text: "Not on your PATH",
        fix: "path",
        profile: status.profile ? tilde(status.link)(status.profile) : undefined,
        line: status.profile_line ?? undefined,
      };
    // the shell could not be asked, or it answered nothing: the command is in place
    return done;
  }
  return {
    state: "failed",
    text: "Not installed",
    reason: status.bundled
      ? "Pinrail could not install the command."
      : "This build of Pinrail does not carry the command. Build it from a checkout with cargo install --path cli.",
    fix: "check",
  };
}

async function runCommand(): Promise<Outcome> {
  if (!inTauri()) return { state: "skipped", text: "The desktop app installs it" };
  let status = await invoke<CliStatus>("cli_status");
  const missing = commandOutcome(status).state !== "done";
  if (missing && status.bundled && status.mode !== "package" && (!status.installed || status.outdated))
    status = await invoke<CliStatus>("install_cli");
  return commandOutcome(status);
}

/** Connects each agent and turns its Report status on, and says which failed. */
async function runAgents(agents: AgentStatus[], say: (text: string) => void): Promise<Outcome> {
  const several = agents.length > 1;
  const failures: string[] = [];
  let all = await invoke<AgentStatus[]>("agents_status");
  for (const agent of agents) {
    say(several ? `Connecting ${agent.name}…` : "Connecting…");
    try {
      let now = all.find((a) => a.id === agent.id);
      if (now && now.found && (now.state === "absent" || now.state === "outdated")) {
        all = await invoke<AgentStatus[]>("connect_agent", { id: agent.id });
        now = all.find((a) => a.id === agent.id);
      }
      // Report status, where Pinrail installs it and nothing of the person's is in its place
      const hooks = now?.integration?.state;
      if (hooks === "absent" || hooks === "outdated")
        all = await invoke<AgentStatus[]>("set_agent_hooks", { id: agent.id, on: true });
    } catch (e) {
      failures.push(several ? `${agent.name}: ${message(e)}` : message(e));
    }
  }
  if (!failures.length) return { state: "done", text: "Connected" };
  return {
    state: "failed",
    text: several ? `${failures.length} not connected` : "Not connected",
    reason: failures.join("\n"),
  };
}

/** Installs the plugins that are not installed yet, and says which failed. */
async function runPlugins(plugins: CatalogEntry[], say: (text: string) => void): Promise<Outcome> {
  const failures: string[] = [];
  const installed = new Set((await api.catalog()).plugins.filter((e) => e.installed).map((e) => e.id));
  for (const plugin of plugins) {
    if (installed.has(plugin.id)) continue;
    say(plugins.length > 1 ? `Installing ${plugin.title}…` : "Installing…");
    try {
      await api.installPlugin({ id: plugin.id });
    } catch (e) {
      failures.push(`${plugin.title}: ${message(e)}`);
    }
  }
  if (!failures.length) return { state: "done", text: "Installed" };
  return { state: "failed", text: "Not installed", reason: failures.join("\n") };
}

/** Asks the system for notifications when it has not been asked. */
async function runNotifications(): Promise<Outcome> {
  let status = await invoke<NotificationStatus | null>("notification_status");
  if (status?.authorization === "not_determined")
    status = await invoke<NotificationStatus | null>("request_notifications");
  // the person's own answer is not a failure
  if (status?.authorization === "denied") return { state: "skipped", text: "Not allowed in System Settings" };
  return { state: "done", text: "On" };
}

/** Runs `plan` once it is given, and runs the failed parts again on request. */
export function useSetupRun(plan: SetupPlan | null) {
  const [parts, setParts] = useState<Part[]>([]);
  const [running, setRunning] = useState(false);
  const [finished, setFinished] = useState(false);
  const started = useRef(false);

  const run = useCallback(async (list: Part[]) => {
    setRunning(true);
    const set = (key: string, change: Partial<Part>) =>
      setParts((all) => all.map((p) => (p.key === key ? { ...p, ...change } : p)));
    for (const part of list) {
      const say = (text: string) => set(part.key, { text });
      set(part.key, { state: "working", text: undefined, reason: undefined, fix: undefined, newTerminal: undefined });
      let outcome: Outcome;
      try {
        if (part.kind === "command") outcome = await runCommand();
        else if (part.kind === "plugins") outcome = await runPlugins(part.plugins ?? [], say);
        else if (part.kind === "notifications") outcome = await runNotifications();
        else outcome = await runAgents(part.agents ?? [], say);
      } catch (e) {
        outcome = {
          state: "failed",
          text: "Failed",
          reason: message(e),
          fix: part.kind === "command" ? "check" : undefined,
        };
      }
      set(part.key, outcome);
    }
    setRunning(false);
    setFinished(true);
  }, []);

  useEffect(() => {
    if (!plan || started.current) return;
    started.current = true;
    const all = partsOf(plan);
    setParts(all);
    void run(all.filter((p) => p.state === "waiting"));
  }, [plan, run]);

  const failed = parts.filter((p) => p.state === "failed");
  /** Runs the failed parts again. */
  const retry = () => void run(failed);
  /** Looks again at whether a terminal finds the command. */
  const recheck = () => void run(parts.filter((p) => p.key === "command"));

  /** Adds the PATH line to the shell's profile file. A new terminal then
   *  finds the command, so the part is done; the open ones do not yet. */
  const addToPath = async () => {
    const command = parts.find((p) => p.key === "command");
    if (!command?.profile) return;
    const set = (change: Partial<Part>) =>
      setParts((all) => all.map((p) => (p.key === "command" ? { ...p, ...change } : p)));
    setRunning(true);
    try {
      await invoke<CliStatus>("add_cli_to_path");
      set({ state: "done", text: `Added to ${command.profile}`, reason: undefined, fix: undefined, newTerminal: true });
    } catch (e) {
      set({ reason: message(e) });
    }
    setRunning(false);
  };

  return { parts, running, finished, failed, retry, recheck, addToPath };
}
