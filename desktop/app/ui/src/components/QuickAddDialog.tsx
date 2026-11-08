// Adding a work item from any page: the Work items page's quick add, in a
// dialog that ⌘⇧C opens. The item is added where the person is, and a
// toast names it. The repository is the one the page shows, such as a
// session's or a review's, else the one the page is narrowed to, else the
// last one used here, else the first.

import { useEffect, useState } from "react";
import { useSearchParams } from "react-router";
import { api } from "../api/client";
import type { WorkItem } from "../api/types";
import { pageRepo } from "../lib/pageRepo";
import { useToast } from "../state/toasts";
import { QuickAddBox } from "./QuickAddBox";

const LAST_REPO = "pinrail.quickAdd.repo";

export function QuickAddDialog({ onClose }: { onClose: () => void }) {
  const toast = useToast();
  const [params] = useSearchParams();
  const [repos, setRepos] = useState<string[] | null>(null);
  // as the dialog opens: the page under it does not change while it shows
  const [shown] = useState(pageRepo);
  const [items, setItems] = useState<WorkItem[]>([]);
  useEffect(() => {
    let cancelled = false;
    Promise.all([api.projects(), api.items()])
      .then(([projects, listed]) => {
        if (cancelled) return;
        setItems(listed.items);
        setRepos([
          ...new Set([
            ...projects.projects.flatMap((p) => p.repos.map((r) => r.repo)),
            ...projects.loose.map((r) => r.repo),
            ...listed.items.map((i) => i.repo),
            ...(shown ? [shown] : []),
          ]),
        ]);
      })
      .catch(() => !cancelled && setRepos([]));
    return () => {
      cancelled = true;
    };
  }, [shown]);

  const last = (() => {
    try {
      return localStorage.getItem(LAST_REPO);
    } catch {
      return null;
    }
  })();
  const here = params.get("repo");
  const defaultRepo = [shown, here, last].find((r) => r && repos?.includes(r)) ?? null;

  return (
    <div className="app-dialog-backdrop quick-add-backdrop" onMouseDown={onClose}>
      <div
        className="app-dialog quick-add-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Add a work item"
        onMouseDown={(e) => e.stopPropagation()}
      >
        {repos ? (
          <QuickAddBox
            onClose={onClose}
            repos={repos}
            items={items}
            defaultRepo={defaultRepo}
            defaultParent={null}
            enterHint="Add"
            onAdded={(item, more) => {
              try {
                localStorage.setItem(LAST_REPO, item.repo);
              } catch {
                // no storage
              }
              toast(`${item.key} added`);
              if (!more) onClose();
            }}
          />
        ) : null}
      </div>
    </div>
  );
}
