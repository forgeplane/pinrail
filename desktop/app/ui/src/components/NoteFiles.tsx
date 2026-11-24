// The files a note carries: where each is read from, saving one, the
// chips that list them at the bottom of the note, and the pop-up that
// shows an image at its full size.

import { ChevronLeft, ChevronRight, Download, FileText, FolderOpen, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { inTauri, serverUrl } from "../api/client";
import type { NoteAttachment, SessionStatus } from "../api/types";
import { size } from "../lib/format";
import { useToast } from "../state/toasts";
import { Dialog } from "./Dialog";

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

/** What a file's button does: save a copy, or show the file, which is on
 *  this computer already, in Finder. */
type FileAction = "save" | "reveal";

const actionLabel = (action: FileAction, name: string) =>
  action === "save" ? `Save ${name}` : `Show ${name} in Finder`;

const ActionIcon = ({ action, size }: { action: FileAction; size: number }) =>
  action === "save" ? <Download size={size} /> : <FolderOpen size={size} />;

/** The note's files as chips: a thumbnail or an icon, the name and the
 *  size when it is known, and a button that saves the file or shows it in
 *  Finder. An image's thumbnail and name open it in a pop-up. */
export function NoteFiles({
  files,
  url,
  onSave,
  action = "save",
}: {
  files: (Omit<NoteAttachment, "size"> & { size?: number })[];
  url: (name: string) => string | null;
  onSave: (name: string) => void;
  action?: FileAction;
}) {
  // the name of the image the pop-up shows
  const [shown, setShown] = useState<string | null>(null);
  if (!files.length) return null;
  const images = files.filter((file) => file.media_type.startsWith("image/") && url(file.name));
  return (
    <ul className="note-files" aria-label="Files">
      {files.map((file) => {
        const src = file.media_type.startsWith("image/") ? url(file.name) : null;
        return (
          <li key={file.name} className="note-file" data-testid="note-file">
            {src ? (
              <button
                type="button"
                className="note-file-open"
                aria-label={`Open ${file.name}`}
                title={`Open ${file.name}`}
                onClick={() => setShown(file.name)}
              >
                <img className="note-file-thumb" src={src} alt="" />
                <span className="note-file-name truncate">{file.name}</span>
              </button>
            ) : (
              <>
                <FileText size={14} className="note-file-icon" aria-hidden />
                <span className="note-file-name truncate">{file.name}</span>
              </>
            )}
            {file.size !== undefined ? <span className="faint">{size(file.size)}</span> : null}
            <button
              type="button"
              className="note-file-save"
              aria-label={actionLabel(action, file.name)}
              title={actionLabel(action, file.name)}
              onClick={() => onSave(file.name)}
            >
              <ActionIcon action={action} size={13} />
            </button>
          </li>
        );
      })}
      {shown ? (
        <ImagePopup
          images={images.map((file) => file.name)}
          shown={shown}
          url={url}
          onShow={setShown}
          onSave={onSave}
          action={action}
          onClose={() => setShown(null)}
        />
      ) : null}
    </ul>
  );
}

/** An image of the note's at its full size, fitted to the window, with its
 *  name, Save and Close. With several, the arrow keys and the buttons
 *  beside the name go to the previous and the next. */
function ImagePopup({
  images,
  shown,
  url,
  onShow,
  onSave,
  action,
  onClose,
}: {
  images: string[];
  shown: string;
  url: (name: string) => string | null;
  onShow: (name: string) => void;
  onSave: (name: string) => void;
  action: FileAction;
  onClose: () => void;
}) {
  const close = useRef<HTMLButtonElement>(null);
  const at = Math.max(0, images.indexOf(shown));
  const go = (by: number) => {
    const next = images[at + by];
    if (next) onShow(next);
  };
  const src = url(shown);
  return (
    <Dialog
      plain
      label={shown}
      className="image-popup"
      initialFocus={close}
      onClose={onClose}
      onKeyDown={(event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
        event.preventDefault();
        go(event.key === "ArrowLeft" ? -1 : 1);
      }}
    >
      <div className="image-popup-bar">
        <span className="image-popup-name truncate">{shown}</span>
        {images.length > 1 ? (
          <>
            <span className="faint">
              {at + 1} of {images.length}
            </span>
            <button
              type="button"
              className="bar-button"
              aria-label="Previous image"
              title="Previous image"
              disabled={at === 0}
              onClick={() => go(-1)}
            >
              <ChevronLeft size={16} />
            </button>
            <button
              type="button"
              className="bar-button"
              aria-label="Next image"
              title="Next image"
              disabled={at === images.length - 1}
              onClick={() => go(1)}
            >
              <ChevronRight size={16} />
            </button>
          </>
        ) : null}
        <button
          type="button"
          className="bar-button"
          aria-label={actionLabel(action, shown)}
          title={actionLabel(action, shown)}
          onClick={() => onSave(shown)}
        >
          <ActionIcon action={action} size={15} />
        </button>
        <button ref={close} type="button" className="bar-button" aria-label="Close" title="Close" onClick={onClose}>
          <X size={16} />
        </button>
      </div>
      {src ? <img className="image-popup-image" src={src} alt={shown} /> : null}
    </Dialog>
  );
}

/** The files of a status, which only a note has. */
export const filesOf = (status: SessionStatus): NoteAttachment[] => status.attachments ?? [];
