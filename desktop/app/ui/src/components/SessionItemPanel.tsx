// A work item beside a session's thread, in place of its details: the item
// panel of the Work items page, with every item for its pickers.

import { useCallback, useEffect, useState } from "react";
import { api } from "../api/client";
import type { WorkItem } from "../api/types";
import { useLive } from "../state/live";
import { ItemPanel } from "./ItemPanel";

export function SessionItemPanel({
  itemKey,
  onOpen,
  onClose,
}: {
  itemKey: string;
  onOpen: (key: string) => void;
  onClose: () => void;
}) {
  const live = useLive();
  const [items, setItems] = useState<WorkItem[]>([]);
  const load = useCallback(() => {
    api
      .items({ status: "open,in_progress,done,cancelled" })
      .then((d) => setItems(d.items))
      .catch(() => {});
  }, []);
  useEffect(load, [load, live.lastNotice]);
  return (
    <ItemPanel
      itemKey={itemKey}
      items={items}
      repos={[...new Set(items.map((i) => i.repo))]}
      onOpen={onOpen}
      onClose={onClose}
      onChanged={load}
    />
  );
}
