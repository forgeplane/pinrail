// The box under a thread while no question waits: a message to the agent,
// or a disabled box when there is nothing to answer.

import { useState } from "react";
import { api, errorMessage } from "../../api/client";
import { readStored, writeStored } from "../../lib/storage";
import { useConnected } from "../../state/live";
import { useToast } from "../../state/toasts";
import { ComposeField } from "../ComposeField";

/** Where a message typed but not sent is kept, by session, while the
 *  window is open. */
const draftKey = (session: { agent: string; id: string }) => `pinrail:draft:message:${session.agent}:${session.id}`;

/** The box under the thread while no question waits: a message to the
 *  agent, typed into its terminal as a prompt when Pinrail runs it
 *  (`typed`), and otherwise received with its next status. */
export function MessageComposer({
  session,
  typed,
  onSent,
}: {
  session: { agent: string; id: string };
  typed: boolean;
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
  const [busy, setBusy] = useState(false);
  const send = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    try {
      await api.sendMessage(session.agent, session.id, text, crypto.randomUUID());
      // the box may show another session by now: only the sent one is cleared
      writeStored(key, null, { session: true });
      setDraft((d) => (d.key === key ? { key, text: "" } : d));
      onSent();
    } catch (e) {
      toast(errorMessage(e, "The message was not sent."));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className={`thread-composer ${busy ? "is-busy" : ""}`}>
      <ComposeField
        value={text}
        onChange={setText}
        label="Message to the agent"
        placeholder="Message the agent"
        onSubmit={send}
        keys="Enter to send · Shift+Enter for a new line"
        send={{
          label: connected ? "Send the message to the agent" : "Reconnect to send",
          name: "Send the message",
          keys: ["Enter"],
          disabled: busy || !connected || !text.trim(),
          testId: "send-message",
        }}
      />
      <p className="thread-composer-hint faint">
        {typed
          ? "Sends to the agent's terminal as your prompt."
          : "The agent receives your message the next time it reports a status."}
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
