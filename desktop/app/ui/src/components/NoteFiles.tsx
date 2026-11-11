// The files a note carries: where each is read from, saving one, and the
// chips that list them at the bottom of the note.

import { Download, FileText } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { inTauri, serverUrl } from "../api/client";
import type { NoteAttachment, SessionStatus } from "../api/types";
import { size } from "../lib/format";
import { useToast } from "../state/toasts";

/** The note a file belongs to: its session, and its status's id. */
export type NoteOf = { agent: string; session: string; status: number };

/** Where a note's file is read from, once the server's address is known. */
export function useNoteFileUrl(note: NoteOf | null): (name: string) => string | null {
  const [base, setBase] = useState<string | null>(null);
  useEffect(() => {
    void serverUrl().then(setBase);
  }, []);
  return useCallback(
    (name: string) => {
      if (!base || !note) return null;
      const seg = encodeURIComponent;
      return `${base}/api/v1/sessions/${seg(note.agent)}/${seg(note.session)}/statuses/${note.status}/attachments/${seg(name)}`;
    },
    [base, note?.agent, note?.session, note?.status], // eslint-disable-line react-hooks/exhaustive-deps -- the note's names, not the object
  );
}

/** Where a file a review or a question set carries is read from. */
export function useReviewFileUrl(review: string): (name: string) => string | null {
  const [base, setBase] = useState<string | null>(null);
  useEffect(() => {
    void serverUrl().then(setBase);
  }, []);
  return useCallback(
    (name: string) =>
      base ? `${base}/api/v1/reviews/${encodeURIComponent(review)}/attachments/${encodeURIComponent(name)}` : null,
    [base, review],
  );
}

/** Saves a file a review or a question set carries where the person
 *  chooses, in the app; outside it, downloads it. */
export function useSaveReviewFile(review: string, url: (name: string) => string | null) {
  const toast = useToast();
  return useCallback(
    async (name: string) => {
      if (inTauri()) {
        try {
          const { invoke } = await import("@tauri-apps/api/core");
          const saved = await invoke<string | null>("save_attachment", { review, name });
          if (saved) toast(`Saved ${name} to ${saved}`);
        } catch (error) {
          toast(String(error), "danger");
        }
        return;
      }
      const href = url(name);
      if (!href) return;
      const link = document.createElement("a");
      link.href = href;
      link.download = name;
      link.rel = "noreferrer";
      document.body.append(link);
      link.click();
      link.remove();
    },
    [review, url, toast],
  );
}

/** Saves a note's file where the person chooses, in the app; outside it,
 *  downloads it. */
export function useSaveNoteFile(note: NoteOf | null, url: (name: string) => string | null) {
  const toast = useToast();
  return useCallback(
    async (name: string) => {
      if (!note) return;
      if (inTauri()) {
        try {
          const { invoke } = await import("@tauri-apps/api/core");
          const saved = await invoke<string | null>("save_note_attachment", { ...note, name });
          if (saved) toast(`Saved ${name} to ${saved}`);
        } catch (error) {
          toast(String(error), "danger");
        }
        return;
      }
      const href = url(name);
      if (!href) return;
      const link = document.createElement("a");
      link.href = href;
      link.download = name;
      link.rel = "noreferrer";
      document.body.append(link);
      link.click();
      link.remove();
    },
    [note?.agent, note?.session, note?.status, url, toast], // eslint-disable-line react-hooks/exhaustive-deps -- the note's names, not the object
  );
}

/** The note's files as chips: a thumbnail or an icon, the name and the
 *  size, and a button that saves the file. */
export function NoteFiles({
  files,
  url,
  onSave,
}: {
  files: NoteAttachment[];
  url: (name: string) => string | null;
  onSave: (name: string) => void;
}) {
  if (!files.length) return null;
  return (
    <ul className="note-files" aria-label="Files">
      {files.map((file) => {
        const src = file.media_type.startsWith("image/") ? url(file.name) : null;
        return (
          <li key={file.name} className="note-file" data-testid="note-file">
            {src ? (
              <img className="note-file-thumb" src={src} alt="" />
            ) : (
              <FileText size={14} className="note-file-icon" aria-hidden />
            )}
            <span className="note-file-name truncate">{file.name}</span>
            <span className="faint">{size(file.size)}</span>
            <button
              type="button"
              className="note-file-save"
              aria-label={`Save ${file.name}`}
              title={`Save ${file.name}`}
              onClick={() => onSave(file.name)}
            >
              <Download size={13} />
            </button>
          </li>
        );
      })}
    </ul>
  );
}

/** The files of a status, which only a note has. */
export const filesOf = (status: SessionStatus): NoteAttachment[] => status.attachments ?? [];
