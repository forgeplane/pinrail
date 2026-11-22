// A line of keyboard hints: each a key or combination in one cap, its keys
// joined with +, and what it does, the hints spaced apart.

/** `className` marks a hint, such as one a narrow container hides */
export type KeyHint = { keys: string[]; label: string; className?: string };

export function KeyHints({ hints, className = "" }: { hints: KeyHint[]; className?: string }) {
  return (
    <span className={`key-hints ${className}`}>
      {hints.map(({ keys, label, className: hintClass }) => (
        <span key={label} className={hintClass ? `key-hint ${hintClass}` : "key-hint"}>
          <kbd className="key-cap">{keys.join("+")}</kbd>
          {label}
        </span>
      ))}
    </span>
  );
}
