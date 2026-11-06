// A work item's Markdown, in its description and its comments: the item's
// images shown in place, its other files linked, and keys of other items
// linking to them.

import { useCallback, useEffect, useMemo, useState } from "react";
import { serverUrl } from "../api/client";
import { renderNote, useItemPrefixes } from "../lib/markdown";

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

/** Markdown with the item's images shown in place and its other files linked. */
export function Markdown({
  text,
  files,
  fileUrl,
}: {
  text: string;
  files: { name: string; media_type: string }[];
  fileUrl: (n: string) => string | null;
}) {
  const imageUrl = useCallback(
    (name: string) => (files.some((f) => f.name === name && f.media_type.startsWith("image/")) ? fileUrl(name) : null),
    [files, fileUrl],
  );
  const prefixes = useItemPrefixes();
  // eslint-disable-next-line react-hooks/exhaustive-deps -- rendered again when the key prefixes change
  const html = useMemo(() => renderNote(text, imageUrl), [text, imageUrl, prefixes]);
  return (
    <div
      className="item-md thread-note-body"
      // the renderer keeps no raw HTML and only safe links
      dangerouslySetInnerHTML={{ __html: html }}
      onClick={(event) => {
        const name = (event.target as HTMLElement).closest("a")?.getAttribute("data-attachment");
        if (!name) return;
        event.preventDefault();
        const url = fileUrl(name);
        if (url) window.open(url, "_blank");
      }}
    />
  );
}
