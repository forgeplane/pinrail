// What a review is, wherever reviews are listed: its plugin's icon and name,
// or, for questions an agent asked, the app's question icon.

import { MessageCircleQuestion } from "lucide-react";
import type { Review } from "../api/types";
import { usePlugins } from "../state/live";
import { PluginIcon } from "./PluginIcon";

type Of = Pick<Review, "kind" | "plugin">;

export function ReviewIcon({
  review,
  size = 15,
  strokeWidth = 1.75,
}: {
  review: Of;
  size?: number;
  strokeWidth?: number;
}) {
  const { pluginIcon } = usePlugins();
  if (review.kind === "question") return <MessageCircleQuestion size={size} strokeWidth={strokeWidth} aria-hidden />;
  return <PluginIcon icon={pluginIcon(review.plugin)} size={size} strokeWidth={strokeWidth} />;
}

/** The name a list shows for what a review is. */
export const reviewLabel = (review: Of) => (review.kind === "question" ? "questions" : review.plugin);
