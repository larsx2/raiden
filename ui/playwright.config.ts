import { defineConfig, devices } from "@playwright/test";

const port =
  14000 + [...process.cwd()].reduce((sum, letter) => (sum * 31 + letter.charCodeAt(0)) % 1000, 0);

export default defineConfig({
  testDir: "scenes",
  testMatch: "*.spec.ts",
  fullyParallel: true,
  reporter: "line",
  use: { baseURL: `http://localhost:${port}`, viewport: { width: 900, height: 600 } },
  projects: [
    {
      name: "webkit",
      use: { ...devices["Desktop Safari"], viewport: { width: 900, height: 600 } },
    },
  ],
  webServer: {
    command: `node_modules/.bin/vite --port ${port} --strictPort`,
    url: `http://localhost:${port}/scene.html`,
    reuseExistingServer: false,
  },
});
