// A session's menu in the sidebar, from the ⋯ button on its row or a
// right-click, and in its header: rename or archive it, mark it read or
// unread, copy its agent's id for it, stop or resume its agent when it runs
// in Pinrail, and delete it, after asking.

import { Archive, ArchiveRestore, Copy, Mail, MailOpen, Pencil, Play, Square, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import { api, errorMessage } from "../api/client";
import { runtime, type Run } from "../api/runtime";
import type { Session } from "../api/types";
import { agentOf } from "./AgentIcon";
import { copyText } from "../lib/clipboard";
import { canResume, resumeProblem } from "../lib/resume";
import { sessionLabel } from "../lib/sessions";
import { useSettings } from "../state/settings";
import { useToast } from "../state/toasts";
import { useToggleArchive } from "./ArchiveButton";
import { Dialog } from "./Dialog";
import { PopoverMenu, type MenuEntry } from "./PopoverMenu";
import { sessionGrid } from "./session/startRun";
import { sessionPath } from "./Thread";

export function SessionMenu({
  session,
  anchor,
  onClose,
  markUnread,
  onRename,
}: {
  session: Session;
  anchor: HTMLElement;
  onClose: () => void;
  /** starts renaming it where it is shown; without, its page opens to rename it */
  onRename?: () => void;
  /** marks it unread where its thread is shown, which keeps it so while it stays shown */
  markUnread?: (() => void) | null;
}) {
  const { settings } = useSettings();
  const runsOn = settings.labs.runtime;
  const toast = useToast();
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const toggleArchive = useToggleArchive();
  const name = agentOf(session.agent)?.label ?? session.agent;
  const named = { agent: session.agent, id: session.id };

  // its runs, to offer Stop or Resume, once they have loaded
  const [runs, setRuns] = useState<Run[] | null>(null);
  useEffect(() => {
    if (!runsOn) return;
    let current = true;
    runtime.runs({ session: `${session.agent}/${session.id}` }).then(
      ({ runs }) => current && setRuns(runs),
      () => current && setRuns([]),
    );
    return () => {
      current = false;
    };
  }, [runsOn, session.agent, session.id]);
  const live = runs?.find((r) => r.state !== "ended" && r.terminal) ?? null;
  const resumable = !!runs && !live && canResume(session, runs);

  // Stop asks first while the agent works on a turn
  const [asking, setAsking] = useState<Run | null>(null);
  const stop = async (run: Run) => {
    try {
      await runtime.stop(run.id);
    } catch (e) {
      toast(`Could not stop ${name}: ${errorMessage(e)}`, "danger");
    }
    setAsking(null);
    onClose();
  };
  const resume = async () => {
    onClose();
    try {
      await runtime.resume(named, { ...(await sessionGrid()), local_id: crypto.randomUUID(), anyway: false });
      navigate(`${sessionPath(named)}?view=terminal`);
    } catch (e) {
      toast(resumeProblem(e, runs?.[0]?.folder ?? session.folder ?? null), "danger");
    }
  };

  // Delete asks first, and leaves the session's page if it shows
  const [deleting, setDeleting] = useState(false);
  const label = sessionLabel(session);
  const remove = async () => {
    try {
      await api.deleteSession(session.agent, session.id);
      toast(`Deleted ${label}`);
      if (pathname === sessionPath(named)) navigate("/sessions");
    } catch (e) {
      toast(`Could not delete ${label}: ${errorMessage(e)}`, "danger");
    }
    onClose();
  };

  // read once its finished turn was seen, unread while it waits to be
  const p = session.presence;
  const seen = !!p?.finished_at && !!p.seen_at && p.seen_at >= p.finished_at;
  const unread = session.attention === "finished";
  const archived = !!session.archived_at;

  const entries: MenuEntry[] = [
    {
      label: "Rename",
      icon: Pencil,
      onPick: () => (onRename ? onRename() : navigate(sessionPath(named), { state: { rename: true } })),
    },
    {
      label: archived ? "Unarchive" : "Archive",
      icon: archived ? ArchiveRestore : Archive,
      onPick: () => toggleArchive(session),
    },
  ];
  if (unread || seen)
    entries.push({
      label: unread ? "Mark as read" : "Mark as unread",
      icon: unread ? MailOpen : Mail,
      onPick: () =>
        !unread && markUnread ? markUnread() : void api.setSeen(session.agent, session.id, unread).catch(() => {}),
    });
  // the agent's own id, which its resume takes, once the agent reported it
  const agentSession = session.agent_session;
  entries.push({
    label: "Copy session id",
    icon: Copy,
    hint: agentSession ? undefined : "Not known yet",
    disabled: agentSession ? undefined : `${name} has not reported its id for this session yet.`,
    onPick: () => {
      if (!agentSession) return;
      void copyText(agentSession).then(
        () => toast("Copied the session id"),
        () => toast("Could not copy the session id", "danger"),
      );
    },
  });
  if (live)
    entries.push(
      { kind: "separator" },
      {
        label: `Stop ${name}`,
        icon: Square,
        danger: true,
        // while the agent works, Stop asks first, in place of the menu
        asks: session.presence?.state === "working",
        onPick: () => (session.presence?.state === "working" ? setAsking(live) : void stop(live)),
      },
    );
  else if (resumable)
    entries.push({ kind: "separator" }, { label: `Resume ${name}`, icon: Play, onPick: () => void resume() });
  entries.push(
    { kind: "separator" },
    { label: "Delete…", icon: Trash2, danger: true, asks: true, onPick: () => setDeleting(true) },
  );

  const keep = useRef<HTMLButtonElement>(null);
  if (deleting)
    return (
      <Dialog labelledBy="menu-delete-title" className="discard-dialog" initialFocus={keep} onClose={onClose}>
        <div className="dialog-head">
          <h2 id="menu-delete-title">Delete {label}?</h2>
        </div>
        <p className="dim">
          {live ? `Pinrail stops ${name} first. ` : ""}Its statuses, notes and messages are deleted, and its questions
          and reviews that still wait are withdrawn. Its decided reviews stay in History, and the work items it holds
          are given up. This cannot be undone.
        </p>
        <div className="dialog-actions">
          <button ref={keep} type="button" className="chrome-button" onClick={onClose}>
            Keep it
          </button>
          <button type="button" className="chrome-button button-danger" onClick={() => void remove()}>
            Delete
          </button>
        </div>
      </Dialog>
    );
  return asking ? (
    <Dialog labelledBy="menu-stop-title" className="discard-dialog" initialFocus={keep} onClose={onClose}>
      <div className="dialog-head">
        <h2 id="menu-stop-title">Stop {name}?</h2>
      </div>
      <p className="dim">
        {name} is working on a turn. Stopping it ends the turn, and {name} exits.
      </p>
      <div className="dialog-actions">
        <button ref={keep} type="button" className="chrome-button" onClick={onClose}>
          Keep it running
        </button>
        <button type="button" className="chrome-button button-danger" onClick={() => void stop(asking)}>
          Stop
        </button>
      </div>
    </Dialog>
  ) : (
    <PopoverMenu
      anchor={anchor}
      label={`${session.name ?? session.title ?? session.generated_name ?? name} actions`}
      entries={entries}
      onClose={onClose}
      align="right"
    />
  );
}
