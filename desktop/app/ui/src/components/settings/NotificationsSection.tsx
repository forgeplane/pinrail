// The Notifications section: what the inbox collects and what raises a
// system notification, kind by kind, the muted projects and repositories,
// and when system notifications are on.

import { Bell, FolderGit2, Plus, Settings, X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { api } from "../../api/client";
import type { ProjectsListing } from "../../api/types";
import { type Announce, type InboxShow, useSettings } from "../../state/settings";
import { describeSystem, useNotificationStatus } from "../../state/notifications";
import { Select } from "../Select";
import { useOptionsMenu } from "../OptionsMenu";
import { ProjectAvatar } from "../ProjectAvatar";
import { Toggle } from "./controls";
import { SettingsGroup, SettingsPage, SettingsRow } from "./layout";

/** A kind of notification: its inbox switch, or none when it always
 *  waits there, and its system notification switch, if it has one. */
type Kind = { label: string; description: string; inbox?: keyof InboxShow; system?: keyof Announce };

const KINDS: { caption: string; kinds: Kind[] }[] = [
  {
    caption: "Reviews and agents",
    kinds: [
      { label: "Reviews", description: "A new review or a new round of one.", system: "review_requested" },
      { label: "Questions", description: "A question an agent asks you.", system: "question_asked" },
      {
        label: "Agent waits",
        description: "An agent waits for a permission or an answer in its terminal.",
        system: "agent_waits",
      },
      {
        label: "Agent finished",
        description: "An agent finished or failed its turn. Its system notification covers a stopped agent too.",
        inbox: "agent_finished",
        system: "agent_finished",
      },
      { label: "Agent stopped", description: "An agent's session stopped.", inbox: "agent_stopped" },
      { label: "Agent notes", description: "A note an agent adds to a review it sent.", inbox: "agent_note" },
      {
        label: "Ended reviews",
        description: "A review that its agent withdrew, or that expired.",
        inbox: "review_ended",
      },
    ],
  },
  {
    caption: "Work items",
    kinds: [
      { label: "New work items", description: "An agent adds a work item.", inbox: "item_added" },
      {
        label: "Comments",
        description: "An agent comments on a work item.",
        inbox: "item_commented",
        system: "item_commented",
      },
      {
        label: "Closed work items",
        description: "An agent marks a work item done or cancels it.",
        inbox: "item_closed",
      },
      { label: "Claimed work items", description: "An agent claims a work item.", inbox: "item_claimed" },
      { label: "Other changes", description: "An agent makes any other change to a work item.", inbox: "item_changed" },
    ],
  },
];

/** The pause as a moment still to come, or null. */
const pausedUntil = (iso: string | null) => {
  if (!iso) return null;
  const t = new Date(iso);
  return t.getTime() > Date.now() ? t : null;
};

const clock = (t: Date) => t.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

/** What a choice in the pause menu means, as a timestamp or null. */
const pauseUntil = (choice: string): string | null => {
  const now = new Date();
  if (choice === "15") return new Date(now.getTime() + 15 * 60_000).toISOString();
  if (choice === "60") return new Date(now.getTime() + 60 * 60_000).toISOString();
  if (choice === "tomorrow")
    return new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 0, 0).toISOString();
  return null;
};

/** A project or a repository that can be muted, as the filters show it. */
type Scope = { value: string; label: string; mark: ReactNode };

/** The projects first, with their avatars, then the repositories, then
 *  none, as the list filters show them. */
function scopes(listing: ProjectsListing | null): Scope[] {
  const projects = [...(listing?.projects ?? [])].sort((a, b) => a.name.localeCompare(b.name));
  const repos = [
    ...new Set([...projects.flatMap((p) => p.repos.map((r) => r.repo)), ...(listing?.loose ?? []).map((r) => r.repo)]),
  ].sort((a, b) => a.localeCompare(b));
  return [
    ...projects.map((p) => ({
      value: `project:${p.id}`,
      label: p.name,
      mark: <ProjectAvatar project={p} size={16} />,
    })),
    ...repos.map((r) => ({ value: r, label: r, mark: <FolderGit2 size={14} /> })),
    { value: "-", label: "No repository", mark: <FolderGit2 size={14} /> },
  ];
}

