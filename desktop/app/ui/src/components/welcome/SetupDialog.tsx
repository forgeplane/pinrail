// Setting Pinrail up. The welcome screen shows the agents found on this
// computer and one button that sets everything up; the person can leave an
// agent out and add more plugins first. Pinrail opens the setup until it is
// finished or skipped, and the command palette brings it back.

import { useEffect, useRef, useState } from "react";
import { api, inTauri } from "../../api/client";
import type { CatalogEntry } from "../../api/types";
import { useWindowKey } from "../../lib/useWindowKey";
import { useLive } from "../../state/live";
import { useSettings } from "../../state/settings";
import { Dialog } from "../Dialog";
import { byName, useAgents } from "../settings/AgentsSection";
import { SetupPlugins, included } from "./SetupPlugins";
import { SetupRun } from "./SetupRun";
import { SetupWelcome, connectable } from "./SetupWelcome";
import { StepDots } from "./StepDots";
import { TipBar } from "./TipBar";
import { useSetupRun, type SetupPlan } from "./useSetupRun";

/** The setup's steps: the welcome, the setup itself, and a first session. */
const STEPS = 3;

/** The agents the welcome always shows as tiles, in this order. */
const FIRST = ["claude-code", "codex", "opencode", "grok", "pi"];
const rank = (id: string) => (FIRST.includes(id) ? FIRST.indexOf(id) : FIRST.length);

/** The recommended plugins first, and each group in the order of its titles. */
const recommendedFirst = (a: CatalogEntry, b: CatalogEntry) =>
  Number(b.recommended) - Number(a.recommended) || a.title.localeCompare(b.title);

type Screen = "welcome" | "plugins" | "run" | "last";

