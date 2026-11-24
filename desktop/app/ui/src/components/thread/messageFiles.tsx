// The files attached to a message to an agent: each is saved for the session
// as soon as it is pasted, dropped or picked, shown as a chip above the
// text, and sent as its path at the end of the message.

import { FileText, Image, LoaderCircle, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ClipboardEvent, type DragEvent } from "react";
import { api, errorMessage } from "../../api/client";
import { readStored, writeStored } from "../../lib/storage";
import { quotePath } from "../../lib/shell";

/** A file attached to the message: its name, and its path once saved. */
export type MessageFile = {
  /** tells two files of one name apart while they are saved */
  key: string;
  name: string;
  type: string;
  path: string | null;
  /** an image's picture, while the window that attached it is open */
  preview?: string;
};

/** Where the files of a message not sent yet are kept, by session. */
const storeKey = (session: { agent: string; id: string }) => `pinrail:draft:files:${session.agent}:${session.id}`;

/** The message as the agent receives it: the text, then each file's path
 *  on a line of its own. */
export function withPaths(text: string, files: MessageFile[]): string {
  const paths = files.flatMap((f) => (f.path ? [quotePath(f.path)] : []));
  const body = text.trim();
  if (!paths.length) return body;
  return body ? `${body}\n\n${paths.join("\n")}` : paths.join("\n");
}

/** The files of the message to `session`, which outlive a visit to
 *  another page until the message is sent. */
export function useMessageFiles(session: { agent: string; id: string }, onError: (message: string) => void) {
  const key = storeKey(session);
  const stored = (): MessageFile[] => {
    try {
      const files = JSON.parse(readStored(key, { session: true }) ?? "[]") as MessageFile[];
      return Array.isArray(files) ? files.filter((f) => f.path) : [];
    } catch {
      return [];
    }
  };
  const [state, setState] = useState(() => ({ key, files: stored() }));
  if (state.key !== key) setState({ key, files: stored() });
  const files = state.key === key ? state.files : [];
  const next = useRef(0);

  // what is saved is kept, without the pictures, which do not outlive the window
  useEffect(() => {
    const kept = state.files.filter((f) => f.path).map(({ preview: _, ...f }) => f);
    writeStored(state.key, kept.length ? JSON.stringify(kept) : null, { session: true });
  }, [state]);

  const update = useCallback(
    (change: (files: MessageFile[]) => MessageFile[]) =>
      setState((s) => (s.key === key ? { key, files: change(s.files) } : s)),
    [key],
  );

  const attach = useCallback(
    (list: File[]) => {
      for (const file of list) {
        const added: MessageFile = {
          key: `f${next.current++}`,
          name: file.name || "file",
          type: file.type,
          path: null,
          preview: file.type.startsWith("image/") ? URL.createObjectURL(file) : undefined,
        };
        update((current) => [...current, added]);
        api
          .saveSessionFile(session.agent, session.id, file)
          .then((saved) =>
            update((current) =>
              current.map((f) => (f.key === added.key ? { ...f, name: saved.name, path: saved.path } : f)),
            ),
          )
          .catch((error) => {
            update((current) => current.filter((f) => f.key !== added.key));
            onError(errorMessage(error, `${added.name} could not be attached.`));
          });
      }
    },
    [session.agent, session.id, update, onError],
  );

  const remove = (gone: MessageFile) => {
    if (gone.preview) URL.revokeObjectURL(gone.preview);
    update((current) => current.filter((f) => f.key !== gone.key));
  };

  const clear = () => {
    for (const f of files) if (f.preview) URL.revokeObjectURL(f.preview);
    update(() => []);
  };

  /** what to give the field's container, so a paste or a drop attaches */
  const handlers = {
    onPaste: (event: ClipboardEvent) => {
      const pasted = Array.from(event.clipboardData.files);
      if (!pasted.length) return;
      event.preventDefault();
      attach(pasted);
    },
    onDragOver: (event: DragEvent) => {
      if (event.dataTransfer.types.includes("Files")) event.preventDefault();
    },
    onDrop: (event: DragEvent) => {
      const dropped = Array.from(event.dataTransfer.files);
      if (!dropped.length) return;
      event.preventDefault();
      attach(dropped);
    },
  };

  return { files, attach, remove, clear, handlers, saving: files.some((f) => !f.path) };
}

/** The chips of the files attached to a message. */
export function MessageFileChips({ files, onRemove }: { files: MessageFile[]; onRemove: (file: MessageFile) => void }) {
  if (!files.length) return null;
  return (
    <ul className="message-files" aria-label="Attached files">
      {files.map((file) => (
        <li
          key={file.key}
          className="message-file"
          title={file.path ?? file.name}
          data-saving={!file.path || undefined}
        >
          {file.preview ? (
            <img className="message-file-thumb" src={file.preview} alt="" />
          ) : (
            <span className="message-file-icon" aria-hidden>
              {!file.path ? (
                <LoaderCircle size={14} className="message-file-spinner" />
              ) : file.type.startsWith("image/") ? (
                <Image size={14} />
              ) : (
                <FileText size={14} />
              )}
            </span>
          )}
          <span className="message-file-name truncate">{file.name}</span>
          <button
            type="button"
            className="message-file-remove"
            aria-label={`Remove ${file.name}`}
            onClick={() => onRemove(file)}
          >
            <X size={12} />
          </button>
        </li>
      ))}
    </ul>
  );
}
