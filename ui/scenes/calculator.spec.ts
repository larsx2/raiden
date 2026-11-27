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

test("typing a conversion shows it under Conversion, and Enter copies the bare number", async ({
  page,
}) => {
  await page.goto("/scene.html?fixture=apps");
  await page.waitForLoadState("load");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("100 usd to mxn");
  await expect(page.locator(".sec").first()).toHaveText("Conversion");
  await expect(page.locator(".row.selected")).toContainText("1,800.00 MXN100 usd to mxnBuilt in");
  await expect(page.locator("#ctx")).toHaveText("Conversion");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => page.evaluate(() => window.sent.find((call) => call.command === "copy_text")?.args))
    .toEqual({ text: "1800.00" });
});

test("units convert without any rates", async ({ page }) => {
  await page.goto("/scene.html?fixture=apps");
  await page.waitForLoadState("load");
  await page.locator(".row.selected").waitFor();
  await page.keyboard.type("100 c to f");
  await expect(page.locator(".row.selected")).toContainText("212 °F100 c to fBuilt in");
});
