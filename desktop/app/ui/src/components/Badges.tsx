import { Paperclip } from "lucide-react";
import type { Review, Summary } from "../api/types";
import { size } from "../lib/format";
import { usePlugins } from "../state/live";
import { PluginIcon } from "./PluginIcon";
import { Tooltip } from "./Tooltip";

/**
 * How a review ended: the verdict its plugin's outcome summary names, in
 * that verdict's tone, or else the status. Pinrail reads no field of a
 * decision itself.
 */
export function outcomeOf(review: Pick<Review, "status" | "decision">): { label: string; tone: string } {
  const verdict = review.status === "decided" ? review.decision?.summary?.verdict : undefined;
  if (verdict) return { label: verdict.label, tone: verdict.tone };
  return { label: review.status, tone: review.status };
}

export function OutcomeBadge({ review }: { review: Pick<Review, "status" | "decision"> }) {
  const { label, tone } = outcomeOf(review);
  return <span className={`status-badge status-${tone}`}>{label}</span>;
}

export function PluginBadge({ name, version, icon }: { name: string; version?: number; icon?: string | null }) {
  const { pluginIcon } = usePlugins();
  return (
    <span className="plugin-badge">
      <PluginIcon icon={icon === undefined ? pluginIcon(name) : icon} size={12} strokeWidth={2} />
      {name}
      {version ? <span className="faint">v{version}</span> : null}
    </span>
  );
}

/** A summary's counts as chips, each in its tone; nothing for none. */
export function SummaryCounts({ summary }: { summary: Summary | null | undefined }) {
  const counts = summary?.counts ?? [];
  if (counts.length === 0) return null;
  return (
    <span className="summary-counts">
      {counts.map(({ label, count, tone }) => (
        <span key={label} className={`summary-count tone-${tone}`}>
          {count} {label}
        </span>
      ))}
    </span>
  );
}

/** A paperclip and how many files a review came with, the size on hover;
 *  nothing for a review without any. */
export function FilesCount({ total }: { total?: { count: number; bytes: number } }) {
  if (!total || total.count === 0) return null;
  const label = `${total.count} file${total.count === 1 ? "" : "s"} · ${size(total.bytes)}`;
  return (
    <Tooltip label={label} side="top">
      <span className="files-count" aria-label={label} data-files-count={total.count}>
        <Paperclip size={11} />
        {total.count}
      </span>
    </Tooltip>
  );
}