export function SetupDialog({ onClose }: { onClose: () => void }) {
  const live = useLive();
  const { settings, update } = useSettings();
  const [screen, setScreen] = useState<Screen>("welcome");
  // the agents are looked at again when the window comes back, but not
  // while the setup runs and changes them itself
  const { agents, error } = useAgents(screen !== "run");
  // what the setup runs, once Set up Pinrail is pressed
  const [plan, setPlan] = useState<SetupPlan | null>(null);
  const { parts, running, finished, failed, retry, recheck, addToPath } = useSetupRun(plan);
  const primary = useRef<HTMLButtonElement>(null);
  // what the person chose: the agents left out, and the plugins added
  const [leftOut, setLeftOut] = useState<Set<string>>(new Set());
  const [added, setAdded] = useState<Set<string>>(new Set());
  // the button the list of every agent is open under
  const [agentMenu, setAgentMenu] = useState<HTMLElement | null>(null);

  // the official plugins the app carries, with what is installed
  const [catalog, setCatalog] = useState<CatalogEntry[] | null>(null);
  useEffect(() => {
    api
      .catalog()
      .then(({ plugins }) => setCatalog(plugins.sort(recommendedFirst)))
      .catch(() => {});
  }, [live.plugins]);

  // the dialog holds the keyboard: no key reaches the screen behind it, and
  // Esc leaves the plugins view, or else the setup; buttons, Tab and
  // copying still work, being the browser's own
  useWindowKey(
    (e) => {
      e.stopPropagation();
      // the list of agents takes its own keys, Esc among them
      if (e.key !== "Escape" || agentMenu) return;
      if (screen === "plugins") setScreen("welcome");
      // the setup cannot be left halfway through its run
      else if (!running) onClose();
    },
    { capture: true },
  );

  // the agents found: the most used ones first, then the others by name, with
  // those the setup leaves alone last
  const found = agents
    ? byName(agents.filter((a) => a.found)).sort(
        (a, b) => rank(a.id) - rank(b.id) || Number(a.state === "theirs") - Number(b.state === "theirs"),
      )
    : null;
  const toggleAgent = (id: string) =>
    setLeftOut((out) => {
      const next = new Set(out);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  // what the setup does: the agents it connects and the plugins it installs
  const toConnect = (found ?? []).filter((a) => connectable(a) && !leftOut.has(a.id));
  const toInstall = (catalog ?? []).filter((e) => included(e) || (added.has(e.id) && !e.installed && !e.needs));

  const setUp = () => {
    // the welcome told the person about usage data, so the inbox need not
    if (!settings.telemetry.notice_shown)
      void update({ telemetry: { notice_shown: new Date().toISOString().replace(/\.\d+Z$/, "Z") } });
    setPlan({
      agents: toConnect,
      noAgents: inTauri() && found?.length === 0,
      plugins: toInstall,
      notifications: settings.notifications.enabled,
    });
    setScreen("run");
  };

  // a run that ends with every part done goes on by itself, after a moment
  // to see the last pin fill; a part that failed keeps the screen
  // one with something left for the person to read keeps it too
  const complete = screen === "run" && finished && !running && failed.length === 0 && !parts.some((p) => p.newTerminal);
  useEffect(() => {
    if (!complete) return;
    const timer = window.setTimeout(() => setScreen("last"), 700);
    return () => window.clearTimeout(timer);
  }, [complete]);

  // back from the terminal with the PATH changed: the command is looked at again
  const waitsOnPath = screen === "run" && !running && failed.some((p) => p.fix);
  useEffect(() => {
    if (!waitsOnPath) return;
    window.addEventListener("focus", recheck);
    return () => window.removeEventListener("focus", recheck);
  });

  const agentsNote = !inTauri()
    ? "The desktop app finds your agents and connects them."
    : found
      ? null
      : (error?.message ?? "Looking for agents…");

  return (
    // the setup holds every key itself, Esc included, and stays open on a
    // click outside
    <Dialog
      label="Set up Pinrail"
      plain
      className="setup-dialog"
      initialFocus={primary}
      onClose={onClose}
      closeOnEscape={false}
      closeOnBackdrop={false}
    >
      <StepDots count={STEPS} current={screen === "run" ? 1 : screen === "last" ? 2 : 0} />
      {screen !== "welcome" ? null : (
        <button type="button" className="setup-corner is-skip" onClick={onClose}>
          Skip setup
        </button>
      )}

      {screen === "welcome" ? (
        <>
          <SetupWelcome
            agents={found}
            agentsNote={agentsNote}
            leftOut={leftOut}
            onToggleAgent={toggleAgent}
            menu={agentMenu}
            onMenu={setAgentMenu}
            plugins={toInstall.map((e) => e.title)}
            telemetry={settings.telemetry.usage || settings.telemetry.crashes}
            primary={primary}
            onSetUp={setUp}
            onPlugins={() => setScreen("plugins")}
          />
          <div className="setup-tip">
            <TipBar>
              You can connect an agent later in <b>Settings › Agents</b>.
            </TipBar>
          </div>
        </>
      ) : null}

      {screen === "plugins" ? (
        <>
          <SetupPlugins
            catalog={catalog}
            picked={added}
            onDone={(picked) => {
              setAdded(picked);
              setScreen("welcome");
            }}
            onBack={() => setScreen("welcome")}
          />
          <div className="setup-tip">
            <TipBar>
              You can add more plugins at any time in <b>Settings › Plugins</b>.
            </TipBar>
          </div>
        </>
      ) : null}

      {screen === "run" ? (
        <>
          <SetupRun
            parts={parts}
            running={running}
            failed={failed}
            onRetry={retry}
            onRecheck={recheck}
            onAddToPath={addToPath}
            onContinue={() => setScreen("last")}
          />
          {(failed.length || parts.some((p) => p.newTerminal)) && !running ? null : (
            <div className="setup-tip">
              <TipBar>
                <b>Report status</b> lets Pinrail show when an agent works, waits for you or finishes.
              </TipBar>
            </div>
          )}
        </>
      ) : null}

      {screen === "last" ? (
        // the first session is not built yet: this ends the setup
        <>
          <div className="setup-body">
            <h1 className="setup-title is-view">Pinrail is ready</h1>
            <p className="setup-sub">Your agents can now ask you. What they ask appears in your inbox.</p>
          </div>
          <div className="setup-foot">
            <button type="button" className="chrome-button button-primary setup-button" onClick={onClose} autoFocus>
              Done
            </button>
          </div>
        </>
      ) : null}
    </Dialog>
  );
}
