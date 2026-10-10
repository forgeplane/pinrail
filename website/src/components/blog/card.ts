// A post's link-preview image, 1200 × 630, drawn when the site builds from
// one template: the Pinrail logo, the post's title and its dek, in the
// colours of the site's social card (public/og.png). Chromium renders it, the one Playwright installs for
// the docs' diagrams.
import { chromium } from "playwright";
import type { Post } from "./posts";

const escape = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// the brand's mark, as Brand.astro draws it, in the card's colours
const mark = `<svg viewBox="0 0 24 24" aria-hidden="true">
  <defs><mask id="c" maskUnits="userSpaceOnUse" x="0" y="0" width="24" height="24">
    <rect width="24" height="24" fill="#fff"/>
    <g transform="rotate(20 12 13.4)"><rect x="9.6" y="2.2" width="4.8" height="20.6" rx="2.4" fill="#000"/></g>
  </mask></defs>
  <rect x="1.2" y="12.6" width="21.6" height="3.4" rx="1.7" fill="#f1ebdf"/>
  <g mask="url(#c)">
    <rect x="4" y="3.4" width="3" height="17.6" rx="1.5" fill="#f1ebdf"/>
    <rect x="17" y="3.4" width="3" height="17.6" rx="1.5" fill="#f1ebdf"/>
  </g>
  <g transform="rotate(20 12 13.4)"><rect x="10.5" y="3.2" width="3" height="18.6" rx="1.5" fill="#e5694f"/></g>
</svg>`;

function html(post: Post): string {
  const { title, dek } = post.data;
  // the title's last word in the red pen, as on the post's page
  const at = title.lastIndexOf(" ");
  return `<!doctype html>
<html><head><meta charset="utf-8">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Bricolage+Grotesque:opsz,wght@12..96,800&family=Geist:wght@800&family=Newsreader:ital,opsz,wght@1,6..72,400;1,6..72,500&display=block">
<style>
  * { margin: 0; box-sizing: border-box; }
  body {
    width: 1200px; height: 630px; overflow: hidden; position: relative;
    padding: 64px 80px 72px; display: flex; flex-direction: column;
    color: #f1ebdf; background-color: #1a1715;
    background-image:
      radial-gradient(60% 70% at 50% 40%, rgba(229, 105, 79, 0.09), transparent 70%),
      radial-gradient(rgba(241, 235, 223, 0.06) 1px, transparent 1px);
    background-size: 100% 100%, 22px 22px;
  }
  .brand { align-self: flex-start; display: flex; align-items: center; gap: 12px; font: 800 38px/1 Geist, sans-serif; letter-spacing: -0.05em; }
  .brand svg { width: 44px; height: 44px; }
  h1 {
    margin-top: auto; max-width: 1000px;
    font: 800 80px/0.98 "Bricolage Grotesque", sans-serif; letter-spacing: -0.035em;
    text-wrap: balance;
  }
  h1 em { font: italic 500 84px/0.98 Newsreader, serif; letter-spacing: -0.02em; color: #e5694f; }
  .dek {
    margin-top: 26px; max-width: 960px;
    font: italic 400 30px/1.35 Newsreader, serif; color: #b3aa9a;
    display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
</style></head>
<body>
  <span class="brand">${mark}pinrail</span>
  <h1>${escape(title.slice(0, at + 1))}<em>${escape(title.slice(at + 1))}</em></h1>
  <p class="dek">${escape(dek)}</p>
</body></html>`;
}

/** The post's card as a PNG. */
export async function card(post: Post): Promise<Buffer> {
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage({ viewport: { width: 1200, height: 630 } });
    await page.setContent(html(post), { waitUntil: "networkidle" });
    await page.evaluate(() => document.fonts.ready);
    return await page.screenshot({ type: "png" });
  } finally {
    await browser.close();
  }
}
