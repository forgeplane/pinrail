// Setting Pinrail up, as a short wizard in three steps: connecting the
// pinrail command, the agents and notifications; choosing the plugins to
// install; and a first review, from an agent or a plugin's sample. Each step
// ticks itself in the rail when it is done. Try it opens once a plugin is
// installed, since every review needs one. Pinrail opens the setup until it
// is finished or skipped; the command palette brings it back.

import { Bell, Bot, Check, Copy, Lock, SquareTerminal } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { ApiError, api, inTauri } from "../../api/client";
import type { CatalogEntry, Plugin } from "../../api/types";
import { copyText } from "../../lib/clipboard";
import { useLive } from "../../state/live";
import { describeSystem, useNotificationStatus } from "../../state/notifications";
import { useSettings } from "../../state/settings";
import { PluginIcon } from "../PluginIcon";
import { Toggle } from "../settings/controls";
import { Tile, useAgents, where, type AgentStatus } from "../settings/AgentsSection";
import { useCli, type CliStatus } from "../settings/CliRow";
import { WaitMark } from "../WaitMark";

const PATH_LINE = `export PATH="$HOME/.local/bin:$PATH"`;

/** A prompt to give an agent for each official plugin. The skill that
 *  connecting an agent adds tells it how to submit. */
const PROMPTS: Record<string, string> = {
  list: "Ask me through Pinrail which TODOs in this repository to tackle first.",
  feedback: "Ask me through Pinrail what you need to know before the next task.",
  "code-review": "Review my last commit, and send me your comments on my changes through Pinrail.",
  markdown: "Plan the next change, and ask me through Pinrail to review the plan.",
  image: "Make three versions of an icon for this project, and ask me through Pinrail which to keep.",
};

const STEPS = [
  { label: "Connect", detail: "The command and your agents" },
  { label: "Plugins", detail: "The kinds of review" },
  { label: "Try it", detail: "Your first review" },
] as const;

/** The step the setup comes back to once its first review is decided. */
export const FIRST_REVIEW = 2;

const failure = (e: unknown, fallback: string) =>
  e instanceof ApiError ? (e.violations[0]?.message ?? e.message) : e instanceof Error ? e.message : fallback;

/** The app's mark: pin, slash, pin. */
function Mark() {
  return (
    <svg className="welcome-logo" viewBox="0 0 824 824" aria-hidden="true">
      <rect width="824" height="824" rx="185" fill="#221f1c" />
      <rect
        x="6"
        y="6"
        width="812"
        height="812"
        rx="180"
        fill="none"
        stroke="#f1ebdf"
        strokeOpacity="0.12"
        strokeWidth="12"
      />
      <g transform="translate(412 412) scale(24) translate(-12 -12.2)">
        <rect x="1.5" y="13" width="21" height="3.4" rx="1.4" fill="#f1ebdf" />
        <rect x="4.8" y="5" width="2.4" height="16" rx="1.2" fill="#f1ebdf" />
        <rect x="16.8" y="5" width="2.4" height="16" rx="1.2" fill="#f1ebdf" />
        <g transform="rotate(20 12 14.7)">
          <rect x="10.8" y="4.2" width="2.4" height="16.8" rx="1.2" fill="#e5694f" />
        </g>
      </g>
    </svg>
  );
}

function CopyButton({ text, label = "Copy" }: { text: string; label?: string }) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const t = window.setTimeout(() => setCopied(false), 1600);
    return () => window.clearTimeout(t);
  }, [copied]);
  return (
    <button
      type="button"
      className="chrome-button"
      onClick={() =>
        copyText(text)
          .then(() => setCopied(true))
          .catch(() => {})
      }
    >
      {copied ? <Check size={13} /> : <Copy size={13} />} {copied ? "Copied" : label}
    </button>
  );
}

/** One part of a step: an icon, a title and a line, what it needs on the
 *  right, and anything more below. */
