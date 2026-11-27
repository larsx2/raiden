import { expect, test } from "@playwright/test";

test("typing arithmetic shows the answer first, and Enter copies it and hides", async ({
  page,
}) => {
  await page.goto("/scene.html?fixture=apps");
  await page.waitForLoadState("load");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("(2+3)*4");
  await expect(page.locator(".sec").first()).toHaveText("Calculator");
  await expect(page.locator(".row.selected")).toContainText("20(2+3)*4Built in");
  await expect(page.locator(".list .none")).toHaveCount(0);
  await expect(page.locator("#ctx")).toHaveText("Calculator");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => page.evaluate(() => window.sent.map((call) => call.command)))
    .toEqual(expect.arrayContaining(["copy_text", "hide"]));
  expect(
    await page.evaluate(() => window.sent.find((call) => call.command === "copy_text")?.args),
  ).toEqual({ text: "20" });
});
