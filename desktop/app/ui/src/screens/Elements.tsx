// The periodic table of sessions on a page of its own, opened from the
// command palette. It takes the whole window: the layout hides the sidebar
// and puts a link back to the app in the top bar.

import { PeriodicTable } from "../components/PeriodicTable";
import { aboutSessions, useNoticeCount } from "../state/live";

export function Elements() {
  const sessionNotices = useNoticeCount(aboutSessions);
  return (
    <div className="elements-page">
      <PeriodicTable changed={sessionNotices} />
    </div>
  );
}
