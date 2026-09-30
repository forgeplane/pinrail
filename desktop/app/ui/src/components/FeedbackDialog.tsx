// Sending feedback to the Pinrail team: a reply address, a subject, the
// message, files the person attaches, drops or pastes, and the app's
// diagnostics unless the person leaves them out. The app adds its version
// and system when it sends. ⌘Enter sends, Esc leaves.

import { Paperclip, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, inTauri, sendFeedback } from "../api/client";
import { describeDiagnostics, type CliState } from "../lib/diagnostics";
import { size } from "../lib/format";
import { MOD } from "../lib/keys";

/** What the feedback service accepts. */
export const MAX_FILES = 5;
export const MAX_FILES_BYTES = 10 * 1024 * 1024;

const EMAIL_KEY = "pinrail.feedback.email";

const remembered = () => {
  try {
    return localStorage.getItem(EMAIL_KEY) ?? "";
  } catch {
    return "";
  }
};

const remember = (email: string) => {
  try {
    localStorage.setItem(EMAIL_KEY, email);
  } catch {
    // the address is only a convenience for next time
  }
};

/** The diagnostics text, from what the core and the shell say; a part that cannot be read is left out. */
async function gatherDiagnostics(): Promise<string> {
  const quietly = <T,>(promise: Promise<T>) => promise.catch(() => null);
  const [info, plugins, settings, cli] = await Promise.all([
    quietly(api.info()),
    quietly(api.plugins().then((answer) => answer.plugins)),
    quietly(api.settings()),
    inTauri()
      ? quietly(import("@tauri-apps/api/core").then(({ invoke }) => invoke<CliState>("cli_status")))
      : Promise.resolve(null),
  ]);
  return describeDiagnostics({ info, plugins, settings, cli });
}

/** A pasted screenshot has a generic name; give it one that says what it is. */
const named = (file: File, index: number) =>
  file.name && file.name !== "image.png" ? file : new File([file], `pasted-${index + 1}.png`, { type: file.type });

