// What's new: shown once for each release that has notes, when the settings
// first load, and again from the menu, the command palette or Settings ›
// About. A fresh install shows nothing and records the installed release as
// seen.

import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { api } from "../api/client";
import { RELEASES, compareVersions, latestNotes, releasesToShow, type CardAction, type Release } from "../lib/whatsNew";
import { useSettings } from "../state/settings";
import type { SettingsSection } from "./settings/SettingsDialog";

export function useWhatsNew({ onSettings }: { onSettings: (section: SettingsSection) => void }) {
  const navigate = useNavigate();
  const { settings: prefs, update, loaded } = useSettings();
  const [whatsNew, setWhatsNew] = useState<Release[] | null>(null);
  const [running, setRunning] = useState<string | null>(null);
  const newsAsked = useRef(false);
  useEffect(() => {
    if (!loaded || newsAsked.current) return;
    newsAsked.current = true;
    const fresh = !prefs.welcome.seen;
    const seen = prefs.whats_new.seen;
    api
      .info()
      .then(({ version }) => {
        setRunning(version);
        const releases = releasesToShow(RELEASES, version, seen, fresh);
        if (releases.length) setWhatsNew(releases);
        // record a release without notes as seen, but never an older one than recorded
        else if (!seen || compareVersions(seen, version) < 0) void update({ whats_new: { seen: version } });
      })
      .catch(() => {});
  }, [loaded, prefs.welcome.seen, prefs.whats_new.seen, update]);
  // opened from the menu, the command palette or Settings › About
  const latest = running ? latestNotes(RELEASES, running, import.meta.env.DEV) : [];
  const openWhatsNew = () => {
    if (latest.length) setWhatsNew(latest);
  };
  const openWhatsNewRef = useRef(openWhatsNew);
  openWhatsNewRef.current = openWhatsNew;
  const closeWhatsNew = () => {
    const seen = prefs.whats_new.seen;
    if (running && (!seen || compareVersions(seen, running) < 0)) void update({ whats_new: { seen: running } });
    setWhatsNew(null);
  };
  const openFromWhatsNew = (action: CardAction) => {
    if (action.settings) onSettings(action.settings as SettingsSection);
    else if (action.to) navigate(action.to);
  };

  return { whatsNew, latest, openWhatsNew, openWhatsNewRef, closeWhatsNew, openFromWhatsNew };
}
