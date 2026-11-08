// The setup: opened until it is finished or skipped, asked once when the
// settings first arrive. It steps aside while its first review is decided,
// and comes back on that step once it is.

import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { reviewPath } from "../lib/url";
import { useLive } from "../state/live";
import { useSettings } from "../state/settings";
import { FIRST_REVIEW, type WelcomeAt } from "./welcome/WelcomeDialog";

export function useWelcome() {
  const live = useLive();
  const navigate = useNavigate();
  const { settings: prefs, update, loaded } = useSettings();
  // Pinrail opens the setup until it is finished or skipped; asked once,
  // when the settings first arrive
  const [welcome, setWelcome] = useState<WelcomeAt | null>(null);
  const welcomed = useRef(false);
  useEffect(() => {
    if (!loaded || welcomed.current) return;
    welcomed.current = true;
    if (!prefs.welcome.seen) setWelcome({ step: 0 });
  }, [loaded, prefs.welcome.seen]);
  const closeWelcome = () => {
    setWelcome(null);
    setAway(null);
    if (!prefs.welcome.seen) void update({ welcome: { seen: true } });
  };
  // the setup steps aside while its first review is decided, and comes
  // back on that step once it is
  const [away, setAway] = useState<string | null>(null);
  const openFromWelcome = (id: string) => {
    setAway(id);
    setWelcome(null);
    navigate(reviewPath(id, prefs.inbox.open_in));
  };
  useEffect(() => {
    if (!away || !live.connected || live.pending.some((r) => r.id === away)) return;
    setWelcome({ step: FIRST_REVIEW, sample: away, decided: true });
    setAway(null);
  }, [away, live.connected, live.pending]);

  return { welcome, setWelcome, closeWelcome, openFromWelcome };
}
