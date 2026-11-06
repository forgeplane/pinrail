// A line of keyboard hints: each a key or combination in one cap, its keys
// joined with +, and what it does, the hints spaced apart.

export type KeyHint = { keys: string[]; label: string };

export function KeyHints({ hints, className = "" }: { hints: KeyHint[]; className?: string }) {
  return (
    <span className={`key-hints ${className}`}>
      {hints.map(({ keys, label }) => (
        <span key={label} className="key-hint">
          <kbd className="key-cap">{keys.join("+")}</kbd>
          {label}
        </span>
      ))}
    </span>
  );
}
