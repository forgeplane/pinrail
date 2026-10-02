// A video in the docs. Written as `![alt](video:demo "caption")`, it becomes
// a player: the video's poster with a play button, which opens the video
// large over the page, with a bar of its own and a mark on the timeline for
// each chapter (public/docs.js). The bar names the chapter that is playing,
// with a line on what it shows. The chapters are listed under the poster
// too, each a way in. src/videos.mjs says where each video is and what its chapters are.
// Without the script, the browser's own controls play it where it is.
import { visit } from "unist-util-visit";
import { videos } from "../videos.mjs";

const escape = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const clock = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;

const svg = (body, size = 18) =>
  `<svg viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
const icons = {
  play: svg(
    '<path d="M6 4.5v15a1 1 0 0 0 1.5.86l12.5-7.5a1 1 0 0 0 0-1.72L7.5 3.64A1 1 0 0 0 6 4.5z" fill="currentColor" stroke="none"/>',
  ),
  pause: svg(
    '<rect x="6" y="4" width="4" height="16" rx="1" fill="currentColor" stroke="none"/><rect x="14" y="4" width="4" height="16" rx="1" fill="currentColor" stroke="none"/>',
  ),
  sound: svg('<path d="M11 5 6 9H2v6h4l5 4z"/><path d="M15.5 8.5a5 5 0 0 1 0 7"/><path d="M18.5 5.5a9 9 0 0 1 0 13"/>'),
  muted: svg('<path d="M11 5 6 9H2v6h4l5 4z"/><path d="m22 9-6 6"/><path d="m16 9 6 6"/>'),
  full: svg(
    '<path d="M8 3H5a2 2 0 0 0-2 2v3"/><path d="M21 8V5a2 2 0 0 0-2-2h-3"/><path d="M3 16v3a2 2 0 0 0 2 2h3"/><path d="M16 21h3a2 2 0 0 0 2-2v-3"/>',
  ),
  close: svg('<path d="M18 6 6 18"/><path d="m6 6 12 12"/>'),
};

function player(name, alt, caption) {
  const v = videos[name];
  if (!v) throw new Error(`video:${name} is not in src/videos.mjs`);
  const marks = v.chapters
    .map(
      (c, i) =>
        // a name near either end of the bar reads inwards, so it stays inside the player
        `<button type="button" class="pr-video-mark${c.at / v.seconds < 0.3 ? " is-start" : c.at / v.seconds > 0.6 ? " is-end" : ""}" data-at="${c.at}" style="left:${((c.at / v.seconds) * 100).toFixed(3)}%" aria-label="${escape(`Chapter ${i + 1}: ${c.title}. ${c.detail}`)}"><span class="pr-video-tip"><b>${escape(c.title)}</b><span>${escape(c.detail)}</span></span></button>`,
    )
    .join("");
  const chapters = v.chapters
    .map(
      (c) =>
        `<li><button type="button" class="pr-video-jump" data-at="${c.at}"><span>${clock(c.at)}</span>${escape(c.title)}</button></li>`,
    )
    .join("");
  return `<figure class="pr-video" data-video data-seconds="${v.seconds}">
<div class="pr-video-frame not-content">
<div class="pr-video-shell">
<div class="pr-video-backdrop" data-video-close></div>
<div class="pr-video-dialog" role="group" aria-label="${alt}">
<div class="pr-video-surface">
<video controls playsinline preload="none" poster="${v.poster}"><source src="${v.src}" type="video/mp4" /></video>
<button type="button" class="pr-video-start" aria-label="${escape(`Play: ${alt}`)}"><span class="pr-video-play">${svg('<path d="M7 4.5v15a1 1 0 0 0 1.5.86l12.5-7.5a1 1 0 0 0 0-1.72L8.5 3.64A1 1 0 0 0 7 4.5z" fill="currentColor" stroke="none"/>', 34)}</span><span class="pr-video-label">${escape(v.label)} · ${clock(v.seconds)}</span></button>
</div>
<div class="pr-video-bar">
<div class="pr-video-track" role="slider" tabindex="0" aria-label="Seek" aria-valuemin="0" aria-valuemax="${Math.round(v.seconds)}" aria-valuenow="0"><span class="pr-video-progress"></span>${marks}</div>
<button type="button" class="pr-video-button pr-video-toggle" aria-label="Play">${icons.play}${icons.pause}</button>
<span class="pr-video-time">0:00 / ${clock(v.seconds)}</span>
<span class="pr-video-chapter" aria-live="polite"><b>${escape(v.chapters[0].title)}</b><span>${escape(v.chapters[0].detail)}</span></span>
<button type="button" class="pr-video-button pr-video-mute" aria-label="Mute">${icons.sound}${icons.muted}</button>
<button type="button" class="pr-video-button pr-video-full" aria-label="Full screen">${icons.full}</button>
<button type="button" class="pr-video-button" data-video-close aria-label="Close the video">${icons.close}</button>
</div>
</div>
</div>
</div>
<ol class="pr-video-chapters not-content" aria-label="Chapters">${chapters}</ol>
${caption ? `<figcaption>${escape(caption)}</figcaption>` : ""}
</figure>`;
}

const isVideo = (child) => child.type === "image" && child.url.startsWith("video:");

export default function remarkVideo() {
  return (tree) => {
    visit(tree, "paragraph", (node, index, parent) => {
      if (!parent || node.children.length !== 1 || !isVideo(node.children[0])) return;
      const [image] = node.children;
      parent.children[index] = {
        type: "html",
        value: player(image.url.slice("video:".length), escape(image.alt ?? ""), image.title),
      };
    });
  };
}
