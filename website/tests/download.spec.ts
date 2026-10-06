import { expect, test, type Browser } from "@playwright/test";

// The download button shows the build for the visitor's system, which a
// script reads from the browser as the page loads. Each case makes the
// browser report another system.
const latest = "https://github.com/forgeplane/pinrail/releases/latest/download";

async function visit(browser: Browser, platform: string, userAgent: string, path = "/") {
  const context = await browser.newContext({ userAgent });
  const page = await context.newPage();
  await page.addInitScript((platform) => {
    Object.defineProperty(navigator, "platform", { get: () => platform });
    // Chromium's own answer would name the machine the test runs on
    Object.defineProperty(navigator, "userAgentData", { get: () => undefined });
  }, platform);
  await page.goto(path);
  return page;
}

const cases = [
  {
    system: "macOS",
    platform: "MacIntel",
    ua: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/605.1.15 Version/17.5 Safari/605.1.15",
    link: "mac",
    text: "Download for macOS",
    href: `${latest}/pinrail-app-universal.dmg`,
  },
  {
    system: "Linux on x86-64",
    platform: "Linux x86_64",
    ua: "Mozilla/5.0 (X11; Linux x86_64; rv:131.0) Gecko/20100101 Firefox/131.0",
    link: "linux",
    text: "Download for Linux",
    href: `${latest}/pinrail-app-amd64.AppImage`,
  },
  {
    system: "Linux on ARM64",
    platform: "Linux aarch64",
    ua: "Mozilla/5.0 (X11; Linux aarch64; rv:131.0) Gecko/20100101 Firefox/131.0",
    link: "linux-arm",
    text: "Download for Linux on ARM64",
    href: `${latest}/pinrail-app-aarch64.AppImage`,
  },
  {
    system: "Linux on 32-bit ARM, which has no build",
    platform: "Linux armv7l",
    ua: "Mozilla/5.0 (X11; Linux armv7l) AppleWebKit/537.36 Chrome/129.0 Safari/537.36",
    link: "other",
    text: "Download",
    href: "/download/",
  },
  {
    system: "Windows",
    platform: "Win32",
    ua: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/129.0 Safari/537.36",
    link: "windows",
    text: "Windows: coming soon",
    href: "/download/",
  },
  {
    system: "a phone",
    platform: "iPhone",
    ua: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 Version/17.5 Mobile/15E148 Safari/604.1",
    link: "other",
    text: "Download",
    href: "/download/",
  },
];

for (const c of cases) {
  test(`the download button on ${c.system}`, async ({ browser }) => {
    const page = await visit(browser, c.platform, c.ua);
    // the page has the button more than once: each shows one link
    const buttons = page.locator("[data-dl]");
    expect(await buttons.count()).toBeGreaterThan(0);
    for (const button of await buttons.all()) {
      const shown = button.locator("[data-download]:visible");
      await expect(shown).toHaveCount(1);
      await expect(shown).toHaveAttribute("data-download", c.link);
      await expect(shown).toHaveText(c.text);
      await expect(shown).toHaveAttribute("href", c.href);
    }
  });
}

test("the download page lists Linux for x86-64 and ARM64, each in three formats", async ({ browser }) => {
  const page = await visit(browser, "Linux x86_64", cases[1].ua, "/download/");
  const linux = page.locator('[data-download-card="linux"]');
  for (const [arch, files] of [
    ["x86-64", ["pinrail-app-amd64.AppImage", "pinrail-app-amd64.deb", "pinrail-app-x86_64.rpm"]],
    ["arm64", ["pinrail-app-aarch64.AppImage", "pinrail-app-arm64.deb", "pinrail-app-aarch64.rpm"]],
  ] as const) {
    const row = linux.locator(`[data-download-arch="${arch}"]`);
    const links = await row.getByRole("link").evaluateAll((as) => as.map((a) => a.getAttribute("href")));
    expect(links).toEqual(files.map((f) => `${latest}/${f}`));
  }
});
