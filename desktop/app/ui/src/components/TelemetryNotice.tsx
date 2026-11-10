// The card that tells a new person about anonymous usage data, at the top
// of the inbox. Nothing is recorded until they have pressed OK or
// Settings on it; either records that the notice was shown.

import { useNavigate } from "react-router";
import { useSettings } from "../state/settings";

export function TelemetryNotice() {
  const { settings, update, loaded } = useSettings();
  const navigate = useNavigate();
  if (!loaded || settings.telemetry.notice_shown) return null;
  const shown = () => update({ telemetry: { notice_shown: new Date().toISOString().replace(/\.\d+Z$/, "Z") } });
  return (
    <div className="notice telemetry-notice" role="region" aria-label="Usage data" data-testid="telemetry-notice">
      <p>
        Pinrail sends anonymous usage data and crash reports to help improve it. Everything you review stays on your
        computer: no titles, content, file paths or repository names are sent.
      </p>
      <div className="telemetry-notice-actions">
        <button
          type="button"
          className="chrome-button"
          onClick={() => {
            shown();
            navigate("/", { state: { settings: "privacy" } });
          }}
        >
          Settings
        </button>
        <button type="button" className="chrome-button button-primary" onClick={shown}>
          OK
        </button>
      </div>
    </div>
  );
}
