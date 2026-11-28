import { defineConfig, devices } from "@playwright/test";

const port =
  14000 + [...process.cwd()].reduce((sum, letter) => (sum * 31 + letter.charCodeAt(0)) % 1000, 0);

const safari = { ...devices["Desktop Safari"], viewport: { width: 900, height: 600 } };

export default defineConfig({
  testDir: "scenes",
  testMatch: "*.spec.ts",
  fullyParallel: true,
  workers: process.env["CI"] ? "100%" : undefined,
  reporter: "line",
  use: { baseURL: `http://localhost:${port}`, viewport: { width: 900, height: 600 } },
  projects: [
    { name: "webkit", grepInvert: /@speed/, use: safari },
    ...(process.env["CI"]
      ? []
      : [
          {
            name: "speed",
            grep: /@speed/,
            dependencies: ["webkit"],
            fullyParallel: false,
            use: safari,
          },
        ]),
  ],
  webServer: {
    command: `node_modules/.bin/vite --port ${port} --strictPort`,
    url: `http://localhost:${port}/scene.html`,
    reuseExistingServer: false,
  },
});
