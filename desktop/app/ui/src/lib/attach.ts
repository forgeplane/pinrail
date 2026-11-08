// Attaching files to Markdown the person writes, a work item's description
// or a comment: each file is uploaded, given a name the Markdown can link
// to, and linked at the cursor, an image shown in place.

import { useCallback, useRef, useState, type ClipboardEvent, type DragEvent, type RefObject } from "react";
import { ApiError, api } from "../api/client";

/** A file the Markdown links to, as the core takes it beside the text. */
export type Attached = { sha256: string; size: number; media_type: string };

/** A name the core takes and a Markdown link can hold: no folders, no
 *  spaces or brackets, not hidden, at most 120 characters. */
export function fileName(name: string): string {
  const cleaned = [...name]
    .filter((c) => c.charCodeAt(0) >= 32)
    .join("")
    .replace(/[\\/\s()[\]<>]+/g, "-")
    .replace(/^[.-]+/, "");
  const dot = cleaned.lastIndexOf(".");
  const [stem, ext] = dot > 0 ? [cleaned.slice(0, dot), cleaned.slice(dot)] : [cleaned, ""];
  return (stem.slice(0, 120 - ext.length) || "file") + ext;
}

/** `name`, or the first of name-2, name-3… that is not taken. */
export function freeName(name: string, taken: (n: string) => boolean): string {
  if (!taken(name)) return name;
  const dot = name.lastIndexOf(".");
  const [stem, ext] = dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ""];
  for (let n = 2; ; n++) {
    const next = `${stem}-${n}${ext}`;
    if (!taken(next)) return next;
  }
}

/** The Markdown that shows a file: an image in place, else a link. */
export const markdownFor = (name: string, mediaType: string) =>
  mediaType.startsWith("image/") ? `![${name}](attachment:${name})` : `[${name}](attachment:${name})`;

/**
 * Uploads files dropped, pasted or picked into a text box, and links each
 * at the cursor. `taken` are the names already in use, such as the files
 * an item's description carries. `files` are those to send with the text.
 */
export function useAttach({
  box,
  onChange,
  taken = [],
  onError,
}: {
  box: RefObject<HTMLTextAreaElement | null>;
  onChange: (text: string) => void;
  taken?: string[];
  onError: (message: string) => void;
}) {
  const [files, setFiles] = useState<Record<string, Attached>>({});
  const [busy, setBusy] = useState(0);
  // the names given so far, kept as each is chosen: a file pasted while
  // another uploads must not take its name
  const given = useRef(new Set<string>());

  const attach = useCallback(
    async (list: File[]) => {
      if (!list.length) return;
      setBusy((n) => n + 1);
      try {
        const added: Record<string, Attached> = {};
        const links: string[] = [];
        const names = list.map((file) => {
          const name = freeName(fileName(file.name || "file"), (n) => taken.includes(n) || given.current.has(n));
          given.current.add(name);
          return name;
        });
        try {
          for (const [i, file] of list.entries()) {
            const stored = await api.uploadFile(file);
            added[names[i]] = stored;
            links.push(markdownFor(names[i], stored.media_type));
          }
        } finally {
          // a name whose file was not uploaded is free again
          for (const name of names) if (!(name in added)) given.current.delete(name);
        }
        setFiles((current) => ({ ...current, ...added }));
        const el = box.current;
        const text = el?.value ?? "";
        const start = el?.selectionStart ?? text.length;
        const end = el?.selectionEnd ?? text.length;
        const inserted = links.join("\n");
        onChange(text.slice(0, start) + inserted + text.slice(end));
        requestAnimationFrame(() => {
          el?.focus();
          el?.setSelectionRange(start + inserted.length, start + inserted.length);
        });
      } catch (error) {
        onError(error instanceof ApiError ? error.message : "The file could not be attached");
      } finally {
        setBusy((n) => n - 1);
      }
    },
    [box, onChange, taken, onError],
  );

  /** what to give the text box's container, so a paste or a drop attaches */
  const handlers = {
    onPaste: (event: ClipboardEvent) => {
      const pasted = Array.from(event.clipboardData.files);
      if (!pasted.length) return;
      event.preventDefault();
      void attach(pasted);
    },
    onDragOver: (event: DragEvent) => {
      if (event.dataTransfer.types.includes("Files")) event.preventDefault();
    },
    onDrop: (event: DragEvent) => {
      const dropped = Array.from(event.dataTransfer.files);
      if (!dropped.length) return;
      event.preventDefault();
      void attach(dropped);
    },
  };

  /** the files the text still links to, to send beside it */
  const linked = (text: string) =>
    Object.fromEntries(Object.entries(files).filter(([name]) => text.includes(`(attachment:${name})`)));

  const reset = () => {
    setFiles({});
    given.current.clear();
  };

  return { attach, handlers, linked, busy: busy > 0, reset };
}
