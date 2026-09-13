// The icon a plugin declares in its manifest, or the generic one. Icons load
// on demand by name, so the shell ships none of the set it does not use.

import { Blocks } from "lucide-react";
import { DynamicIcon } from "lucide-react/dynamic";
import type { ComponentProps } from "react";

type Props = { icon?: string | null; size?: number; strokeWidth?: number; className?: string };

export function PluginIcon({ icon, size = 15, strokeWidth = 1.75, className }: Props) {
  if (!icon) return <Blocks size={size} strokeWidth={strokeWidth} className={className} />;
  const fallback = () => <Blocks size={size} strokeWidth={strokeWidth} className={className} />;
  return <DynamicIcon name={icon as ComponentProps<typeof DynamicIcon>["name"]} size={size} strokeWidth={strokeWidth} className={className} fallback={fallback} />;
}