function Section({
  icon,
  title,
  detail,
  side,
  children,
  name,
}: {
  icon: ReactNode;
  title: string;
  detail: ReactNode;
  side?: ReactNode;
  children?: ReactNode;
  name: string;
}) {
  return (
    <section className="welcome-section" data-welcome-section={name}>
      <div className="welcome-section-head">
        <span className="welcome-section-icon">{icon}</span>
        <span className="welcome-section-text">
          <b>{title}</b>
          <span>{detail}</span>
        </span>
        {side}
      </div>
      {children}
    </section>
  );
}

/** What a new terminal finds for `pinrail`, and whether it is this app's. */
type Found = "checking" | "ours" | "other" | "missing" | "unknown";
function found(status: CliStatus | null, checking: boolean): Found {
  if (checking || !status) return "checking";
  if (!status.runs) return status.dir_on_path === null ? "unknown" : "missing";
  if (status.installed && status.runs === status.link) return "ours";
  // a development build carries none, so any pinrail on the PATH is the one
  return status.bundled ? "other" : "ours";
}

/** Whether a terminal runs this Pinrail's command, and how to make it so. */
function CommandSection({ cli }: { cli: ReturnType<typeof useCli> }) {
  const icon = <SquareTerminal size={16} />;
  if (!inTauri()) {
    return (
      <Section
        name="command"
        icon={icon}
        title="The command"
        detail="The desktop app installs it and checks that a terminal finds it"
      />
    );
  }

  const status = cli.status;
  const state = found(status, cli.checking || cli.busy);
  const recheck = () => void cli.check();
  const checkAgain = (
    <button type="button" className="chrome-button" onClick={recheck}>
      Check again
    </button>
  );

  let detail: ReactNode = "Looking in a new terminal…";
  let side: ReactNode = null;
  let body: ReactNode = null;
  if (state === "ours") {
    detail = <span className="mono">{status?.runs && where(status.runs)}</span>;
    side = (
      <span className="welcome-ok">
        <Check size={14} strokeWidth={2.5} /> Installed
      </span>
    );
  } else if (state === "missing" && status?.installed && status.dir_on_path === false) {
    detail = (
      <>
        Installed in <span className="mono">~/.local/bin</span>, which is not on your PATH
      </>
    );
    side = checkAgain;
    body = (
      <>
        <p>
          Add this line to your shell profile, such as <code>~/.zshrc</code> for zsh or <code>~/.bashrc</code> for bash,
          then check again:
        </p>
        <div className="welcome-line">
          <code>{PATH_LINE}</code>
          <CopyButton text={PATH_LINE} />
        </div>
      </>
    );
  } else if (state === "missing" || state === "unknown" || (state === "other" && !status?.installed)) {
    if (status?.bundled && status.mode !== "package") {
      detail =
        state === "other" ? (
          <>
            A terminal finds another <span className="mono">pinrail</span> first
          </>
        ) : (
          "Agents ask through it"
        );
      side = (
        <button type="button" className="chrome-button button-primary" onClick={cli.install} disabled={cli.busy}>
          {cli.busy ? "Installing…" : status.outdated ? "Update" : "Install"}
        </button>
      );
      body = (
        <p>
          {status.mode === "copy" ? "Installing copies" : "Installing links"} it into <code>~/.local/bin</code>, where
          your terminal and your agents find it.
          {state === "other" ? " Put that folder before the other one on your PATH." : null}
        </p>
      );
    } else {
      detail = "This build of Pinrail does not carry it";
      side = checkAgain;
      body = (
        <p>
          Build it from a checkout with <code>cargo install --path cli</code>, then check again.
        </p>
      );
    }
  } else if (state === "other") {
    detail = (
      <>
        A terminal finds <span className="mono">{status?.runs}</span> first
      </>
    );
    side = checkAgain;
    body = (
      <p>
        This Pinrail's command is installed at <code>{status?.link}</code>. Put <code>~/.local/bin</code> earlier on
        your PATH, then check again.
      </p>
    );
  }

  return (
    <Section name="command" icon={icon} title="The command" detail={detail} side={side}>
      {body || cli.error ? (
        <div className="welcome-section-body">
          {body}
          {cli.error ? <p className="welcome-error">{cli.error}</p> : null}
        </div>
      ) : null}
    </Section>
  );
}