/** A muted entry as the filters show it, for one no longer listed too. */
const scopeOf = (value: string, all: Scope[]): Scope =>
  all.find((s) => s.value === value) ?? {
    value,
    label: value.startsWith("project:") ? "A deleted project" : value,
    mark: <FolderGit2 size={14} />,
  };

/** The list filters' menu of projects and repositories, with a check on
 *  each muted one: a pick mutes or unmutes it and leaves the menu open. */
function MuteMenu({
  anchor,
  all,
  muted,
  onChange,
  onClose,
}: {
  anchor: HTMLElement;
  all: Scope[];
  muted: string[];
  onChange: (muted: string[]) => void;
  onClose: () => void;
}) {
  const { menu, pos, list } = useOptionsMenu(anchor, onClose);
  if (!pos) return null;
  return createPortal(
    <div
      ref={menu}
      className="menu-popup item-menu opens-left is-searchable"
      role="menu"
      aria-label="Project or repository"
      // it scrolls inside the window, however many there are
      style={{
        top: pos.top,
        right: window.innerWidth - pos.right,
        maxHeight: Math.min(320, window.innerHeight - pos.top - 8),
        overflowY: "auto",
      }}
    >
      {list(
        "Project or repository",
        all.map((s) => ({
          value: s.value,
          label: s.label,
          mark: s.mark,
          checked: muted.includes(s.value),
          onPick: () => onChange(muted.includes(s.value) ? muted.filter((m) => m !== s.value) : [...muted, s.value]),
        })),
        true,
      )}
    </div>,
    document.body,
  );
}

