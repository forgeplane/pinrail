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
import { Mark } from "./Mark";
import { listed } from "./names";
import { SetupPlugins, included } from "./SetupPlugins";
import { SetupWelcome, connectable } from "./SetupWelcome";
import { StepDots } from "./StepDots";
import { TipBar } from "./TipBar";

/** The setup's steps: the welcome, the setup itself, and a first session. */
const STEPS = 3;

/** The recommended plugins first, and each group in the order of its titles. */
/** The agents the welcome always shows as tiles, in this order. */
const FIRST = ["claude-code", "codex", "opencode", "grok", "pi"];
const rank = (id: string) => (FIRST.includes(id) ? FIRST.indexOf(id) : FIRST.length);

const recommendedFirst = (a: CatalogEntry, b: CatalogEntry) =>
  Number(b.recommended) - Number(a.recommended) || a.title.localeCompare(b.title);

type Screen = "welcome" | "plugins" | "run";

export function SetupDialog({ onClose }: { onClose: () => void }) {
  const live = useLive();
  const { settings } = useSettings();
  const { agents, error } = useAgents(true);
  const [screen, setScreen] = useState<Screen>("welcome");
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
      else onClose();
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
      <StepDots count={STEPS} current={screen === "run" ? 1 : 0} />
      {screen === "plugins" ? null : (
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
            onSetUp={() => setScreen("run")}
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
        // the setup itself is not built yet: this says what it will do
        <>
          <div className="setup-body">
            <span className="setup-run-mark">
              <Mark size={44} />
            </span>
            <h1 className="setup-title is-view">Setting up Pinrail</h1>
            <p className="setup-sub" data-testid="setup-plan-agents">
              {toConnect.length ? (
                <>Agents to connect: {listed(toConnect.map((a) => a.name))}.</>
              ) : (
                "No agent to connect."
              )}
            </p>
            <p className="setup-sub" data-testid="setup-plan-plugins">
              {toInstall.length ? <>Plugins to add: {listed(toInstall.map((e) => e.title))}.</> : "No plugin to add."}
            </p>
          </div>
          <div className="setup-foot">
            <button type="button" className="chrome-button setup-button" onClick={() => setScreen("welcome")}>
              Back
            </button>
          </div>
        </>
      ) : null}
    </Dialog>
  );
}
