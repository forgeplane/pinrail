import type { Review } from "../api/types";
/** "3m", "2h", "5d": how long ago, in one unit. */
export function age(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return "";
  const seconds = Math.max(0, Math.floor((now - Date.parse(iso)) / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

/** The full timestamp for titles and history rows. */
export function stamp(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** "2.6 MB", "340 KB", "12 bytes": a file's size, as people read one. */
export function size(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} ${bytes === 1 ? "byte" : "bytes"}`;
}

/** ".glb, .gltf, up to 50 MB each, at most 3 files": the files a plugin accepts beside a payload. */
export function takes(rules: { accept: string[]; max_size?: number; max_count?: number }): string {
  const limits = [
    rules.max_size ? `up to ${size(rules.max_size)} each` : null,
    rules.max_count ? `at most ${rules.max_count} ${rules.max_count === 1 ? "file" : "files"}` : null,
  ].filter(Boolean);
  return `${rules.accept.join(", ")}${limits.length ? `, ${limits.join(", ")}` : ""}`;
}

/** When an ended review ended: decided, withdrawn, discarded or expired. */
export function settledAt(
  review: Pick<Review, "decision" | "withdrawn_at" | "discarded_at" | "expires_at">,
): string | null | undefined {
  return review.decision?.decided_at ?? review.withdrawn_at ?? review.discarded_at ?? review.expires_at;
}
