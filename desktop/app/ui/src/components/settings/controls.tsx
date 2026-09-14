// The controls a settings row ends with: a switch and a segmented choice.

import type { ReactNode } from "react";

export function Toggle({ checked, onChange, disabled, label }: { checked: boolean; onChange: (checked: boolean) => void; disabled?: boolean; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className={`toggle ${checked ? "is-on" : ""}`}
      onClick={() => onChange(!checked)}
      onKeyDown={(e) => {
        if (e.key === "ArrowLeft" && checked) onChange(false);
        if (e.key === "ArrowRight" && !checked) onChange(true);
      }}
    >
      <span className="toggle-knob" />
    </button>
  );
}

export type SegmentedOption<T extends string> = { value: T; label: ReactNode; title?: string };

export function Segmented<T extends string>({ value, options, onChange, label, disabled }: { value: T; options: SegmentedOption<T>[]; onChange: (value: T) => void; label: string; disabled?: boolean }) {
  return (
    <span className="segmented" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          title={o.title}
          disabled={disabled}
          className={o.value === value ? "is-on" : ""}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </span>
  );
}
