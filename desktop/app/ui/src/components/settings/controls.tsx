// The controls a settings row ends with: a switch, a segmented choice and
// a shortcut recorder.

import { useEffect, useState, type ReactNode } from "react";
import { shortcutFromEvent, shortcutGlyphs } from "../../lib/shortcuts";

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

/**
 * Shows a shortcut as keys; a click listens for the next combination.
 * Esc leaves it as it was. The value is the string the app registers.
 */
export function ShortcutRecorder({ value, onChange, label }: { value: string; onChange: (shortcut: string) => void; label: string }) {
  const [recording, setRecording] = useState(false);

  useEffect(() => {
    if (!recording) return;
    const onKey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        setRecording(false);
        return;
      }
      const next = shortcutFromEvent(event);
      if (!next) return;
      setRecording(false);
      if (next !== value) onChange(next);
    };
    const stop = () => setRecording(false);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", stop);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", stop);
    };
  }, [recording, value, onChange]);

  return (
    <button type="button" className={`shortcut-recorder ${recording ? "is-recording" : ""}`} aria-label={label} onClick={() => setRecording(true)} onBlur={() => setRecording(false)}>
      {recording ? (
        <span className="shortcut-recorder-hint">Press keys…</span>
      ) : (
        <span className="settings-combo">
          {shortcutGlyphs(value).map((k, i) => (
            <kbd key={i}>{k}</kbd>
          ))}
        </span>
      )}
    </button>
  );
}
