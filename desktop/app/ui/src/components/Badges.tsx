import { Paperclip } from "lucide-react";
import type { Review, Summary } from "../api/types";
import { size } from "../lib/format";
import { usePlugins } from "../state/live";
import { PluginIcon } from "./PluginIcon";
import { Tooltip } from "./Tooltip";

/**
 * A decision that says "approve" or "revise" in a top-level `verdict` is
 * shown as such; a plugin whose decision has no verdict stays "decided".
 */
export function outcomeOf(review: Pick<Review, "status" | "decision">): { label: string; tone: string } {
  const verdict =
    review.status === "decided" ? (review.decision?.data as { verdict?: unknown } | null)?.verdict : undefined;
  if (verdict === "approve") return { label: "approved", tone: "approved" };
  if (verdict === "revise") return { label: "changes requested", tone: "revise" };
  if (typeof verdict === "string" && verdict.length <= 24) return { label: verdict, tone: "decided" };
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

const severity = (label: string) =>
  ["blocker", "major", "minor", "nit"].includes(label) ? `sev-${label}` : "sev-other";

export function SummaryCounts({ summary }: { summary: Summary | null }) {
  const counts = Array.isArray(summary?.counts) ? summary!.counts : [];
  const subtitle = summary?.subtitle;
  if (counts.length === 0 && !subtitle) return null;
  return (
    <span className="summary-counts">
      {counts.map(([label, n]) => (
        <span key={label} className={`summary-count ${severity(String(label))}`}>
          {n} {label}
        </span>
      ))}
      {subtitle ? <span className="faint">{subtitle}</span> : null}
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
