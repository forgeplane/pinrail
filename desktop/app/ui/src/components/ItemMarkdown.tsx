// A work item's Markdown, in its description and its comments: the item's
// images shown in place, its other files linked, and keys of other items
// linking to them.

import { useCallback, useEffect, useState } from "react";
import { inTauri, serverUrl } from "../api/client";
import { useToast } from "../state/toasts";
import { MarkdownHtml } from "./MarkdownHtml";

/** The addresses of an item's files, for its description and comments. */
export function useFileUrl(key: string | undefined) {
  const [base, setBase] = useState<string | null>(null);
  useEffect(() => {
    void serverUrl().then(setBase);
  }, []);
  return useCallback(
    (name: string) =>
      base && key ? `${base}/api/v1/items/${encodeURIComponent(key)}/files/${encodeURIComponent(name)}` : null,
    [base, key],
  );
}

/** Opens one of an item's files: in the app, with the system's default
 *  program for it; outside the app, in a new tab. */
export function useOpenItemFile(key: string | undefined, fileUrl: (name: string) => string | null) {
  const toast = useToast();
  return useCallback(
    async (name: string) => {
      if (!inTauri()) {
        const url = fileUrl(name);
        if (url) window.open(url, "_blank", "noreferrer");
        return;
      }
      if (!key) return;
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("open_item_file", { key, name });
      } catch (error) {
        toast(String(error), "danger");
      }
    },
    [key, fileUrl, toast],
  );
}

/** A link to one of an item's files, which opens it as `useOpenItemFile`
 *  does. */
export function ItemFileLink({
  name,
  fileUrl,
  onOpen,
  children,
}: {
  name: string;
  fileUrl: (name: string) => string | null;
  onOpen: (name: string) => void;
  children: React.ReactNode;
}) {
  return (
    <a
      href={fileUrl(name) ?? "#"}
      rel="noreferrer"
      onClick={(event) => {
        event.preventDefault();
        onOpen(name);
      }}
    >
      {children}
    </a>
  );
}

/** Markdown with the item's images shown in place and its other files linked. */
export function Markdown({
  text,
  files,
  fileUrl,
  onOpenFile,
}: {
  text: string;
  files: { name: string; media_type: string }[];
  fileUrl: (n: string) => string | null;
  onOpenFile: (name: string) => void;
}) {
  const imageUrl = useCallback(
    (name: string) => (files.some((f) => f.name === name && f.media_type.startsWith("image/")) ? fileUrl(name) : null),
    [files, fileUrl],
  );
  return (
    <MarkdownHtml
      text={text}
      mode="note"
      imageUrl={imageUrl}
      className="item-md thread-note-body"
      onAttachment={onOpenFile}
    />
  );
}
