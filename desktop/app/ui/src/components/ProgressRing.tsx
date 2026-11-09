// How much of a parent's work is closed, as a ring that fills, the way the
// sub-items' count reads at a glance.

export function ProgressRing({ done, total, size = 12 }: { done: number; total: number; size?: number }) {
  const r = size / 2 - 1.5;
  const length = 2 * Math.PI * r;
  const filled = total ? (done / total) * length : 0;
  return (
    <svg className="progress-ring" width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden="true">
      <circle cx={size / 2} cy={size / 2} r={r} className="progress-ring-track" />
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        className="progress-ring-fill"
        strokeDasharray={`${filled} ${length}`}
        transform={`rotate(-90 ${size / 2} ${size / 2})`}
      />
    </svg>
  );
}
