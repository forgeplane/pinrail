// Setting Pinrail up, as a short wizard: the pinrail command, a first
// review, notifications, and words for an agent's instructions. Each step
// ticks itself in the rail when it is done, and none has to be. Pinrail
// opens it until it is finished or skipped; the command palette brings it
// back.

import { Check, Copy, ExternalLink } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { inTauri } from "../../api/client";
import { copyText } from "../../lib/clipboard";
import { openExternal } from "../../lib/native";
import { useLive } from "../../state/live";
import { describeSystem, useNotificationStatus } from "../../state/notifications";
import { useSettings } from "../../state/settings";
import { Toggle } from "../settings/controls";
import { SettingsRow } from "../settings/layout";
import { useCli, type CliStatus } from "../settings/CliRow";

// the list plugin is built in, so its sample is there on every install
const TRY = "pinrail submit list --sample --wait";

/** Prompts that leave the how to the agent: `plugins describe` teaches it
 * the rest. Both work with the built-in list plugin. */
const TRY_NOW =
  "Find the TODOs in this repository and ask me through Pinrail which to tackle first. Run `pinrail docs` to see how.";
const HABIT =
  "Write a skill that asks me through Pinrail to approve the changes before you commit. Run `pinrail docs` to see how.";

const BUILD = "https://pinrail.dev/docs/building/writing/";
const PATH_LINE = `export PATH="$HOME/.local/bin:$PATH"`;

const STEPS = ["The command", "A first review", "Notifications", "Your agent"] as const;

