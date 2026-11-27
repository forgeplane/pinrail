// The messages sent earlier in a session, recalled into its message box
// with the arrow keys, as in a shell or the Claude Code prompt.

import { useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import type { SessionMessage, SessionTurn } from "../../api/types";

/** What the person sent the session, oldest first: the prompts of its
 *  turns, as typed in the terminal, and the messages sent from the box.
 *  A prompt cut short is left out. */
export function sentTexts(messages: SessionMessage[], turns: SessionTurn[]): string[] {
  const sent = [
    ...messages.map((m) => ({ at: m.sent_at, text: m.text })),
    ...turns.filter((t) => t.prompt && !t.prompt_cut).map((t) => ({ at: t.started_at, text: t.prompt as string })),
  ];
  return sent.sort((a, b) => Date.parse(a.at) - Date.parse(b.at)).map((s) => s.text);
}

/**
 * Up shows the message sent before the one shown, and Down the one after
 * it, then the text that was being typed. Within a message of several
 * lines, the arrows first move the caret to its start (Up) or end (Down),
 * and recall a message only from there. A message recalled with Up shows
 * with the caret at its start, and one recalled with Down at its end, so
 * that the next press in the same direction recalls the next message.
 *
 * `sent` holds the texts sent, oldest first. `key` names the session: the
 * recall starts again in another session.
 */
export function useMessageHistory({
  key,
  sent,
  text,
  setText,
}: {
  key: string;
  sent: string[];
  text: string;
  setText: (text: string) => void;
}) {
  // which message is shown, 0 for the newest, and the text it replaced
  const [shown, setShown] = useState<{ key: string; at: number; draft: string } | null>(null);
  const current = shown?.key === key ? shown : null;
  const history = newestFirst(sent);

  // where the caret goes in a recalled text once the box shows it
  const caret = useRef<{ el: HTMLTextAreaElement; atStart: boolean } | null>(null);
  useLayoutEffect(() => {
    const to = caret.current;
    caret.current = null;
    const at = to?.atStart ? 0 : text.length;
    to?.el.setSelectionRange(at, at);
  }, [text]);
  const show = (el: HTMLTextAreaElement, next: string, atStart: boolean) => {
    caret.current = { el, atStart };
    setText(next);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    if (event.shiftKey || event.altKey || event.metaKey || event.ctrlKey || event.nativeEvent.isComposing) return;
    const el = event.currentTarget;
    const from = el.selectionStart === el.selectionEnd ? el.selectionStart : -1;
    if (event.key === "ArrowUp") {
      if (from !== 0) return;
      const at = current ? current.at + 1 : 0;
      if (at >= history.length) return;
      event.preventDefault();
      setShown({ key, at, draft: current ? current.draft : text });
      show(el, history[at], true);
    } else {
      if (!current || from !== text.length) return;
      event.preventDefault();
      if (current.at === 0) {
        setShown(null);
        show(el, current.draft, false);
      } else {
        setShown({ ...current, at: current.at - 1 });
        show(el, history[current.at - 1], false);
      }
    }
  };

  /** Starts the recall again, from the newest message, once one is sent. */
  const reset = () => setShown(null);

  return { onKeyDown, reset };
}

/** The texts, newest first, without a text sent twice in a row. */
function newestFirst(sent: string[]): string[] {
  const out: string[] = [];
  for (let i = sent.length - 1; i >= 0; i--) {
    if (sent[i].trim() && sent[i] !== out[out.length - 1]) out.push(sent[i]);
  }
  return out;
}
