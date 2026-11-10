// Settings › Privacy: the switches for anonymous usage data and crash
// reports, and what is waiting to be sent and was sent last, as JSON.

import { useEffect, useState } from "react";
import { api, errorMessage } from "../../api/client";
import type { TelemetryStatus } from "../../api/types";
import { useSettings } from "../../state/settings";
import { Toggle } from "./controls";
import { SettingsGroup, SettingsPage, SettingsRow } from "./layout";

export function PrivacySection() {
  const { settings, update } = useSettings();
  const [status, setStatus] = useState<TelemetryStatus | null>(null);
  const [showing, setShowing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // read again after each switch, which may delete what waits
  const load = () =>
    api
      .telemetry()
      .then((s) => {
        setStatus(s);
        setError(null);
      })
      .catch((e) => setError(errorMessage(e)));
  const { usage, crashes } = settings.telemetry;
  useEffect(() => {
    load();
  }, [usage, crashes]);

  const offBy = status?.off_by ?? null;
  const note = offBy ? <span>{offBy} is set in the environment, so nothing is recorded or sent.</span> : undefined;

  return (
    <SettingsPage title="Privacy">
      <SettingsGroup caption="Everything you review stays on your computer. These help improve Pinrail.">
        <SettingsRow
          label="Send usage data"
          description="Which features are used and how often, with the app's version and the system. Never titles, content, file paths or repository names."
          note={note}
        >
          <Toggle
            label="Send usage data"
            checked={usage && !offBy}
            disabled={!!offBy}
            onChange={(v) => update({ telemetry: { usage: v } })}
          />
        </SettingsRow>
        <SettingsRow
          label="Send crash reports"
          description="When Pinrail crashes, the error and where it happened, with file paths and quoted text removed."
          note={note}
        >
          <Toggle
            label="Send crash reports"
            checked={crashes && !offBy}
            disabled={!!offBy}
            onChange={(v) => update({ telemetry: { crashes: v } })}
          />
        </SettingsRow>
        <SettingsRow
          label="What is sent"
          description="The events waiting to be sent and the last 100 sent, exactly as they leave your computer."
          note={error ?? undefined}
        >
          <button
            type="button"
            className="chrome-button"
            aria-expanded={showing}
            onClick={() => {
              if (!showing) load();
              setShowing(!showing);
            }}
          >
            {showing ? "Hide" : "Show what is sent"}
          </button>
        </SettingsRow>
        {showing && status ? (
          <pre className="telemetry-sent" data-testid="telemetry-sent" tabIndex={0}>
            {JSON.stringify(
              { install: status.install, session: status.session, waiting: status.waiting, sent: status.sent },
              null,
              2,
            )}
          </pre>
        ) : null}
      </SettingsGroup>
    </SettingsPage>
  );
}
