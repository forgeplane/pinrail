// A modal dialog over the app: the dimmed backdrop, which closes it when
// pressed, and the box. Esc closes it, Tab and Shift+Tab go round its
// controls without leaving it, and closing it gives the focus back to what
// had it when it opened.

import { useEffect, useRef, type HTMLAttributes, type ReactNode, type RefObject } from "react";
import { useWindowKey } from "../lib/useWindowKey";

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function Dialog({
  labelledBy,
  label,
  className = "",
  plain = false,
  backdropClass = "",
  initialFocus,
  onClose,
  closeOnEscape = true,
  closeOnBackdrop = true,
  onKeyDown,
  children,
  ...boxProps
}: Omit<HTMLAttributes<HTMLDivElement>, "className"> & {
  /** the id of the element that names the dialog, such as its heading */
  labelledBy?: string;
  /** the dialog's name, when no element names it */
  label?: string;
  /** classes for the box, besides app-dialog */
  className?: string;
  /** a box with its own look, without app-dialog's */
  plain?: boolean;
  /** classes for the backdrop, besides app-dialog-backdrop */
  backdropClass?: string;
  /** what takes the focus as it opens; by default its first control */
  initialFocus?: RefObject<HTMLElement | null>;
  onClose: () => void;
  /** whether Esc closes it: false, or a test of the key press, where the
   *  dialog lets a control inside it take Esc first */
  closeOnEscape?: boolean | ((event: KeyboardEvent) => boolean);
  /** whether a press on the backdrop closes it */
  closeOnBackdrop?: boolean;
  children: ReactNode;
}) {
  const box = useRef<HTMLDivElement>(null);

  // the focus goes in as it opens, and back where it was as it closes
  useEffect(() => {
    const opener = document.activeElement;
    const first = initialFocus?.current ?? box.current?.querySelector<HTMLElement>(FOCUSABLE);
    (first ?? box.current)?.focus();
    return () => {
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus({ preventScroll: true });
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps -- as it opens and closes

  useWindowKey(
    (event) => {
      if (event.key !== "Escape") return;
      if (typeof closeOnEscape === "function" && !closeOnEscape(event)) return;
      event.stopPropagation();
      event.preventDefault();
      onClose();
    },
    { capture: true, enabled: closeOnEscape !== false },
  );

  // Tab and Shift+Tab go round the dialog's controls
  const keepTab = (event: React.KeyboardEvent<HTMLDivElement>) => {
    onKeyDown?.(event);
    if (event.defaultPrevented || event.key !== "Tab" || !box.current) return;
    const controls = [...box.current.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent);
    if (!controls.length) {
      event.preventDefault();
      return;
    }
    const first = controls[0];
    const last = controls[controls.length - 1];
    const at = document.activeElement;
    if (event.shiftKey && (at === first || !box.current.contains(at))) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (at === last || !box.current.contains(at))) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <div className={`app-dialog-backdrop ${backdropClass}`} onMouseDown={closeOnBackdrop ? onClose : undefined}>
      <div
        {...boxProps}
        ref={box}
        className={`${plain ? "" : "app-dialog"} ${className}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        aria-label={label}
        tabIndex={-1}
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={keepTab}
      >
        {children}
      </div>
    </div>
  );
}
