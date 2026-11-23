// The setup: opened until it is finished or skipped, asked once when the
// settings first arrive.

import { useState } from "react";
import { useSettings } from "../state/settings";

export function useWelcome() {
  const { settings: prefs, update, loaded } = useSettings();
  // Pinrail opens the setup until it is finished or skipped; asked once,
  // when the settings first arrive
  const [welcome, setWelcome] = useState(false);
  const [asked, setAsked] = useState(false);
  if (loaded && !asked) {
    setAsked(true);
    if (!prefs.welcome.seen) setWelcome(true);
  }
  const closeWelcome = () => {
    setWelcome(false);
    if (!prefs.welcome.seen) void update({ welcome: { seen: true } });
  };

  return { welcome, openWelcome: () => setWelcome(true), closeWelcome };
}
