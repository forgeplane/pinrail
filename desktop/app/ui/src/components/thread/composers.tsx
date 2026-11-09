// The box under a thread while no question waits: a message to the agent,
// or a disabled box when there is nothing to answer.

import { useState } from "react";
import { ApiError, api } from "../../api/client";
import { useConnected } from "../../state/live";
import { useToast } from "../../state/toasts";
import { ComposeField } from "../ComposeField";

/** The chat box with nothing to answer. */
/** The box under the thread while no question waits: a message to the
 *  agent, which it receives with its next status. */
export function MessageComposer({ session, onSent }: { session: { agent: string; id: string }; onSent: () => void }) {
  const connected = useConnected();
  const toast = useToast();
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const send = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    try {
      await api.sendMessage(session.agent, session.id, text);
      setText("");
      onSent();
    } catch (e) {
      toast(e instanceof ApiError ? e.message : "The message was not sent.");
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
      <p className="thread-composer-hint faint">The agent receives your message the next time it reports a status.</p>
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
