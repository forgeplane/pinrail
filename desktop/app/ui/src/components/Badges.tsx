import { ExternalLink } from "lucide-react";
import type { Origin, Status, Summary } from "../api/types";
import { useLive } from "../state/live";
import { PluginIcon } from "./PluginIcon";

export function StatusBadge({ status }: { status: Status }) {
  return <span className={`status-badge status-${status}`}>{status}</span>;
}

export function PluginBadge({ name, version, icon }: { name: string; version?: number; icon?: string | null }) {
  const live = useLive();
  return (
    <span className="plugin-badge">
      <PluginIcon icon={icon === undefined ? live.pluginIcon(name) : icon} size={12} strokeWidth={2} />
      {name}
      {version ? <span className="faint">v{version}</span> : null}
    </span>
  );
}

export function OriginLine({ origin, link = true }: { origin: Origin; link?: boolean }) {
  return (
    <span className="origin-line">
      {origin.repo ? <span>{origin.repo}</span> : null}
      {origin.workflow ? <span className="faint">/</span> : null}
      {origin.workflow ? <span>{origin.workflow}</span> : null}
      {origin.ref ? <span className="mono faint">#{origin.ref}</span> : null}
      {link && origin.url ? (
        <a href={origin.url} target="_blank" rel="noreferrer" className="accent with-icon">
          open <ExternalLink size={12} />
        </a>
      ) : null}
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
