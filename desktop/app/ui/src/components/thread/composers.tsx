// The box under a thread while no question waits: a message to the agent,
// or a disabled box when there is nothing to answer.

import { useRef, useState } from "react";
import { api, errorMessage } from "../../api/client";
import { readStored, writeStored } from "../../lib/storage";
import { useConnected } from "../../state/live";
import { useToast } from "../../state/toasts";
import { ComposeField } from "../ComposeField";
import { useItemPicker } from "../ItemPicker";
import type { SessionMessage } from "../../api/types";
import { MessageQueue } from "./messages";
import { AttachButton } from "../AttachButton";
import { MessageFileChips, useMessageFiles, withPaths } from "./messageFiles";

/** Where a message typed but not sent is kept, by session, while the
 *  window is open. */
const draftKey = (session: { agent: string; id: string }) => `pinrail:draft:message:${session.agent}:${session.id}`;

/** The box under the thread while no question waits: a message to the
 *  agent, typed into its terminal as a prompt when Pinrail runs it
 *  (`typed`), and otherwise received with its next status. `#` lists the
 *  work items, those of `repo` first. The messages that still wait on the
 *  agent show at the top of the box. */
export function MessageComposer({
  session,
  repo,
  typed,
  waiting = [],
  onSent,
}: {
  session: { agent: string; id: string };
  repo: string | null;
  typed: boolean;
  /** the messages the agent has not received yet, oldest first */
  waiting?: SessionMessage[];
  onSent: () => void;
}) {
  const connected = useConnected();
  const toast = useToast();
  // what is typed stays with its session, through a visit to another page
  // or session, until it is sent
  const key = draftKey(session);
  const [draft, setDraft] = useState(() => ({ key, text: readStored(key, { session: true }) ?? "" }));
  if (draft.key !== key) setDraft({ key, text: readStored(key, { session: true }) ?? "" });
  const text = draft.key === key ? draft.text : "";
  const setText = (next: string) => {
    setDraft({ key, text: next });
    writeStored(key, next || null, { session: true });
  };
  const box = useRef<HTMLTextAreaElement>(null);
  const picker = useItemPicker({ box, value: text, onChange: setText, repo });
  const [busy, setBusy] = useState(false);
  const attached = useMessageFiles(session, (message) => toast(message));
  const empty = !text.trim() && !attached.files.length;
  const send = async () => {
    if (empty || busy || attached.saving) return;
    setBusy(true);
    try {
      await api.sendMessage(session.agent, session.id, withPaths(text, attached.files), crypto.randomUUID());
      // the box may show another session by now: only the sent one is cleared
      writeStored(key, null, { session: true });
      setDraft((d) => (d.key === key ? { key, text: "" } : d));
      attached.clear();
      onSent();
    } catch (e) {
      toast(errorMessage(e, "The message was not sent."));
    } finally {
      setBusy(false);
    }
  };
  const queue = waiting.length ? <MessageQueue messages={waiting} session={session} onChanged={onSent} /> : null;
  const chips = <MessageFileChips files={attached.files} onRemove={attached.remove} />;
  return (
    <div className={`thread-composer ${busy ? "is-busy" : ""}`} {...attached.handlers}>
      <ComposeField
        value={text}
        onChange={setText}
        label="Message to the agent"
        placeholder="Message the agent"
        onSubmit={send}
        boxRef={box}
        above={
          queue || attached.files.length ? (
            <>
              {queue}
              {chips}
            </>
          ) : null
        }
        tools={<AttachButton label="Attach files" onFiles={attached.attach} disabled={busy} className="compose-icon" />}
        keys="Enter to send · Shift+Enter for a new line"
        send={{
          label: connected ? "Send the message to the agent" : "Reconnect to send",
          name: "Send the message",
          keys: ["Enter"],
          disabled: busy || !connected || empty || attached.saving,
          testId: "send-message",
        }}
      />
      {picker}
      <p className="thread-composer-hint faint">
        {typed
          ? "Sends to the agent's terminal as your prompt. Paste or drop files to attach them."
          : "The agent receives your message the next time it reports a status. Paste or drop files to attach them."}
      </p>
    </div>
  );
}

export function IdleComposer() {
  return (
    <div className="thread-composer is-idle">
      <ComposeField
        value=""
        onChange={() => {}}
        label="Comment to the agent"
        placeholder="No question is waiting for an answer"
        disabled
      />
    </div>
  );
}
