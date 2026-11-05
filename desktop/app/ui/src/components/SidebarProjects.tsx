// The sidebar while sessions are off in Settings › Labs: the oldest
// reviews that wait, then each project with how many wait in it, which
// filters the inbox.

import { FolderGit2 } from "lucide-react";
import { useMemo } from "react";
import { Link, NavLink } from "react-router";
import { NO_PROJECT, reviewPath } from "../lib/url";
import { useLive } from "../state/live";
import { PluginIcon } from "./PluginIcon";
import { Tooltip } from "./Tooltip";

/** How many waiting reviews the sidebar lists before pointing to the inbox. */
const WAITING_SHOWN = 5;

export function SidebarProjects({ shown, openIn }: { shown: string | null; openIn: "panel" | "page" }) {
  const live = useLive();
  // oldest first, the order the global shortcut uses
  const waiting = useMemo(
    () => [...live.pending].sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id)),
    [live.pending],
  );
  // how many wait per project, beside the inbox's quick filters
  const perProject = useMemo(() => {
    const counts = new Map<string, number>();
    for (const r of live.pending) if (r.origin.repo) counts.set(r.origin.repo, (counts.get(r.origin.repo) ?? 0) + 1);
    return counts;
  }, [live.pending]);

  return (
    <>
      {waiting.length > 0 ? (
        <section className="sidebar-waiting" aria-label="Waiting">
          <h2>Waiting</h2>
          {waiting.slice(0, WAITING_SHOWN).map((r) => (
            <Tooltip
              key={r.id}
              label={[r.origin.repo, r.origin.workflow].filter(Boolean).join(" · ") || r.plugin}
              side="top"
            >
              <Link
                to={reviewPath(r.id, openIn)}
                className={`sidebar-review ${shown === r.id ? "active" : ""}`}
                aria-current={shown === r.id ? "page" : undefined}
                data-testid="waiting-review"
              >
                <PluginIcon icon={live.pluginIcon(r.plugin)} size={14} strokeWidth={1.75} />
                <span className="sidebar-review-title">{r.title}</span>
              </Link>
            </Tooltip>
          ))}
          {waiting.length > WAITING_SHOWN ? (
            <NavLink to="/" end className="sidebar-review sidebar-more">
              <span className="sidebar-review-title">{waiting.length - WAITING_SHOWN} more in the inbox</span>
            </NavLink>
          ) : null}
        </section>
      ) : null}
      {live.projects.length > 0 || live.unassigned > 0 ? (
        <section className="sidebar-repositories" aria-label="Repositories">
          <h2>Repositories</h2>
          {live.projects.map((project) => (
            <NavLink key={project} to={`/?repo=${encodeURIComponent(project)}`} className="sidebar-repo">
              <FolderGit2 size={14} strokeWidth={1.75} />
              <span className="sidebar-review-title">{project}</span>
              <span className="nav-count">{perProject.get(project) ?? 0}</span>
            </NavLink>
          ))}
          {live.unassigned > 0 ? (
            <NavLink to={`/?repo=${NO_PROJECT}`} className="sidebar-repo">
              <FolderGit2 size={14} strokeWidth={1.75} className="is-faint" />
              <span className="sidebar-review-title">No repository</span>
              <span className="nav-count">{live.unassigned}</span>
            </NavLink>
          ) : null}
        </section>
      ) : null}
    </>
  );
}
