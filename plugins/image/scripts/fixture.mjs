// The Tern fixture: four candidate spot illustrations for the empty inbox
// of a made-up mail app, each in a different format (PNG, JPEG, WebP, SVG),
// the way an image agent would send them with --attach. Draws each one as
// SVG and renders it in Chromium; writes them to fixtures/tern/, with a
// second round of the paper plane that fixes what the first was asked to.
//
//   node scripts/fixture.mjs      (from the plugin folder or the repo root)
// the drawing runs in the page, where Image and document exist
/* global Image, document */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const here = path.dirname(fileURLToPath(import.meta.url));
const out = path.join(here, "..", "fixtures", "tern");
fs.mkdirSync(out, { recursive: true });

const W = 960,
  H = 720;
const C = {
  cream: "#F4EFE6",
  navy: "#1F2A44",
  teal: "#2A9D8F",
  mint: "#A8DADC",
  coral: "#E76F51",
  sand: "#E9C46A",
  bark: "#B7895A",
  white: "#FFFFFF",
};
const svg = (body) =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">${body}</svg>`;
const cloud = (x, y, s = 1) =>
  `<g fill="${C.white}" transform="translate(${x} ${y}) scale(${s})"><rect x="-75" y="0" width="150" height="44" rx="22"/><circle cx="-25" cy="0" r="30"/><circle cx="22" cy="-6" r="38"/></g>`;

// A paper plane looping over hills. Its flaws, for the review to find: a
// cloud cut off at the right edge, and a stray piece of trail under the loop.
const plane = svg(`
  <rect width="${W}" height="${H}" fill="${C.cream}"/>
  <circle cx="770" cy="160" r="72" fill="${C.sand}"/>
  ${cloud(210, 150)}${cloud(560, 110, 0.7)}${cloud(955, 345, 0.9)}
  <path d="M0 560 C 180 470 330 470 500 540 S 820 610 960 520 L960 720 L0 720Z" fill="${C.mint}"/>
  <path d="M0 625 C 220 565 420 595 600 645 S 860 665 960 625 L960 720 L0 720Z" fill="${C.teal}"/>
  <path d="M110 500 C 220 440 250 310 360 335 C 470 360 410 470 335 445 C 260 420 360 290 520 300 C 600 305 640 300 690 292"
        fill="none" stroke="${C.navy}" stroke-width="5" stroke-linecap="round" stroke-dasharray="2 17"/>
  <path d="M395 520 C 430 505 470 510 505 530" fill="none" stroke="${C.navy}" stroke-width="5" stroke-linecap="round" stroke-dasharray="2 17"/>
  <g stroke="${C.navy}" stroke-width="5" stroke-linejoin="round">
    <polygon points="690,300 840,215 738,318" fill="${C.white}"/>
    <polygon points="738,318 840,215 752,356" fill="#DCE6EA"/>
    <polygon points="738,318 752,356 760,326" fill="#C3D2D8"/>
  </g>`);

