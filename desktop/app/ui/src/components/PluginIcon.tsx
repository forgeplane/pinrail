// The icon a plugin brings, the SVG its manifest names, or the generic one.
// It is drawn as a mask in the text's colour, like the app's own Lucide
// icons: the plugin's shapes, the app's colour, and nothing in the file runs,
// which matters here, in the app's own window rather than the plugin's frame.

import { Blocks } from "lucide-react";

type Props = { icon?: string | null; size?: number; strokeWidth?: number; className?: string };

export function PluginIcon({ icon, size = 15, strokeWidth = 1.75, className }: Props) {
  if (!icon) return <Blocks size={size} strokeWidth={strokeWidth} className={className} />;
  const url = `url("data:image/svg+xml;charset=utf-8,${encodeURIComponent(icon)}")`;
  return (
    <span
      className={`plugin-icon${className ? ` ${className}` : ""}`}
      aria-hidden="true"
      style={{ width: size, height: size, maskImage: url, WebkitMaskImage: url }}
    />
  );
}