/** An agent that can be chosen: found, and without Pinrail's current skill. */
const connectable = (a: AgentStatus) => a.found && (a.state === "absent" || a.state === "outdated");

/** The agents found on this computer, each to tick and connect. */
function AgentsSection() {
  const { agents, busy, error, act } = useAgents(true);
  const [chosen, setChosen] = useState<Set<string> | null>(null);
  const [connecting, setConnecting] = useState(false);
  const icon = <Bot size={16} />;

  // every agent that can be connected, chosen once the agents are known
  useEffect(() => {
    if (agents && !chosen) setChosen(new Set(agents.filter(connectable).map((a) => a.id)));
  }, [agents, chosen]);

  if (!inTauri()) {
    return (
      <Section
        name="agents"
        icon={icon}
        title="Your agents"
        detail="The desktop app finds your agents and connects them"
      />
    );
  }
  if (!agents) {
    return <Section name="agents" icon={icon} title="Your agents" detail={error?.message ?? "Looking for agents…"} />;
  }

  const shown = agents.filter((a) => a.found);
  const toConnect = shown.filter((a) => connectable(a) && chosen?.has(a.id));
  const connect = async () => {
    setConnecting(true);
    for (const a of toConnect) await act("connect_agent", a.id);
    setConnecting(false);
  };
  const toggle = (id: string) =>
    setChosen((c) => {
      const next = new Set(c);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  if (shown.length === 0) {
    return (
      <Section
        name="agents"
        icon={icon}
        title="Your agents"
        detail="Pinrail found no agents on this computer. Settings › Agents has the skill to give an agent yourself."
      />
    );
  }

  return (
    <Section
      name="agents"
      icon={icon}
      title="Your agents"
      detail="Choose the agents to connect. These are the ones Pinrail found on this computer."
      side={
        toConnect.length > 0 ? (
          <button
            type="button"
            className="chrome-button button-primary"
            onClick={connect}
            disabled={connecting}
            data-welcome-connect
          >
            {connecting
              ? "Connecting…"
              : toConnect.length === 1
                ? "Connect 1 agent"
                : `Connect ${toConnect.length} agents`}
          </button>
        ) : null
      }
    >
      <div className="welcome-agents">
        {shown.map((a) => {
          const done = a.state === "connected" || a.state === "covered";
          const pill =
            a.state === "connected" ? (
              <span className="welcome-pill is-ok">Connected</span>
            ) : a.state === "covered" ? (
              <span className="welcome-pill is-ok">Uses {a.covered_by}'s skill</span>
            ) : a.state === "outdated" ? (
              <span className="welcome-pill">From another version</span>
            ) : a.state === "theirs" ? (
              <span className="welcome-pill">Has a skill of your own</span>
            ) : busy === a.id ? (
              <span className="welcome-pill">Connecting…</span>
            ) : null;
          return (
            <label
              key={a.id}
              className={`welcome-agent ${connectable(a) ? "" : "is-fixed"}`}
              data-welcome-agent={a.id}
              data-agent-state={a.state}
            >
              <input
                type="checkbox"
                checked={done || (connectable(a) && !!chosen?.has(a.id))}
                disabled={!connectable(a) || connecting}
                onChange={() => toggle(a.id)}
              />
              <Tile id={a.id} />
              <span className="welcome-agent-text">
                <b>{a.name}</b>
                <span className="mono">
                  {a.state === "covered" ? `Reads ${a.covered_by}'s skills` : where(a.skill)}
                </span>
              </span>
              {pill}
            </label>
          );
        })}
      </div>
      {error ? (
        <div className="welcome-section-body">
          <p className="welcome-error">{error.message}</p>
        </div>
      ) : null}
    </Section>
  );
}

/** Notifications, as Settings › General has them: turning them on is when
 *  macOS is asked, and a system that says no gets its settings page. */
function NotificationsSection({ system, request, openSystemSettings }: ReturnType<typeof useNotificationStatus>) {
  const { settings, update, native } = useSettings();
  const status = system.status;
  const on = settings.notifications.enabled;
  const turn = (enabled: boolean) => {
    void update({ notifications: { enabled } });
    if (enabled && status?.authorization === "not_determined") void request();
  };
  const blocked = native && on && status && !status.shows && status.authorization !== "not_determined";
  return (
    <Section
      name="notifications"
      icon={<Bell size={16} />}
      title="Notifications"
      detail="When an agent asks, even with Pinrail in the background"
      side={<Toggle label="Notifications" checked={on} onChange={turn} />}
    >
      {blocked ? (
        <div className="welcome-section-body welcome-section-row">
          <p>{describeSystem(system)}</p>
          <button type="button" className="chrome-button" onClick={openSystemSettings}>
            Open System Settings
          </button>
        </div>
      ) : null}
    </Section>
  );
}

/** Where one plugin's install stands. */
type Progress = "installing" | "installed" | { error: string };

/** The official plugins to choose from, the recommended ones chosen, and
 *  installing the chosen ones. */
function usePluginChoice(catalog: CatalogEntry[] | null) {
  const [chosen, setChosen] = useState<Set<string> | null>(null);
  const [progress, setProgress] = useState<Record<string, Progress>>({});

  useEffect(() => {
    if (catalog && !chosen) {
      setChosen(new Set(catalog.filter((e) => e.recommended && !e.installed && !e.needs).map((e) => e.id)));
    }
  }, [catalog, chosen]);

  const busy = Object.values(progress).includes("installing");
  const toInstall = (catalog ?? []).filter((e) => chosen?.has(e.id) && !e.installed && !e.needs);
  const toggle = (id: string) =>
    setChosen((c) => {
      const next = new Set(c);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  /** Installs the chosen ones, and says whether all of them installed. */
  const install = async () => {
    let all = true;
    for (const e of toInstall) {
      setProgress((p) => ({ ...p, [e.id]: "installing" }));
      try {
        await api.installPlugin({ id: e.id });
        setProgress((p) => ({ ...p, [e.id]: "installed" }));
      } catch (error) {
        all = false;
        setProgress((p) => ({ ...p, [e.id]: { error: failure(error, "It did not install") } }));
      }
    }
    return all;
  };

  return { chosen, progress, busy, toInstall, toggle, install };
}

function PluginsStep({
  catalog,
  choice,
}: {
  catalog: CatalogEntry[] | null;
  choice: ReturnType<typeof usePluginChoice>;
}) {
  const { chosen, progress, busy, toggle } = choice;
  return (
    <>
      <h2>Choose your plugins</h2>
      <p>
        Each plugin is one kind of review an agent can ask you for. Start with the recommended ones, and add others in
        Settings › Plugins whenever an agent needs them.
      </p>
      <div className="welcome-plugins" data-welcome-plugins>
        {[...(catalog ?? [])]
          .sort((a, b) => Number(b.recommended) - Number(a.recommended))
          .map((e) => {
            const state = progress[e.id];
            return (
              <label
                key={e.id}
                className={`welcome-plugin ${e.needs ? "is-unavailable" : ""}`}
                data-welcome-plugin={e.name}
              >
                <input
                  type="checkbox"
                  checked={!!e.installed || !!chosen?.has(e.id)}
                  disabled={!!e.installed || !!e.needs || busy}
                  onChange={() => toggle(e.id)}
                />
                <span className="welcome-section-icon">
                  <PluginIcon icon={e.icon} size={16} />
                </span>
                <span className="welcome-plugin-text">
                  <span className="welcome-plugin-title">
                    {e.title}
                    {e.recommended ? <span className="welcome-pill">Recommended</span> : null}
                  </span>
                  {(e.description ?? e.use_when) ? <span className="dim">{e.description ?? e.use_when}</span> : null}
                </span>
                <span className="welcome-plugin-state">
                  {e.needs ? (
                    <span className="faint">Needs Pinrail {e.needs}</span>
                  ) : state === "installing" ? (
                    <span className="dim">Installing…</span>
                  ) : e.installed ? (
                    <span className="welcome-ok">
                      <Check size={12} strokeWidth={3} /> Installed
                    </span>
                  ) : typeof state === "object" ? (
                    <span className="danger">{state.error}</span>
                  ) : null}
                </span>
              </label>
            );
          })}
      </div>
    </>
  );
}

/** The installed plugins in the order Try it lists them: those with a
 *  prompt first, in the order of the prompts, then the others by title. */
const ORDER = Object.keys(PROMPTS);
function tryOrder(plugins: Plugin[]) {
  const rank = (p: Plugin) => (ORDER.includes(p.name) ? ORDER.indexOf(p.name) : ORDER.length);
  return plugins.filter((p) => p.usable).sort((a, b) => rank(a) - rank(b) || a.title.localeCompare(b.title));
}

function TryRow({ plugin, onSent }: { plugin: Plugin; onSent: (id: string) => void }) {
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const prompt = PROMPTS[plugin.name];
  const send = async () => {
    setSending(true);
    setError(null);
    try {
      onSent((await api.sendSample(plugin.name)).id);
    } catch (e) {
      setError(failure(e, "The sample was not sent"));
    } finally {
      setSending(false);
    }
  };
  return (
    <div className="welcome-try" data-welcome-try={plugin.name}>
      <span className="welcome-section-icon">
        <PluginIcon icon={plugin.icon} size={16} />
      </span>
      <span className="welcome-try-text">
        <b>{plugin.title}</b>
        {prompt ? (
          <span className="welcome-try-prompt">“{prompt}”</span>
        ) : (
          <span>Send yourself its sample to see it.</span>
        )}
        {error ? <span className="welcome-error">{error}</span> : null}
      </span>
      {prompt ? <CopyButton text={prompt} /> : null}
      {plugin.samples?.length ? (
        <button type="button" className="chrome-button" onClick={send} disabled={sending}>
          {sending ? "Sending…" : "Sample"}
        </button>
      ) : null}
    </div>
  );
}

/** Where the setup picks up: the step, and the first review once it came. */
export type WelcomeAt = { step: number; sample?: string; decided?: boolean };

export function WelcomeDialog({
  at,
  onClose,
  onOpenReview,
}: {
  at: WelcomeAt;
  onClose: () => void;
  onOpenReview: (id: string) => void;
}) {
  const live = useLive();
  const cli = useCli(true);
  const notifications = useNotificationStatus(true);
  const [step, setStep] = useState(at.step);
  const [sample, setSample] = useState<string | null>(at.sample ?? null);

  // the official plugins the app carries, with what is installed
  const [catalog, setCatalog] = useState<CatalogEntry[] | null>(null);
  const loadCatalog = () =>
    api
      .catalog()
      .then(({ plugins }) => setCatalog(plugins))
      .catch(() => {});
  useEffect(() => {
    void loadCatalog();
  }, [live.plugins]);
  const choice = usePluginChoice(catalog);

  // the dialog holds the keyboard: Esc skips, and no key reaches the screen
  // behind it; buttons, Tab and copying still work, being the browser's own
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      e.stopPropagation();
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  // the first review: any review that arrives while the setup is open
  const [opened] = useState(() => new Date().toISOString().slice(0, 19));
  useEffect(() => {
    if (sample) return;
    const found = live.pending.find((r) => r.created_at >= opened);
    if (found) setSample(found.id);
  }, [live.pending, sample, opened]);
  const arrived = sample ? live.pending.find((r) => r.id === sample) : undefined;

  const installed = tryOrder([...live.plugins.values()]);
  const hasPlugins = installed.length > 0 || !!catalog?.some((e) => e.installed);
  const done = [found(cli.status, cli.checking || cli.busy) === "ours", hasPlugins, !!sample];
  const locked = [false, false, !hasPlugins];
  const last = step === STEPS.length - 1;

  const installAndGo = async () => {
    const all = await choice.install();
    await loadCatalog();
    if (all) setStep(2);
  };

  let next: ReactNode;
  if (step === 1 && choice.toInstall.length > 0) {
    const n = choice.toInstall.length;
    next = (
      <button
        type="button"
        className="chrome-button button-primary"
        onClick={installAndGo}
        disabled={choice.busy}
        data-welcome-install
      >
        {choice.busy ? "Installing…" : n === 1 ? "Install 1 plugin" : `Install ${n} plugins`}
      </button>
    );
  } else {
    next = (
      <button
        type="button"
        className="chrome-button button-primary"
        onClick={() => (last ? onClose() : setStep(step + 1))}
        disabled={!last && locked[step + 1]}
        data-welcome-next
      >
        {last ? "Done" : "Next"}
      </button>
    );
  }

  return (
    <div className="app-dialog-backdrop">
      <div className="welcome-dialog" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
        <aside className="welcome-rail">
          <Mark />
          <h1 id="welcome-title">Set up Pinrail</h1>
          <p>Agents ask here before they act. You decide, and they carry on with your answer.</p>
          <ol>
            {STEPS.map(({ label, detail }, i) => (
              <li key={label} className={done[i] ? "is-done" : ""}>
                <button
                  type="button"
                  className={i === step ? "is-current" : ""}
                  aria-current={i === step ? "step" : undefined}
                  disabled={locked[i]}
                  onClick={() => setStep(i)}
                  data-welcome-step={label}
                >
                  <span className={`welcome-dot ${done[i] ? "is-done" : ""} ${locked[i] ? "is-locked" : ""}`}>
                    {done[i] ? (
                      <Check size={11} strokeWidth={3} />
                    ) : locked[i] ? (
                      <Lock size={10} strokeWidth={2.5} />
                    ) : (
                      i + 1
                    )}
                  </span>
                  <span className="welcome-step-text">
                    <span className="welcome-step-label">{label}</span>
                    <span className="welcome-step-detail">{locked[i] ? "Install a plugin first" : detail}</span>
                  </span>
                </button>
              </li>
            ))}
          </ol>
        </aside>

        <section className="welcome-main">
          <div className="welcome-content" key={step}>
            <span className="welcome-eyebrow">
              Step {step + 1} of {STEPS.length}
            </span>

            {step === 0 ? (
              <>
                <h2>Connect Pinrail</h2>
                <p>
                  Agents ask through the <code>pinrail</code> command. A short skill tells each agent how and when to
                  ask you.
                </p>
                <CommandSection cli={cli} />
                <AgentsSection />
                <NotificationsSection {...notifications} />
              </>
            ) : null}

            {step === 1 ? <PluginsStep catalog={catalog} choice={choice} /> : null}

            {step === FIRST_REVIEW ? (
              <>
                <h2>Try it</h2>
                <p>Give your agent one of these, or send yourself a sample to see a plugin first.</p>
                <div className="welcome-tries">
                  {installed.map((p) => (
                    <TryRow key={p.name} plugin={p} onSent={setSample} />
                  ))}
                </div>
                {at.decided && sample === at.sample ? (
                  <div className="welcome-wait is-done" role="status">
                    <span className="welcome-wait-mark">
                      <Check size={12} strokeWidth={3} />
                    </span>
                    Decided. An agent that asks gets your answer the same way.
                  </div>
                ) : sample ? (
                  <div className="welcome-wait is-done" role="status">
                    <span className="welcome-wait-mark">
                      <Check size={12} strokeWidth={3} />
                    </span>
                    <span>
                      Arrived: <b>{arrived?.title ?? "your first review"}</b>
                    </span>
                    <button type="button" className="chrome-button button-primary" onClick={() => onOpenReview(sample)}>
                      Open the review
                    </button>
                  </div>
                ) : (
                  <div className="welcome-wait" role="status">
                    <WaitMark size={20} />
                    Waiting for a review to arrive
                  </div>
                )}
                {sample && !(at.decided && sample === at.sample) ? (
                  <p>Decide it and hand it over. The setup comes back here once you have decided.</p>
                ) : null}
              </>
            ) : null}
          </div>

          <footer className="welcome-foot">
            <button type="button" className="welcome-skip" onClick={onClose}>
              {last ? "Close" : "Skip setup"}
            </button>
            <div className="welcome-nav">
              {step > 0 ? (
                <button type="button" className="chrome-button" onClick={() => setStep(step - 1)}>
                  Back
                </button>
              ) : null}
              {next}
            </div>
          </footer>
        </section>
      </div>
    </div>
  );
}