// The same plane in a second round, with both flaws gone and the nose up.
const planeRevised = plane
  .replace(cloud(955, 345, 0.9), "")
  .replace(/<path d="M395 520[^>]*\/>/, "")
  .replace('points="690,300 840,215 738,318"', 'points="690,300 830,196 738,318"')
  .replace('points="738,318 840,215 752,356"', 'points="738,318 830,196 752,356"');

// A mailbox with a garden growing out of it and a bird on the roof. Its
// flaw: the post stops short of the ground.
const flower = (
  x,
  y,
  c,
) => `<line x1="${x}" y1="${y + 60}" x2="${x}" y2="${y}" stroke="${C.teal}" stroke-width="6" stroke-linecap="round"/>
  <g fill="${c}"><circle cx="${x}" cy="${y - 14}" r="12"/><circle cx="${x + 14}" cy="${y}" r="12"/><circle cx="${x}" cy="${y + 14}" r="12"/><circle cx="${x - 14}" cy="${y}" r="12"/></g>
  <circle cx="${x}" cy="${y}" r="9" fill="${C.sand}"/>`;
const mailbox = svg(`
  <rect width="${W}" height="${H}" fill="${C.cream}"/>
  <circle cx="480" cy="380" r="270" fill="${C.mint}" opacity=".55"/>
  <ellipse cx="480" cy="652" rx="330" ry="38" fill="${C.teal}"/>
  <rect x="458" y="428" width="44" height="178" rx="6" fill="${C.bark}"/>
  ${flower(360, 250, C.coral)}${flower(410, 226, C.white)}${flower(300, 290, C.sand)}
  <path d="M330 440 V330 A62 62 0 0 1 392 268 H540 A62 62 0 0 1 602 330 V440Z" fill="${C.coral}"/>
  <path d="M330 440 V330 A62 62 0 0 1 454 330 V440Z" fill="#C8573D"/>
  <rect x="604" y="302" width="10" height="96" fill="${C.navy}"/>
  <rect x="614" y="302" width="56" height="32" fill="${C.sand}"/>
  <g transform="translate(505 246)">
    <ellipse cx="0" cy="0" rx="42" ry="24" fill="${C.navy}"/>
    <circle cx="34" cy="-20" r="18" fill="${C.navy}"/>
    <polygon points="50,-24 72,-18 50,-14" fill="${C.sand}"/>
    <circle cx="38" cy="-24" r="4" fill="${C.white}"/>
    <path d="M-40 -6 L-70 -20 L-52 4Z" fill="${C.navy}"/>
  </g>
  <path d="M260 650 C 250 610 280 590 300 600 C 300 620 290 640 260 650Z M700 648 C 720 612 690 590 672 604 C 672 626 684 640 700 648Z" fill="${C.teal}"/>`);

// An envelope asleep in a hammock between two palms, at night.
const palm = (
  x0,
  y0,
  x1,
  y1,
  lean,
) => `<path d="M${x0} ${y0} Q ${(x0 + x1) / 2 + lean} ${(y0 + y1) / 2} ${x1} ${y1}" fill="none" stroke="${C.bark}" stroke-width="26" stroke-linecap="round"/>
  <g fill="${C.teal}" transform="translate(${x1} ${y1})">
    <path d="M0 0 C -60 -50 -130 -40 -170 10 C -110 -10 -60 -5 0 0Z"/>
    <path d="M0 0 C 60 -50 130 -40 170 10 C 110 -10 60 -5 0 0Z"/>
    <path d="M0 0 C -30 -70 -10 -120 40 -140 C 10 -100 10 -50 0 0Z"/>
    <path d="M0 0 C -80 10 -120 60 -120 110 C -90 60 -50 30 0 0Z"/>
    <path d="M0 0 C 80 10 120 60 120 110 C 90 60 50 30 0 0Z"/>
  </g>`;
const hammock = svg(`
  <rect width="${W}" height="${H}" fill="${C.navy}"/>
  <circle cx="770" cy="140" r="56" fill="${C.cream}"/><circle cx="796" cy="124" r="50" fill="${C.navy}"/>
  <g fill="${C.cream}">${[
    [120, 90],
    [260, 160],
    [420, 70],
    [560, 180],
    [640, 60],
    [880, 250],
    [90, 300],
    [900, 60],
  ]
    .map(([x, y], i) => `<circle cx="${x}" cy="${y}" r="${i % 3 ? 2.5 : 4}"/>`)
    .join("")}</g>
  <path d="M0 610 C 240 570 700 570 960 620 L960 720 L0 720Z" fill="#17304A"/>
  ${palm(170, 650, 225, 270, -40)}${palm(800, 650, 745, 290, 40)}
  <path d="M232 330 Q 490 640 742 356" fill="none" stroke="${C.sand}" stroke-width="5"/>
  <path d="M262 382 Q 490 600 716 404 Q 490 540 262 382Z" fill="${C.coral}"/>
  <g transform="translate(490 440) rotate(-7)">
    <rect x="-80" y="-52" width="160" height="104" rx="8" fill="${C.white}"/>
    <path d="M-80 -44 L0 12 L80 -44" fill="none" stroke="#C3D2D8" stroke-width="5" stroke-linejoin="round"/>
    <path d="M-34 22 q 10 8 20 0 M14 22 q 10 8 20 0" fill="none" stroke="${C.navy}" stroke-width="4" stroke-linecap="round"/>
  </g>
  <path d="M262 382 Q 490 600 716 404" fill="none" stroke="#C8573D" stroke-width="6"/>
  <g fill="${C.sand}" font-family="Helvetica, Arial, sans-serif" font-weight="700">
    <text x="585" y="330" font-size="34">z</text><text x="620" y="290" font-size="44">z</text><text x="664" y="240" font-size="56">z</text>
  </g>`);

// A tern gliding over the waves with a letter in its beak.
const wave = (y, fill, phase) => {
  let d = `M0 ${y}`;
  for (let x = 0; x <= W; x += 120) d += ` q 30 -22 60 0 t 60 0`;
  return `<path d="${d} L${W + 120} ${H} L0 ${H}Z" fill="${fill}" transform="translate(${-phase} 0)"/>`;
};
const tern = svg(`
  <defs><linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#E4F1F1"/><stop offset="1" stop-color="${C.cream}"/></linearGradient></defs>
  <rect width="${W}" height="${H}" fill="url(#sky)"/>
  <circle cx="200" cy="470" r="90" fill="${C.sand}" opacity=".9"/>
  ${wave(520, C.mint, 0)}${wave(575, C.teal, 40)}${wave(635, C.navy, 15)}
  <g stroke="${C.navy}" stroke-width="4" stroke-linejoin="round">
    <path d="M470 292 C 400 200 300 168 196 188 C 300 214 380 262 440 306Z" fill="${C.white}"/>
    <path d="M400 312 L318 356 L352 322 L304 330Z" fill="${C.white}"/>
    <ellipse cx="480" cy="306" rx="96" ry="28" fill="${C.white}" transform="rotate(-8 480 306)"/>
    <path d="M500 292 C 560 188 664 148 770 158 C 676 198 594 250 524 306Z" fill="#EEF3F5"/>
    <circle cx="578" cy="282" r="26" fill="${C.white}"/>
    <path d="M552 282 A26 26 0 0 1 604 280 L578 276Z" fill="${C.navy}"/>
    <polygon points="600,284 654,292 600,296" fill="${C.coral}"/>
  </g>
  <g transform="translate(676 318) rotate(14)">
    <rect x="-34" y="-22" width="68" height="46" rx="4" fill="${C.white}" stroke="${C.navy}" stroke-width="3"/>
    <path d="M-34 -18 L0 6 L34 -18" fill="none" stroke="${C.navy}" stroke-width="3"/>
    <rect x="16" y="-18" width="12" height="14" fill="${C.coral}"/>
  </g>`);

const browser = await chromium.launch();
const page = await browser.newPage();
async function encode(markup, type, quality) {
  return page.evaluate(
    async ({ markup, type, quality, W, H }) => {
      const img = new Image();
      img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(markup);
      await img.decode();
      const canvas = Object.assign(document.createElement("canvas"), { width: W, height: H });
      canvas.getContext("2d").drawImage(img, 0, 0);
      return canvas.toDataURL(type, quality).split(",")[1];
    },
    { markup, type, quality, W, H },
  );
}
const write = (name, base64) => fs.writeFileSync(path.join(out, name), Buffer.from(base64, "base64"));

write("paper-plane.png", await encode(plane, "image/png"));
write("paper-plane-r2.png", await encode(planeRevised, "image/png"));
write("mailbox.jpg", await encode(mailbox, "image/jpeg", 0.86));
write("hammock.webp", await encode(hammock, "image/webp", 0.86));
fs.writeFileSync(path.join(out, "tern.svg"), tern.trim() + "\n");
await browser.close();

for (const f of fs.readdirSync(out)) console.log(`${f}\t${fs.statSync(path.join(out, f)).size} bytes`);
