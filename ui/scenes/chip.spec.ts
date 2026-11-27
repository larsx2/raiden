import { expect, test } from "@playwright/test";

test("a long window title stays on one line, without the app's name and profile", async ({
  page,
}) => {
  const title = `${"3 × 12 walking lunges with 20 kg dumbbells, slow and steady ".repeat(3)}- Zed - Alex (me@example.com)`;
  await page.goto(`/scene.html?fixture=answer&window=${encodeURIComponent(title)}`);
  await page.waitForLoadState("networkidle");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.press("Tab");
  const label = page.locator(".cx-label").first();
  await expect(label).toHaveText(/^3 × 12 walking lunges.*steady$/);
  await expect(page.locator(".cx-place").first()).toHaveText(" in Zed");
  await expect(page.locator(".cx-place").first()).toBeVisible();
  await expect(label).not.toContainText("Alex");
  const chip = await page.locator(".cx").first().boundingBox();
  expect(chip?.height).toBeLessThanOrEqual(26);
  expect(await label.evaluate((element) => element.scrollWidth > element.clientWidth)).toBe(true);
});
