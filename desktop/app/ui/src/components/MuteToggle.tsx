// Whether the person muted a work item's or a session's notifications, and
// the bell in its header that mutes or unmutes them.

import { Bell, BellOff } from "lucide-react";
import { api } from "../api/client";
import { useLoad } from "../lib/useLoad";
import { useNoticeCount } from "../state/live";
import { useToast } from "../state/toasts";
import { Tooltip } from "./Tooltip";

type Subject = { item: string } | { session: { agent: string; id: string } };

/** Whether the subject's notifications are muted, read again when a
 *  notification changes, as when it is muted from the inbox. */
export function useMuted(subject: Subject | null): boolean {
  const changed = useNoticeCount((n) => n.kind === "notification_changed");
  const key = JSON.stringify(subject);
  const found =
    useLoad(() => (subject ? api.notificationOf(subject).catch(() => null) : Promise.resolve(null)), [key, changed])
      .data ?? null;
  return !!found?.muted;
}

/** The bell in a work item's or a session's header: it mutes the subject's
 *  notifications, which no later event brings back, or unmutes them. */
export function MuteToggle({ subject, className = "" }: { subject: Subject; className?: string }) {
  const toast = useToast();
  const muted = useMuted(subject);
  const label = muted ? "Unmute notifications" : "Mute notifications";
  return (
    <Tooltip label={label} side="bottom">
      <button
        type="button"
        className={`${className} ${muted ? "is-on" : ""}`.trim()}
        aria-label={label}
        aria-pressed={muted}
        onClick={() =>
          api.setSubjectMuted(subject, !muted).then(
            () => toast(muted ? "Notifications are on again" : "Notifications are muted"),
            () => toast("Could not change the notifications", "danger"),
          )
        }
      >
        {muted ? <BellOff size={14} /> : <Bell size={14} />}
      </button>
    </Tooltip>
  );
}
