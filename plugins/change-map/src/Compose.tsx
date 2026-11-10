// The field to write a comment in: the SDK's comment field, one box with the
// text, the microphone and the button that adds the comment. The SDK sizes
// it: it grows with the text, and once the text wraps or has a new line, the
// text takes the whole width and the buttons a row of their own under it,
// with the keys on their left. Enter adds the comment, Shift+Enter starts a
// new line, and Esc cancels when the field can be cancelled. Focused when it
// opens, the cursor is at the end of the text.
import { useCallback, useEffect, useLayoutEffect, useRef, type Ref } from "react";

/** The open fields' submit functions, each as of its last render. */
const open = new Set<{ current: () => void }>();

/** Adds the text of every open field, as Enter would, so that a comment
 *  typed and not yet added goes with the decision. */
export function submitOpenFields() {
  for (const submit of [...open]) submit.current();
}

export function Compose({
  id,
  value,
  onChange,
  onSubmit,
  onCancel,
  placeholder,
  autoFocus,
  inputRef,
  onLeave,
}: {
  id: string;
  value: string;
  onChange(value: string): void;
  onSubmit(): void;
  /** shows a cancel button beside the add button */
  onCancel?: () => void;
  placeholder: string;
  autoFocus?: boolean;
  inputRef?: Ref<HTMLTextAreaElement>;
  /** focus left the field and its buttons */
  onLeave?: () => void;
}) {
  const field = useRef<HTMLTextAreaElement | null>(null);
  const submit = useRef(onSubmit);
  useLayoutEffect(() => {
    submit.current = onSubmit;
  });
  useEffect(() => {
    open.add(submit);
    return () => {
      open.delete(submit);
    };
  }, []);
  const setField = useCallback(
    (el: HTMLTextAreaElement | null) => {
      field.current = el;
      if (typeof inputRef === "function") inputRef(el);
      else if (inputRef) inputRef.current = el;
    },
    [inputRef],
  );
  // a click on the buttons leaves the focus in the text, so that it does not
  // count as leaving the field: WebKit focuses nothing on a button's click
  const keepFocus = (e: React.MouseEvent) => e.preventDefault();
  // focused when it opens, with the cursor after the text
  useLayoutEffect(() => {
    const el = field.current;
    if (!autoFocus || !el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
    // when the field opens: autoFocus does not change while it is open
  }, [autoFocus]);
  // the SDK sizes the field to its text
  useLayoutEffect(() => {
    if (field.current) window.Pinrail.fitField(field.current);
  }, [value]);
  return (
    <div
      className="pinrail-compose"
      onBlur={(e) => {
        if (onLeave && !e.currentTarget.contains(e.relatedTarget as Node | null)) onLeave();
      }}
    >
      <textarea
        id={id}
        ref={setField}
        className="pinrail-compose-input"
        rows={1}
        placeholder={placeholder}
        aria-label={placeholder}
        value={value}
        data-pinrail-dictate
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
            e.preventDefault();
            onSubmit();
          } else if (e.key === "Escape" && onCancel && !e.defaultPrevented) {
            // the field's own Esc: a drawer around it stays open
            e.preventDefault();
            onCancel();
          }
        }}
      />
      <span className="pinrail-compose-keys">Enter to add · Shift+Enter for a new line</span>
      <div className="pinrail-compose-tools">
        <span data-pinrail-dictate-for={id} />
        {onCancel ? (
          <button
            type="button"
            className="pinrail-compose-icon pinrail-compose-cancel"
            onMouseDown={keepFocus}
            onClick={onCancel}
            aria-label="Cancel"
            title="Cancel (Esc)"
          />
        ) : null}
        <button
          type="button"
          className="pinrail-compose-icon pinrail-compose-send"
          onMouseDown={keepFocus}
          onClick={onSubmit}
          disabled={!value.trim()}
          aria-label="Add comment"
          title="Add comment (Enter). Shift+Enter starts a new line."
        />
      </div>
    </div>
  );
}