/** The app's mark: pin, slash, pin. */
function Mark() {
  return (
    <svg className="welcome-logo" viewBox="0 0 824 824" aria-hidden="true">
      <rect width="824" height="824" rx="185" fill="#221f1c" />
      <rect x="6" y="6" width="812" height="812" rx="180" fill="none" stroke="#f1ebdf" strokeOpacity="0.12" strokeWidth="12" />
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

/** The mark while waiting: the slash swings on the rail, between the pins. */
function WaitMark() {
  return (
    <svg className="welcome-wait-logo" viewBox="0 0 24 24" aria-hidden="true">
      <rect x="1.5" y="13" width="21" height="3.4" rx="1.4" fill="currentColor" />
      <rect x="4.8" y="5" width="2.4" height="16" rx="1.2" fill="currentColor" />
      <rect x="16.8" y="5" width="2.4" height="16" rx="1.2" fill="currentColor" />
      <rect className="welcome-wait-slash" x="10.8" y="4.2" width="2.4" height="16.8" rx="1.2" fill="#e5694f" />
    </svg>
  );
}

function CopyButton({ text, label = "Copy", onCopied, dark }: { text: string; label?: string; onCopied?: () => void; dark?: boolean }) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const t = window.setTimeout(() => setCopied(false), 1600);
    return () => window.clearTimeout(t);
  }, [copied]);
  return (
    <button
      type="button"
      className={dark ? "welcome-term-button" : "chrome-button"}
      onClick={() =>
        copyText(text)
          .then(() => {
            setCopied(true);
            onCopied?.();
          })
          .catch(() => {})
      }
    >
      {copied ? <Check size={13} /> : <Copy size={13} />} {copied ? "Copied" : label}
    </button>
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

function Terminal({ status, state }: { status: CliStatus | null; state: Found }) {
  return (
    <div className="welcome-term" aria-live="polite">
      <div>
        <span className="welcome-term-prompt">$</span> command -v pinrail
      </div>
      {state === "checking" ? (
        <div className="welcome-term-dim">
          looking in a new terminal<span className="welcome-term-cursor" />
        </div>
      ) : state === "missing" ? (
        <div className="welcome-term-dim">nothing: pinrail is not on your PATH yet</div>
      ) : state === "unknown" ? (
        <div className="welcome-term-dim">your shell did not answer in time</div>
      ) : (
        <div>
          {status?.runs}
          <span className={state === "ours" ? "welcome-term-ok" : "welcome-term-warn"}>{state === "ours" ? "  ✓" : "  !"}</span>
        </div>
      )}
    </div>
  );
}

function CommandStep({ cli }: { cli: ReturnType<typeof useCli> }) {
  const native = inTauri();
  const status = cli.status;
  const state = found(status, cli.checking || cli.busy);
  const recheck = () => void cli.check();

  if (!native) {
    return (
      <>
        <h2>The pinrail command</h2>
        <p>Agents ask through a command, <code>pinrail</code>. The app checks here whether a terminal can run it.</p>
      </>
    );
  }

  let body: ReactNode = null;
  let action: ReactNode = null;
  if (state === "ours") {
    body = cli.status?.bundled ? <p>A terminal runs this Pinrail's command, so any agent that can run commands can ask.</p> : <p>A terminal finds it, so any agent that can run commands can ask.</p>;
  } else if (state === "missing" && status?.installed && status.dir_on_path === false) {
    body = (
      <>
        <p>
          It's installed in <code>~/.local/bin</code>, but that folder isn't on your PATH. Add this line to your shell profile, <code>~/.zshrc</code> for zsh, then check again:
        </p>
        <div className="welcome-line">
          <code>{PATH_LINE}</code>
          <CopyButton text={PATH_LINE} />
        </div>
      </>
    );
    action = (
      <button type="button" className="chrome-button" onClick={recheck}>
        Check again
      </button>
    );
  } else if (state === "missing" || state === "unknown" || (state === "other" && !status?.installed)) {
    if (status?.bundled && status.mode !== "package") {
      body =
        state === "other" ? (
          <p>
            A terminal finds another <code>pinrail</code> first. Install this Pinrail's command into <code>~/.local/bin</code>, and put that folder before the other one on your PATH.
          </p>
        ) : (
          <p>
            Pinrail carries the command. Installing {status.mode === "copy" ? "copies" : "links"} it into <code>~/.local/bin</code>, where your terminal and your agents find it.
          </p>
        );
      action = (
        <button type="button" className="chrome-button button-primary" onClick={cli.install} disabled={cli.busy}>
          {cli.busy ? "Installing…" : status.outdated ? "Update the command" : "Install the command"}
        </button>
      );
    } else {
      body = (
        <p>
          This build of Pinrail doesn't carry the command. Build it from a checkout with <code>cargo install --path cli</code>, then check again.
        </p>
      );
      action = (
        <button type="button" className="chrome-button" onClick={recheck}>
          Check again
        </button>
      );
    }
  } else if (state === "other") {
    body = (
      <p>
        This Pinrail's command is installed at <code>{status?.link}</code>, but a terminal finds the one above first. Put <code>~/.local/bin</code> earlier on your PATH, then check again.
      </p>
    );
    action = (
      <button type="button" className="chrome-button" onClick={recheck}>
        Check again
      </button>
    );
  }

  return (
    <>
      <h2>The pinrail command</h2>
      <p>Agents ask through a command, <code>pinrail</code>. Here is what a new terminal finds:</p>
      <Terminal status={status} state={state} />
      {body}
      {cli.error ? <p className="welcome-error">{cli.error}</p> : null}
      {action ? <div className="welcome-actions">{action}</div> : null}
    </>
  );
}

/** Notifications, as Settings › General has them: turning them on is when
 * macOS is asked, and a system that says no gets its settings page. */
function NotificationsStep({ system, request, openSystemSettings }: ReturnType<typeof useNotificationStatus>) {
  const { settings, update, native } = useSettings();
  const status = system.status;
  const on = settings.notifications.enabled;
  const turn = (enabled: boolean) => {
    void update({ notifications: { enabled } });
    if (enabled && status?.authorization === "not_determined") void request();
  };
  return (
    <>
      <h2>Turn on notifications</h2>
      <p>So you know when an agent is waiting, even with the window closed. The menu bar counts what waits either way.</p>
      <div className="settings-card">
        <SettingsRow label="Notify me when a review arrives" description="A system notification for each new review">
          <Toggle label="Notify me when a review arrives" checked={on} onChange={turn} />
        </SettingsRow>
        <SettingsRow label="Play a sound" description="The system's notification sound with each one">
          <Toggle label="Play a sound" checked={on && settings.notifications.sound} disabled={!on} onChange={(sound) => update({ notifications: { sound } })} />
        </SettingsRow>
        {native && status ? (
          <SettingsRow label="macOS" description={describeSystem(system)}>
            {status.authorization === "not_determined" ? (
              <button type="button" className="chrome-button" onClick={() => turn(true)}>
                Allow
              </button>
            ) : !status.shows ? (
              <button type="button" className="chrome-button" onClick={openSystemSettings}>
                Open System Settings
              </button>
            ) : null}
          </SettingsRow>
        ) : null}
      </div>
    </>
  );
}

/** Two prompts to paste into an agent, and where to go from here. */
function AgentStep({ onCopied, onPlugins }: { onCopied: () => void; onPlugins: () => void }) {
  return (
    <>
      <h2>Put your agent to work</h2>
      <p>Paste one of these into your agent. It works out the rest itself: <code>pinrail plugins describe</code> tells it how.</p>
      <p className="welcome-caption">Try it now</p>
      <div className="welcome-term welcome-term-copy welcome-term-prose">
        <div>{TRY_NOW}</div>
        <CopyButton text={TRY_NOW} dark onCopied={onCopied} />
      </div>
      <p className="welcome-caption">Make it a habit</p>
      <div className="welcome-term welcome-term-copy welcome-term-prose">
        <div>{HABIT}</div>
        <CopyButton text={HABIT} dark onCopied={onCopied} />
      </div>
      <p className="welcome-next">
        Pinrail comes with two plugins, <b>list</b> and <b>feedback</b>.{" "}
        <button type="button" className="welcome-link" onClick={onPlugins}>
          Add more
        </button>{" "}
        for code reviews, emails and designs, or{" "}
        <button type="button" className="welcome-link" onClick={() => openExternal(BUILD)}>
          build your own <ExternalLink size={12} />
        </button>
        .
      </p>
    </>
  );
}

/** Where the setup picks up: the step, and the first review once it came. */
export type WelcomeAt = { step: number; sample?: string; decided?: boolean };

export function WelcomeDialog({ at, onClose, onOpenReview, onPlugins }: { at: WelcomeAt; onClose: () => void; onOpenReview: (id: string) => void; onPlugins: () => void }) {
  const live = useLive();
  const cli = useCli(true);
  const { system, request, openSystemSettings } = useNotificationStatus(true);
  const [step, setStep] = useState(at.step);
  const [sample, setSample] = useState<string | null>(at.sample ?? null);
  const [told, setTold] = useState(false);

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

  // the first review: a list review that arrives while the setup is open
  const [opened] = useState(() => new Date().toISOString().slice(0, 19));
  useEffect(() => {
    if (sample) return;
    const found = live.pending.find((r) => r.plugin === "list" && r.created_at >= opened);
    if (found) setSample(found.id);
  }, [live.pending, sample, opened]);
  const arrived = sample ? live.pending.find((r) => r.id === sample) : undefined;

  const { settings } = useSettings();
  const status = system.status;
  const done = [
    !!cli.status?.runs && (!cli.status.bundled || cli.status.installed),
    !!sample,
    // without macOS's word (Linux, a development build) notifications just work
    settings.notifications.enabled && system.known && (!status || status.shows),
    told,
  ];
  const last = step === STEPS.length - 1;

  return (
    <div className="app-dialog-backdrop">
      <div className="welcome-dialog" role="dialog" aria-labelledby="welcome-title">
        <aside className="welcome-rail">
          <Mark />
          <h1 id="welcome-title">Set up Pinrail</h1>
          <p>Agents ask here before they act. You decide, and they carry on with your answer.</p>
          <ol>
            {STEPS.map((label, i) => (
              <li key={label}>
                <button type="button" className={i === step ? "is-current" : ""} aria-current={i === step ? "step" : undefined} onClick={() => setStep(i)}>
                  <span className={`welcome-dot ${done[i] ? "is-done" : ""}`}>{done[i] ? <Check size={11} strokeWidth={3} /> : i + 1}</span>
                  {label}
                </button>
              </li>
            ))}
          </ol>
        </aside>

        <section className="welcome-main">
          <div className="welcome-content" key={step}>
            {step === 0 ? <CommandStep cli={cli} /> : null}

            {step === 1 ? (
              <>
                <h2>Send yourself a review</h2>
                <p>Be the agent for a minute. Run this in a terminal: it sends the list plugin's sample review, and waits for your decision.</p>
                <div className="welcome-term welcome-term-copy">
                  <div>
                    <span className="welcome-term-prompt">$</span> {TRY}
                  </div>
                  <CopyButton text={TRY} dark />
                </div>
                {at.decided && sample === at.sample ? (
                  <div className="welcome-wait is-done" role="status">
                    <span className="welcome-wait-mark">
                      <Check size={12} strokeWidth={3} />
                    </span>
                    Decided. Your terminal has the answer, as an agent reads it.
                  </div>
                ) : sample ? (
                  <>
                    <div className="welcome-wait is-done" role="status">
                      <span className="welcome-wait-mark">
                        <Check size={12} strokeWidth={3} />
                      </span>
                      <span>
                        Arrived: <b>{arrived?.title ?? "your first review"}</b>
                      </span>
                    </div>
                    <p>Open it, accept or reject its items, and hand it over. Then look at your terminal: it prints your decision. Setup picks up here once you've decided.</p>
                    <div className="welcome-actions">
                      <button type="button" className="chrome-button button-primary" onClick={() => onOpenReview(sample)}>
                        Open the review
                      </button>
                    </div>
                  </>
                ) : (
                  <div className="welcome-wait" role="status">
                    <WaitMark />
                    Waiting for the review to arrive
                  </div>
                )}
              </>
            ) : null}

            {step === 2 ? <NotificationsStep system={system} request={request} openSystemSettings={openSystemSettings} /> : null}

            {step === 3 ? <AgentStep onCopied={() => setTold(true)} onPlugins={onPlugins} /> : null}
          </div>

          <footer className="welcome-foot">
            <button type="button" className="welcome-skip" onClick={onClose}>
              {last ? "Close" : "Skip setup"}
            </button>
            <span className="welcome-count">
              {step + 1} of {STEPS.length}
            </span>
            <div className="welcome-nav">
              {step > 0 ? (
                <button type="button" className="chrome-button" onClick={() => setStep(step - 1)}>
                  Back
                </button>
              ) : null}
              <button type="button" className="chrome-button button-primary" onClick={() => (last ? onClose() : setStep(step + 1))} data-welcome-next>
                {last ? "Done" : "Next"}
              </button>
            </div>
          </footer>
        </section>
      </div>
    </div>
  );
}
