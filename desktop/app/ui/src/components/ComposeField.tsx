// The field to write a comment or a note in, drawn as the SDK's
// .pinrail-compose is in plugin views: one line with the text and small
// buttons at its end. Once the text wraps or has a new line, the text takes
// the whole width and the buttons a row under it, with the keys on their
// left. Enter submits, Shift+Enter starts a new line, and Esc stops the
// dictation first, then leaves the field.

import { ArrowUp, Mic } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { canDictate, useDictation } from "../lib/dictation";
import { modalOpen } from "../lib/keys";
import { useToast } from "../state/toasts";
import { Tooltip } from "./Tooltip";

/** The tallest the text grows before it scrolls. */
const MAX = 160;

type SendButton = {
  label: string;
  /** the accessible name, when it says more than the tooltip */
  name?: string;
  keys?: string[];
  disabled?: boolean;
  testId?: string;
};

type Props = {
  value: string;
  onChange: (text: string) => void;
  label: string;
  placeholder: string;
  /** Enter without Shift; without it, Enter starts a new line */
  onSubmit?: () => void;
  /** the keys, shown on the left of the buttons once the field is wide */
  keys?: string;
  /** buttons before the send button */
  tools?: ReactNode;
  /** the round arrow button at the end, which does what Enter does */
  send?: SendButton;
  /** Esc, after the dictation is stopped */
  onEscape?: () => void;
  disabled?: boolean;
  /** focus the text when the field appears */
  autoFocus?: boolean;
  className?: string;
  boxRef?: RefObject<HTMLTextAreaElement | null>;
  /** attributes for the text box, such as a data- attribute to find it by */
  boxProps?: Record<string, string | number>;
};

export function ComposeField({
  value,
  onChange,
  label,
  placeholder,
  onSubmit,
  keys,
  tools,
  send,
  onEscape,
  disabled = false,
  autoFocus = false,
  className = "",
  boxRef,
  boxProps,
}: Props) {
  const toast = useToast();
  const own = useRef<HTMLTextAreaElement>(null);
  const box = boxRef ?? own;
  const dictation = useDictation(onChange, (message) => toast(message, "danger"));
  const listening = dictation.listening;
  const stopDictation = dictation.stop;
  useEffect(() => {
    if (!listening) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.isComposing || modalOpen()) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      stopDictation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [listening, stopDictation]);

  // wide once the text no longer fits on one line, and narrow again only
  // when it is emptied, so the buttons do not jump back and forth
  const [wide, setWide] = useState(false);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    if (!value) {
      if (wide) setWide(false);
    } else if (!wide) {
      el.style.height = "auto";
      const style = getComputedStyle(el);
      const oneLine =
        (parseFloat(style.lineHeight) || 18) + parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
      if (value.includes("\n") || el.scrollHeight > oneLine * 1.4) {
        setWide(true);
        return;
      }
    }
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, MAX)}px`;
    el.style.overflowY = el.scrollHeight > MAX ? "auto" : "hidden";
  }, [value, wide, box]);

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === "Enter" && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey && onSubmit) {
      e.preventDefault();
      if (!send?.disabled) onSubmit();
      return;
    }
    if (e.key === "Escape" && !listening) {
      e.preventDefault();
      e.stopPropagation();
      e.currentTarget.blur();
      onEscape?.();
    }
  };

  return (
    <div className={`compose ${wide ? "is-wide" : ""} ${disabled ? "is-disabled" : ""} ${className}`}>
      <textarea
        ref={box}
        rows={1}
        value={value}
        aria-label={label}
        placeholder={placeholder}
        disabled={disabled}
        autoFocus={autoFocus}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        {...boxProps}
      />
      {keys && !disabled ? <span className="compose-keys">{keys}</span> : null}
      {disabled ? null : (
        <span className="compose-tools">
          {canDictate ? (
            <Tooltip label={listening ? "Stop dictating" : "Dictate"} side="top">
              <button
                type="button"
                className={`compose-icon ${listening ? "is-listening" : ""}`}
                aria-label={listening ? "Stop dictating" : "Dictate"}
                aria-pressed={listening}
                onClick={() => {
                  if (listening) dictation.stop();
                  else dictation.start(value);
                  box.current?.focus();
                }}
              >
                <Mic size={14} />
              </button>
            </Tooltip>
          ) : null}
          {tools}
          {send && onSubmit ? (
            <Tooltip label={send.label} keys={send.keys} side="top">
              <button
                type="button"
                className="compose-icon compose-send"
                aria-label={send.name ?? send.label}
                data-testid={send.testId}
                disabled={send.disabled}
                onClick={onSubmit}
              >
                <ArrowUp size={13} strokeWidth={2.5} />
              </button>
            </Tooltip>
          ) : null}
        </span>
      )}
    </div>
  );
}
