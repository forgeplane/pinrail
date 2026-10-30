// Archiving a session: it leaves the Sessions page and the sidebar, and new
// activity from its agent brings it back. One button archives it, or brings
// an archived one back.

import { Archive, ArchiveRestore } from "lucide-react";
import { useCallback } from "react";
import { api } from "../api/client";
import type { Session } from "../api/types";
import { useToast } from "../state/toasts";
import { Tooltip } from "./Tooltip";

/** Archives the session, or brings it back; the event stream updates the views. */
export function useToggleArchive(): (session: Pick<Session, "agent" | "id" | "archived_at">) => void {
  const toast = useToast();
  return useCallback(
    (session) => {
      api
        .archiveSession(session.agent, session.id, !session.archived_at)
        .catch(() => toast("Could not archive the session", "danger"));
    },
    [toast],
  );
}

export function ArchiveButton({
  session,
  inRow = false,
}: {
  session: Pick<Session, "agent" | "id" | "archived_at">;
  /** on a list's row rather than in a toolbar */
  inRow?: boolean;
}) {
  const toggle = useToggleArchive();
  const archived = !!session.archived_at;
  const label = archived ? "Unarchive" : "Archive";
  return (
    <Tooltip label={label} keys={["E"]} side={inRow ? "top" : "bottom"}>
      <button
        type="button"
        className="bar-button archive-button"
        aria-label={label}
        data-testid="archive"
        onClick={(event) => {
          // on a row, the click stays off the row's link
          event.preventDefault();
          event.stopPropagation();
          toggle(session);
        }}
      >
        {archived ? <ArchiveRestore size={inRow ? 14 : 15} /> : <Archive size={inRow ? 14 : 15} />}
      </button>
    </Tooltip>
  );
}
