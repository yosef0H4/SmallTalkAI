const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");
const fixture = require("./settings-fixture.cjs");

async function verifyDownload(page, id, theme) {
  const row = page.locator(`[data-model-id="${id}"]`);
  await row.getByRole("button", { name: "Download", exact: true }).click();
  const progress = row.getByRole("progressbar");
  await progress.waitFor();
  const state = await progress.evaluate(element => {
    const bar = element.getBoundingClientRect();
    const card = element.closest("article").getBoundingClientRect();
    return {
      contained: bar.top >= card.top && bar.bottom <= card.bottom &&
        bar.left >= card.left && bar.right <= card.right,
      fraction: element.position,
    };
  });
  assert(state.contained, `${id} progress belongs inside its download row`);
  assert(state.fraction > 0 && state.fraction < 1, `${id} has partial download progress`);
  if (process.env.SETTINGS_DOWNLOAD_SCREENSHOTS) {
    await row.screenshot({
      path: path.join(process.env.SETTINGS_DOWNLOAD_SCREENSHOTS, `download-progress-${theme}.png`),
    });
  }
  const job = fixture.assetJobs.find(entry => entry.job_id === id);
  job.total = 0;
  await page.locator(".catalog").filter({ has: row }).locator(".catalog-toolbar button").click();
  await row.locator("progress:not([value])").waitFor();
  assert.equal(await progress.evaluate(element => element.position), -1,
    "Unknown download size uses indeterminate progress");
  await row.getByRole("button", { name: /^Cancel / }).click();
  await progress.waitFor({ state: "detached" });
}

async function main() {
  fixture.options.platform = "linux";
  fixture.getConfig().general.ui_language = "en";
  fixture.installFixture("sherpa-cpu");
  await new Promise(resolve => fixture.server.listen(0, "127.0.0.1", resolve));
  const browser = await chromium.launch();
  try {
    const url = `http://127.0.0.1:${fixture.server.address().port}/#token=${fixture.token}`;
    for (const [id, kind, feature, theme, width] of [
      ["parakeet-vulkan", "runtime", "stt", "dark", 480],
      ["kokoro", "model", "tts", "light", 960],
    ]) {
      fixture.getConfig().general.ui_theme = theme;
      const page = await browser.newPage({ viewport: { width, height: 720 } });
      await page.goto(url);
      await page.waitForFunction(expected => getComputedStyle(document.documentElement).colorScheme === expected, theme);
      await page.locator('[data-page="models"]').click();
      await page.locator(`[data-catalog-feature="${feature}"]`).click();
      if (kind === "runtime") await page.locator('[data-download-kind="runtime"]').click();
      await verifyDownload(page, id, theme);
      await page.close();
    }
    console.log("PASS model and runtime download bars stay inside their rows and disappear on cancel");
  } finally {
    await browser.close();
    fixture.server.close();
  }
}

main().catch(error => { console.error(error); process.exitCode = 1; });
