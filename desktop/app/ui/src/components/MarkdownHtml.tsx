// Markdown, or plain text with its links, rendered once for its text and the
// known key prefixes. A link to a work item opens the item in the app, any
// other link opens in the browser, and a link to an attached file goes to
// `onAttachment`.

import { useCallback, useMemo, type HTMLAttributes, type Ref } from "react";
import { stepDiff } from "../lib/diffView";
import { renderAs, useItemPrefixes, type MarkdownMode } from "../lib/markdown";
import { openAgentLink } from "../lib/native";
import { useOpenItem } from "./ItemChip";

/** Follows a link in what an agent wrote: a work item's key opens the item
 *  in the app, and any other link opens in the browser. */
export function useFollowLink() {
  const openItem = useOpenItem();
  return useCallback(
    (link: HTMLAnchorElement) => {
      if (link.dataset.item) openItem(link.dataset.item);
      else openAgentLink(link.href);
    },
    [openItem],
  );
}

export function MarkdownHtml({
  text,
  mode = "markdown",
  imageUrl,
  onAttachment,
  as: Tag = "div",
  ref,
  ...props
}: Omit<HTMLAttributes<HTMLElement>, "children" | "dangerouslySetInnerHTML"> & {
  text: string;
  mode?: MarkdownMode;
  /** where an image the text shows is, for a note: null shows none */
  imageUrl?: (name: string) => string | null;
  /** a link to an attached file, by its name */
  onAttachment?: (name: string) => void;
  /** a span for text inside a line */
  as?: "div" | "span";
  ref?: Ref<HTMLElement>;
}) {
  const followLink = useFollowLink();
  const prefixes = useItemPrefixes();
  const html = useMemo(() => renderAs(mode, text, prefixes, imageUrl), [mode, text, prefixes, imageUrl]);
  return (
    <Tag
      {...props}
      ref={ref as Ref<HTMLDivElement & HTMLSpanElement>}
      onClick={(event: React.MouseEvent<HTMLElement>) => {
        // a diff view's previous and next file buttons
        const step = (event.target as HTMLElement).closest<HTMLElement>("[data-diff-step]");
        if (step) {
          stepDiff(step);
          return;
        }
        const link = (event.target as HTMLElement).closest("a");
        if (!link) return;
        event.preventDefault();
        const file = link.dataset.attachment;
        if (file !== undefined) onAttachment?.(file);
        else followLink(link);
      }}
      // the renderer keeps no raw HTML, and only safe links
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
