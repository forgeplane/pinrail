// About › Updates: where updating stands, with the one thing to do about it
// (look now, restart into the downloaded version, or get a version the
// package manager installs), and whether the app looks by itself.

import { useState } from "react";
import { openExternal } from "../../lib/native";
import { useSettings } from "../../state/settings";
import { useUpdates, type UpdateStatus } from "../../state/updates";
import { Toggle } from "./controls";
import { SettingsRow } from "./layout";

const when = (at: string) => new Date(at).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });

function describe(status: UpdateStatus): string {
  switch (status.state) {
    case "unavailable":
      return "The packaged app updates itself; this development build does not";
    case "idle":
      return "Not checked yet";
    case "checking":
      return "Checking…";
    case "up_to_date":
      return `Pinrail is up to date · checked ${when(status.checked_at)}`;
    case "downloading":
      return `Downloading ${status.version}${status.percent === null ? "…" : ` · ${status.percent}%`}`;
    case "ready":
      return `${status.version} is downloaded; it is installed when Pinrail restarts or quits`;
    case "available":
      return `${status.version} is out; install it the way you installed Pinrail`;
    case "failed":
      return `Could not check · ${when(status.checked_at)}`;
  }
}

export function UpdatesRows() {
  const { settings, update, native } = useSettings();
  const { status, check, restart } = useUpdates();
  const [error, setError] = useState<string | null>(null);
  const unavailable = status.state === "unavailable";

  const run = (action: () => Promise<unknown>) => {
    setError(null);
    action().catch((e) => setError(String(e)));
  };

  const action =
    status.state === "ready" ? (
      <button type="button" className="chrome-button button-primary" onClick={() => run(restart)} data-update-restart>
        Restart to update
      </button>
    ) : status.state === "available" ? (
      <button type="button" className="chrome-button" onClick={() => openExternal(status.url)}>
        Download
      </button>
    ) : (
      <button type="button" className="chrome-button" disabled={unavailable || status.state === "checking" || status.state === "downloading"} onClick={() => run(check)} data-update-check>
        Check for updates
      </button>
    );

  return (
    <>
      <SettingsRow label="Updates" description={describe(status)} note={error ?? (status.state === "failed" ? status.message : undefined)}>
        {action}
      </SettingsRow>
      <SettingsRow label="Check automatically" description="Looks for a new version at start and every few hours, and downloads it in the background">
        <Toggle label="Check for updates automatically" checked={settings.updates.check} disabled={!native} onChange={(v) => update({ updates: { check: v } })} />
      </SettingsRow>
    </>
  );
}