export function NotificationsSection({ open }: { open: boolean }) {
  const { settings, update } = useSettings();
  const n = settings.notifications;
  const show = settings.inbox.show;
  const muted = settings.inbox.muted_repos;
  const paused = pausedUntil(n.paused_until);
  const { system, request, openSystemSettings } = useNotificationStatus(open);
  const [listing, setListing] = useState<ProjectsListing | null>(null);

  // a pause ends on its own: the row says so within the minute
  const [, tick] = useState(0);
  useEffect(() => {
    if (!open || !paused) return;
    const timer = window.setInterval(() => tick((k) => k + 1), 30_000);
    return () => window.clearInterval(timer);
  }, [open, paused]);

  useEffect(() => {
    if (!open) return;
    api
      .projects()
      .then(setListing)
      .catch(() => setListing(null));
  }, [open]);

  const all = scopes(listing);
  const [menu, setMenu] = useState<HTMLElement | null>(null);
  const mute = (next: string[]) => update({ inbox: { muted_repos: next } });

  return (
    <SettingsPage title="Notifications">
      <SettingsGroup caption="System notifications">
        <SettingsRow
          label="System notifications"
          description="Raise a system notification for the kinds turned on below."
        >
          <Toggle
            label="System notifications"
            checked={n.enabled}
            onChange={(v) => update({ notifications: { enabled: v } })}
          />
        </SettingsRow>
        <SettingsRow
          label="Pause"
          description={
            paused
              ? `Nothing is announced until ${clock(paused)}. The tray's menu shows the pause too.`
              : "Nothing is announced while notifications are paused. The tray's menu can pause them too."
          }
        >
          <Select
            label="Pause notifications"
            icon={<Bell size={14} />}
            value={paused ? "paused" : ""}
            onChange={(v) => update({ notifications: { paused_until: pauseUntil(v) } })}
            options={
              paused
                ? [
                    { value: "paused", label: `Paused until ${clock(paused)}` },
                    { value: "", label: "Resume" },
                  ]
                : [
                    { value: "", label: "Not paused" },
                    { value: "15", label: "For 15 minutes" },
                    { value: "60", label: "For 1 hour" },
                    { value: "tomorrow", label: "Until tomorrow" },
                  ]
            }
          />
        </SettingsRow>
        <SettingsRow
          label="Quiet hours"
          description="Nothing is announced between these times each day. What arrives then still waits in the inbox."
        >
          {n.quiet_hours ? (
            <span className="settings-hours">
              <input
                className="settings-input"
                type="time"
                aria-label="Quiet from"
                value={n.quiet_hours.from}
                onChange={(e) =>
                  e.target.value &&
                  update({ notifications: { quiet_hours: { ...n.quiet_hours!, from: e.target.value } } })
                }
              />
              <span>to</span>
              <input
                className="settings-input"
                type="time"
                aria-label="Quiet until"
                value={n.quiet_hours.to}
                onChange={(e) =>
                  e.target.value &&
                  update({ notifications: { quiet_hours: { ...n.quiet_hours!, to: e.target.value } } })
                }
              />
            </span>
          ) : null}
          <Toggle
            label="Quiet hours"
            checked={n.quiet_hours !== null}
            onChange={(v) => update({ notifications: { quiet_hours: v ? { from: "22:00", to: "07:00" } : null } })}
          />
        </SettingsRow>
        <SettingsRow label="Sound" description="Play the system's notification sound with each one.">
          <Toggle label="Sound" checked={n.sound} onChange={(v) => update({ notifications: { sound: v } })} />
        </SettingsRow>
        <SettingsRow label="System" description={describeSystem(system)}>
          {system.status?.authorization === "not_determined" ? (
            <button type="button" className="chrome-button" onClick={request}>
              Turn on
            </button>
          ) : system.status && !system.status.shows ? (
            <button type="button" className="chrome-button" onClick={openSystemSettings}>
              <Settings size={14} /> Open System Settings
            </button>
          ) : null}
        </SettingsRow>
      </SettingsGroup>

      {KINDS.map(({ caption, kinds }) => (
        <SettingsGroup
          key={caption}
          caption={caption}
          action={
            <span className="settings-columns" aria-hidden>
              <span>Inbox</span>
              <span>System</span>
            </span>
          }
        >
          {kinds.map((kind) => {
            const inInbox = kind.inbox ? show[kind.inbox] : true;
            return (
              <SettingsRow key={kind.label} label={kind.label} description={kind.description}>
                <span className="settings-column">
                  {kind.inbox ? (
                    <Toggle
                      label={`${kind.label} in the inbox`}
                      checked={show[kind.inbox]}
                      onChange={(v) => update({ inbox: { show: { [kind.inbox!]: v } } })}
                    />
                  ) : (
                    <span className="settings-always">Always</span>
                  )}
                </span>
                <span className="settings-column">
                  {kind.system ? (
                    <Toggle
                      label={`${kind.label} as a system notification`}
                      checked={inInbox && n.announce[kind.system]}
                      disabled={!inInbox}
                      onChange={(v) => update({ notifications: { announce: { [kind.system!]: v } } })}
                    />
                  ) : null}
                </span>
              </SettingsRow>
            );
          })}
        </SettingsGroup>
      ))}

      <SettingsGroup
        caption="Muted projects and repositories"
        action={
          <button
            type="button"
            className="chrome-button settings-caption-action"
            aria-haspopup="menu"
            aria-expanded={!!menu}
            onClick={(e) => {
              const anchor = e.currentTarget;
              setMenu((m) => (m ? null : anchor));
            }}
          >
            <Plus size={13} /> Mute
          </button>
        }
      >
        {menu ? <MuteMenu anchor={menu} all={all} muted={muted} onChange={mute} onClose={() => setMenu(null)} /> : null}
        {muted.length ? (
          muted
            .map((value) => scopeOf(value, all))
            .map((scope) => (
              <SettingsRow
                key={scope.value}
                label={scope.label}
                icon={scope.mark}
                description="Only what waits for you is added to the inbox."
              >
                <button
                  type="button"
                  className="bar-button"
                  aria-label={`Unmute ${scope.label}`}
                  onClick={() => mute(muted.filter((m) => m !== scope.value))}
                >
                  <X size={15} />
                </button>
              </SettingsRow>
            ))
        ) : (
          <SettingsRow
            label="None muted"
            description="A muted project or repository adds only what waits for you to the inbox."
          />
        )}
      </SettingsGroup>
    </SettingsPage>
  );
}
