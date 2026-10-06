import { defineConfig } from "@playwright/test";

// The built site's static files, served as they are deployed, by Python's
// own server: `npm run build` first. (`astro preview` runs one detached
// server per project, which a test cannot start and stop.)
export default defineConfig({
  testDir: "tests",
  timeout: 30_000,
  reporter: [["list"]],
  use: { baseURL: "http://127.0.0.1:4329", headless: true },
  webServer: {
    command: "python3 -m http.server 4329 --bind 127.0.0.1 --directory dist",
    url: "http://127.0.0.1:4329/",
    reuseExistingServer: !process.env.CI,
  },
});