export function FeedbackDialog({ onClose, onSent }: { onClose: () => void; onSent: () => void }) {
  const [email, setEmail] = useState(remembered);
  const [subject, setSubject] = useState("");
  const [message, setMessage] = useState("");
  const [files, setFiles] = useState<File[]>([]);
  const [dropping, setDropping] = useState(false);
  const [diagnostics, setDiagnostics] = useState<string | null>(null);
  const [includeDiagnostics, setIncludeDiagnostics] = useState(true);
  const [showDiagnostics, setShowDiagnostics] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const first = useRef<HTMLInputElement>(null);
  const second = useRef<HTMLInputElement>(null);
  const picker = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let current = true;
    gatherDiagnostics().then((text) => current && setDiagnostics(text));
    return () => {
      current = false;
    };
  }, []);

  // a remembered address leaves the subject to fill first
  useEffect(() => {
    (first.current?.value ? second : first).current?.focus();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const attach = (added: File[]) => {
    if (!added.length) return;
    const next = [...files, ...added.map((file, i) => named(file, files.length + i))];
    if (next.length > MAX_FILES) {
      setError(`You can attach up to ${MAX_FILES} files.`);
    } else if (next.reduce((total, file) => total + file.size, 0) > MAX_FILES_BYTES) {
      setError(`The attached files can be up to ${MAX_FILES_BYTES / 1024 / 1024} MB in total.`);
    } else {
      setError(null);
      setFiles(next);
    }
  };

  const ready = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim()) && subject.trim() !== "" && message.trim() !== "";

  const send = async () => {
    if (busy || !ready) return;
    setBusy(true);
    setError(null);
    const form = new FormData();
    form.append("email", email.trim());
    form.append("subject", subject.trim());
    form.append("message", message.trim());
    if (includeDiagnostics && diagnostics) form.append("diagnostics", diagnostics);
    for (const file of files) form.append("file", file, file.name);
    try {
      await sendFeedback(form);
      remember(email.trim());
      onSent();
    } catch (e) {
      setError(e instanceof Error ? e.message : "The feedback was not sent.");
      setBusy(false);
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      send();
    }
  };

  return (
    <div className="app-dialog-backdrop" onMouseDown={onClose}>
      <div
        className={`app-dialog feedback-dialog${dropping ? " is-dropping" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="feedback-title"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        onPaste={(e) => {
          const pasted = Array.from(e.clipboardData.files);
          if (pasted.length) {
            e.preventDefault();
            attach(pasted);
          }
        }}
        onDragOver={(e) => {
          if (e.dataTransfer.types.includes("Files")) {
            e.preventDefault();
            setDropping(true);
          }
        }}
        onDragLeave={(e) => {
          if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDropping(false);
        }}
        onDrop={(e) => {
          e.preventDefault();
          setDropping(false);
          attach(Array.from(e.dataTransfer.files));
        }}
      >
        <div className="dialog-head">
          <h2 id="feedback-title">Send feedback</h2>
        </div>
        <p className="dim">
          Report a problem or suggest an idea. The Pinrail team reads every message and replies by email.
        </p>
        <label className="feedback-field">
          <span>Email</span>
          <input
            ref={first}
            type="email"
            className="feedback-input"
            placeholder="you@example.com"
            autoComplete="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </label>
        <label className="feedback-field">
          <span>Subject</span>
          <input
            ref={second}
            type="text"
            className="feedback-input"
            placeholder="A short summary"
            maxLength={200}
            value={subject}
            onChange={(e) => setSubject(e.target.value)}
          />
        </label>
        <label className="feedback-field">
          <span>Message</span>
          <textarea
            className="feedback-input feedback-message"
            rows={6}
            placeholder="What happened, or what would you like Pinrail to do?"
            maxLength={10_000}
            value={message}
            onChange={(e) => setMessage(e.target.value)}
          />
        </label>
        <div className="feedback-attachments">
          {files.length ? (
            <ul className="feedback-files" aria-label="Attachments">
              {files.map((file, i) => (
                <li key={`${file.name}-${i}`} className="feedback-file">
                  <span className="feedback-file-name" title={file.name}>
                    {file.name}
                  </span>
                  <span className="dim">{size(file.size)}</span>
                  <button
                    type="button"
                    className="feedback-file-remove"
                    aria-label={`Remove ${file.name}`}
                    onClick={() => setFiles((current) => current.filter((_, j) => j !== i))}
                    disabled={busy}
                  >
                    <X size={12} />
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
          <div className="feedback-attach-row">
            <button
              type="button"
              className="chrome-button"
              onClick={() => picker.current?.click()}
              disabled={busy || files.length >= MAX_FILES}
            >
              <Paperclip size={13} /> Attach files
            </button>
            <span className="dim">or drop or paste them here. Up to {MAX_FILES} files, 10 MB in total.</span>
          </div>
          <input
            ref={picker}
            type="file"
            multiple
            hidden
            aria-label="Attach files"
            onChange={(e) => {
              attach(Array.from(e.target.files ?? []));
              e.target.value = "";
            }}
          />
        </div>
        <div className="feedback-diagnostics">
          <div className="feedback-diagnostics-row">
            <label className="feedback-check">
              <input
                type="checkbox"
                checked={includeDiagnostics}
                onChange={(e) => setIncludeDiagnostics(e.target.checked)}
                disabled={busy}
              />
              Include diagnostics
            </label>
            <button
              type="button"
              className="feedback-show"
              aria-expanded={showDiagnostics}
              onClick={() => setShowDiagnostics((shown) => !shown)}
            >
              {showDiagnostics ? "Hide" : "Show"}
            </button>
          </div>
          <span className="dim">
            How Pinrail is installed, its plugins and its settings. Nothing from your reviews is included.
          </span>
          {showDiagnostics ? (
            <pre className="feedback-diagnostics-text" aria-label="Diagnostics">
              {diagnostics ?? "Gathering…"}
            </pre>
          ) : null}
        </div>
        {error ? <p className="notice notice-danger">{error}</p> : null}
        <div className="dialog-actions feedback-actions">
          <span className="dim feedback-note">Pinrail adds its version and your operating system.</span>
          <button type="button" className="chrome-button" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button
            type="button"
            className="chrome-button button-primary"
            onClick={send}
            disabled={busy || !ready}
            title={MOD === "⌘" ? "⌘Enter" : "Ctrl+Enter"}
          >
            {busy ? "Sending…" : "Send"}
          </button>
        </div>
      </div>
    </div>
  );
}
