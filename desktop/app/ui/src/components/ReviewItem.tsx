// The work item a review belongs to, in its header: the item's chip, and a
// menu that puts the review, with all its rounds, under another item or
// under none.

import { useEffect, useState } from "react";
import { ApiError, api } from "../api/client";
import type { Review, WorkItem } from "../api/types";
import { useToast } from "../state/toasts";
import { ItemChip } from "./ItemChip";
import { Select } from "./Select";

export function ReviewItem({ review, onChanged }: { review: Review; onChanged: () => void }) {
  const toast = useToast();
  const [items, setItems] = useState<WorkItem[]>([]);
  useEffect(() => {
    let cancelled = false;
    api
      .items()
      .then((d) => !cancelled && setItems(d.items))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);
  const current = review.item ?? null;
  const choose = async (key: string) => {
    try {
      await api.linkReview(review.id, key === "-" ? null : key);
      onChanged();
    } catch (error) {
      toast(error instanceof ApiError ? error.message : "Pinrail could not be reached", "danger");
    }
  };
  return (
    <span className="review-item">
      {current ? <ItemChip item={current} /> : null}
      <Select
        id="review-item"
        searchable
        label={current ? "Change the work item" : "Add to a work item"}
        value=""
        onChange={(v) => v && void choose(v)}
        options={[
          { value: "", label: current ? "Change…" : "Add to a work item…" },
          ...items.filter((i) => i.key !== current?.key).map((i) => ({ value: i.key, label: `${i.key} ${i.title}` })),
          ...(current ? [{ value: "-", label: "No work item" }] : []),
        ]}
      />
    </span>
  );
}
