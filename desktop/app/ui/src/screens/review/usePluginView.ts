// The plugin a review renders with and the address of its view, or null
// when the plugin is not there. Opening a review may move it to the version
// installed now. A view resolved for one review carries over to the next
// that uses the same plugin and bundle.

import { useEffect, useState } from "react";
import { api } from "../../api/client";
import type { Plugin, Review } from "../../api/types";
import { usePlugins } from "../../state/live";

type Resolved = {
  key: string;
  plugin: Plugin | null;
  src: string | null;
  /** the bundle the review renders with */
  bundle: string | null;
  /** the bundle installed when it was opened */
  installed: string | null;
  /** why the installed version does not take the review, when it does not */
  refused: string | null;
};

export function usePluginView(review: Review | null, onMoved: (review: Review) => void) {
  const { plugins } = usePlugins();
  const [resolved, setResolved] = useState<Resolved | null>(null);
  // questions have no plugin: the thread shows them
  const pluginKey = review && review.kind !== "question" ? `${review.plugin}@${review.plugin_bundle}` : null;
  // what was resolved for the last review counts only when it is for this one
  const current = resolved && resolved.key === pluginKey ? resolved : null;

  useEffect(() => {
    if (!review || !pluginKey) return;
    if (resolved?.key === pluginKey) return;
    let cancelled = false;
    (async () => {
      try {
        const view = await api.reviewView(review.id);
        if (cancelled) return;
        // the plugin and its URL arrive in one update, for the key they were looked up for
        setResolved({
          key: `${view.review.plugin}@${view.bundle}`,
          plugin: view.plugin.usable ? view.plugin : null,
          src: view.url,
          bundle: view.bundle,
          refused: view.refused,
          installed: view.installed,
        });
        if (view.review.plugin_bundle !== review.plugin_bundle) onMoved(view.review);
      } catch {
        if (!cancelled)
          setResolved({ key: pluginKey, plugin: null, src: null, bundle: null, installed: null, refused: null });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [pluginKey, resolved]); // eslint-disable-line react-hooks/exhaustive-deps -- the key names the plugin and version; the review changes on every event

  // the plugin as installed now: another version, or a linked folder
  // changed since it was stored, may have come since the review was opened
  const installedNow = review ? plugins.get(review.plugin)?.install : undefined;
  const newerInstalled =
    !!current &&
    review?.status === "pending" &&
    !!installedNow &&
    (installedNow.folder_changed ||
      (installedNow.bundle !== current.installed && installedNow.bundle !== current.bundle));

  return {
    /** undefined while it is looked up */
    plugin: current ? current.plugin : undefined,
    src: current?.src ?? null,
    bundle: current?.bundle ?? null,
    refused: current?.refused ?? null,
    newerInstalled,
    /** opens the review again, on the version installed now when it takes it */
    reopen: () => setResolved(null),
  };
}
