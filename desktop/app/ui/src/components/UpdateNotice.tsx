// The sidebar's word that a new version is here: downloaded and one restart
// away, or out for the package manager to install. Nothing otherwise.

import { CircleArrowUp } from "lucide-react";
import { useState } from "react";
import { openExternal } from "../lib/native";
import { useUpdates } from "../state/updates";

export function UpdateNotice({ onDetails }: { onDetails: () => void }) {
  const { status, restart } = useUpdates();
  const [error, setError] = useState<string | null>(null);
  if (status.state !== "ready" && status.state !== "available") return null;
  const ready = status.state === "ready";
  return (
    <div className="update-notice" data-update-notice>
      <button type="button" className="update-notice-text" onClick={onDetails} title={error ?? "Settings › About"}>
        <CircleArrowUp size={14} />
        <span>
          <b>Pinrail {status.version}</b>
          <small>{ready ? "Ready to install" : "Out now"}</small>
        </span>
      </button>
      {ready ? (
        <button type="button" className="update-notice-go" onClick={() => restart().catch((e) => setError(String(e)))}>
          Restart
        </button>
      ) : (
        <button type="button" className="update-notice-go" onClick={() => openExternal(status.url)}>
          Download
        </button>
      )}
    </div>
  );
}
