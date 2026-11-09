// A link a plugin's view asked to open: opened when its origin is allowed
// for the plugin, and otherwise asked about, one request at a time. Any
// that arrive while the person is being asked are dropped.

import { useCallback, useEffect, useRef, useState } from "react";
import type { Plugin } from "../../api/types";
import type { LinkChoice } from "../../components/LinkDialog";
import { allowedWithoutAsking, allowing, linkRequest, sourceOf, type LinkRequest } from "../../lib/links";
import { openExternal } from "../../lib/native";
import { usePlugins } from "../../state/live";
import { useSettings } from "../../state/settings";

export function useLinkRequests(plugin: Plugin | null | undefined) {
  const { plugins } = usePlugins();
  const { settings: prefs, update: updatePrefs } = useSettings();
  const [asked, setAsked] = useState<LinkRequest | null>(null);
  const asking = useRef(false);
  const prefsNow = useRef(prefs);
  useEffect(() => {
    prefsNow.current = prefs;
  }, [prefs]);

  const onOpen = useCallback(
    (url: string) => {
      if (!plugin || asking.current) return;
      const request = linkRequest(url);
      if (!request) return;
      if (
        allowedWithoutAsking(request, prefsNow.current.links[plugin.name], sourceOf(plugins.get(plugin.name) ?? plugin))
      ) {
        openExternal(url);
        return;
      }
      asking.current = true;
      setAsked(request);
    },
    [plugin, plugins],
  );

  const onChoice = useCallback(
    (choice: LinkChoice) => {
      const request = asked;
      asking.current = false;
      setAsked(null);
      if (!request || !plugin || choice === "cancel") return;
      if (choice === "always" && request.origin) {
        updatePrefs({
          links: {
            [plugin.name]: allowing(
              prefsNow.current.links[plugin.name],
              sourceOf(plugins.get(plugin.name) ?? plugin),
              request.origin,
            ),
          },
        });
      }
      openExternal(request.url);
    },
    [asked, plugin, updatePrefs, plugins],
  );

  return { asked, onOpen, onChoice };
}
