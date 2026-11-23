// The setup's second screen: the parts of the setup as pins on a line, each
// filling as it finishes. It runs by itself. A part that failed says why,
// and the screen then waits for the person.

import { Bell, Blocks, Bot, Check, Copy, SquareTerminal } from "lucide-react";
import type { ReactNode } from "react";
import { useCopy } from "../../lib/useCopy";
import { agentOf } from "../AgentIcon";
import { Mark } from "./Mark";
import { listed } from "./names";
import type { Part } from "./useSetupRun";

const PATH_LINE = `export PATH="$HOME/.local/bin:$PATH"`;

function icon(part: Part): ReactNode {
  if (part.kind === "command") return <SquareTerminal size={16} />;
  if (part.kind === "plugins") return <Blocks size={16} />;
  if (part.kind === "notifications") return <Bell size={16} />;
  const agent = part.kind === "agent" && part.agents?.[0] ? agentOf(part.agents[0].id) : null;
  return agent ? (
    <span className="setup-step-agent" aria-hidden="true" dangerouslySetInnerHTML={{ __html: agent.svg }} />
  ) : (
    <Bot size={16} />
  );
}

function name(part: Part): ReactNode {
  if (part.kind === "command")
    return (
      <>
        The <code>pinrail</code> command
      </>
    );
  if (part.kind === "notifications") return "Notifications";
  if (part.kind === "plugins") {
    const titles = (part.plugins ?? []).map((e) => e.title);
    return titles.length > 3 ? (
      `${titles.length} plugins`
    ) : (
      <>
        {listed(titles)} {titles.length === 1 ? "plugin" : "plugins"}
      </>
    );
  }
  if (part.kind === "agent") return part.agents?.[0]?.name;
  return part.agents?.length ? `${part.agents.length} agents` : "Agents";
}

/** What a part says while it has nothing more exact to say. */
const WORKING: Record<Part["kind"], string> = {
  command: "Installing…",
  agent: "Connecting…",
  agents: "Connecting…",
  plugins: "Installing…",
  notifications: "Asking…",
};

/** The line to add to a shell profile, with a button that copies it. */
function PathLine({ line }: { line: string }) {
  const { state, copy } = useCopy(1600);
  return (
    <>
      <div className="setup-step-line">
        <code>{line}</code>
      </div>
      <button type="button" className="chrome-button" onClick={() => void copy(line)}>
        {state === "done" ? <Check size={13} /> : <Copy size={13} />} {state === "done" ? "Copied" : "Copy the line"}
      </button>
    </>
  );
}

export function SetupRun({
  parts,
  running,
  finished,
  failed,
  onRetry,
  onRecheck,
  onAddToPath,
  onContinue,
}: {
  parts: Part[];
  running: boolean;
  /** whether the run has ended */
  finished: boolean;
  /** the parts that failed, once the run has ended */
  failed: Part[];
  onRetry: () => void;
  onRecheck: () => void;
  onAddToPath: () => void;
  onContinue: () => void;
}) {
  const stopped = !running && failed.length > 0;
  // every part is done, and the command is only in terminals opened from now on
  const newTerminal = !running && !stopped && parts.some((p) => p.newTerminal);
  // every part is done: the screen waits for Continue
  const ready = finished && !running && !stopped;
  // the command is checked again; anything else is tried again
  const retriable = failed.some((p) => !p.fix);
  const onlyPath = failed.length === 1 && failed[0].fix === "path";

  return (
    <>
      <div className="setup-body">
        <div className="setup-run-head" aria-live="polite">
          {stopped ? null : (
            <span className="setup-run-mark">
              <Mark size={44} />
            </span>
          )}
          <h1 className={`setup-title ${stopped ? "is-view" : ""}`}>
            {ready
              ? "Pinrail is set up"
              : !stopped
                ? "Setting up Pinrail"
                : failed.length === 1
                  ? "One step is left"
                  : `${failed.length} steps are left`}
          </h1>
          <p className="setup-sub">
            {newTerminal ? (
              <>
                Open a new terminal to use the <code>pinrail</code> command. The terminals that are open now do not have
                it yet.
              </>
            ) : ready ? (
              "Everything is ready. Continue to start your first session."
            ) : !stopped ? (
              "This takes a few seconds. You do not need to do anything."
            ) : onlyPath ? (
              <>
                A new terminal cannot find the <code>pinrail</code> command yet.
              </>
            ) : (
              "Pinrail could not finish every step. Try again, or continue and finish it later in Settings."
            )}
          </p>
        </div>

        <ol className="setup-steps">
          {parts.map((part) => (
            <li key={part.key} className={`setup-step is-${part.state}`} data-testid={`setup-step-${part.key}`}>
              <span className="setup-step-node">
                {part.state === "done" ? <Check size={14} strokeWidth={2.6} /> : part.state === "failed" ? "!" : null}
              </span>
              <span className="setup-step-icon">{icon(part)}</span>
              <span className="setup-step-name">{name(part)}</span>
              <span className="setup-step-state">
                {part.text ?? (part.state === "working" ? WORKING[part.kind] : null)}
              </span>
              {part.state === "failed" ? (
                <div className="setup-step-fix">
                  {part.fix === "path" ? (
                    <p>
                      Pinrail installed it in <code>~/.local/bin</code>. Add this line to{" "}
                      {part.profile ? (
                        <code>{part.profile}</code>
                      ) : (
                        <>
                          your shell profile, such as <code>~/.zshrc</code>
                        </>
                      )}
                      , then open a new terminal.
                    </p>
                  ) : (
                    <p>{part.reason}</p>
                  )}
                  {/* why the line could not be added */}
                  {part.fix === "path" && part.reason ? <p className="setup-step-error">{part.reason}</p> : null}
                  {part.fix ? (
                    <div className="setup-step-actions">
                      {part.fix === "path" ? <PathLine line={part.line ?? PATH_LINE} /> : null}
                      {part.fix === "path" && part.profile ? (
                        <button
                          type="button"
                          className="chrome-button button-primary"
                          onClick={onAddToPath}
                          disabled={running}
                          aria-label={`Add it to ${part.profile}`}
                        >
                          {/* a long path, such as fish's, names the file alone, to keep the buttons on one line */}
                          Add it to {part.profile.length > 16 ? part.profile.split("/").pop() : part.profile}
                        </button>
                      ) : null}
                      <button type="button" className="chrome-button" onClick={onRecheck} disabled={running}>
                        Check again
                      </button>
                    </div>
                  ) : null}
                </div>
              ) : null}
            </li>
          ))}
        </ol>
      </div>

      {stopped || ready ? (
        <div className="setup-foot">
          <div className="setup-actions">
            {retriable ? (
              <button type="button" className="chrome-button setup-button" onClick={onRetry}>
                Try again
              </button>
            ) : null}
            <button type="button" className="chrome-button button-primary setup-button" onClick={onContinue} autoFocus>
              Continue
            </button>
          </div>
        </div>
      ) : null}
    </>
  );
}
