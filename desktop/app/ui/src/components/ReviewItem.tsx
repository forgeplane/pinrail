// The work items a review belongs to, in its header: a chip for each, with
// an × that takes the review, with all its rounds, out of that item, and a
// menu that adds it to another.

import { X } from "lucide-react";
import { api, errorMessage } from "../api/client";
import type { Review } from "../api/types";
import { useToast } from "../state/toasts";
import { ItemChip } from "./ItemChip";
import { Select } from "./Select";
import { useLoad } from "../lib/useLoad";

export function ReviewItem({ review, onChanged }: { review: Review; onChanged: () => void }) {
  const toast = useToast();
  const items = useLoad(() => api.items().then((d) => d.items), []).data ?? [];
  const current = review.items ?? [];
  const keys = current.map((i) => i.key);
  const save = async (next: string[]) => {
    try {
      await api.setReviewItems(review.id, next);
      onChanged();
    } catch (error) {
      toast(errorMessage(error), "danger");
    }
  };
  return (
    <span className="review-item">
      {current.map((item) => (
        <span key={item.id} className="review-item-chip">
          <ItemChip item={item} />
          <button
            type="button"
            className="review-item-remove"
            aria-label={`Remove from ${item.key}`}
            title={`Remove from ${item.key}`}
            onClick={() => void save(keys.filter((k) => k !== item.key))}
          >
            <X size={11} aria-hidden="true" />
          </button>
        </span>
      ))}
      <Select
        id="review-item"
        searchable
        label="Add to a work item"
        value=""
        onChange={(v) => v && void save([...keys, v])}
        options={[
          { value: "", label: "Add to a work item…" },
          ...items.filter((i) => !keys.includes(i.key)).map((i) => ({ value: i.key, label: `${i.key} ${i.title}` })),
        ]}
      />
    </span>
  );
}
