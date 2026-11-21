const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");
const fixture = require("./settings-fixture.cjs");

async function assertAnchored(page, field, name) {
  const list = page.locator(`[id=${JSON.stringify(await field.getAttribute("aria-controls"))}]`);
  const anchor = await field.boundingBox();
  const menu = await list.boundingBox();
  const pane = await page.locator(".app-shell > main").boundingBox();
  assert(menu, `${name} should show its menu`);
  assert(Math.abs(menu.x - anchor.x) <= 1 && Math.abs(menu.width - anchor.width) <= 1,
    `${name} menu should match the field horizontally`);
  const gap = Math.max(menu.y - anchor.y - anchor.height, anchor.y - menu.y - menu.height);
  assert(gap >= 0 && gap <= 6, `${name} menu is ${gap}px from its field`);
  assert(menu.y >= pane.y - 1 && menu.y + menu.height <= pane.y + pane.height + 1,
    `${name} menu should fit inside the Settings pane`);
  return list;
}

async function verifyGeneral(page) {
  const theme = page.locator('[data-setting-path="general.ui_theme"] input');
  const language = page.locator("#ui-language");
  for (const field of [theme, language]) {
    await field.click();
    await assertAnchored(page, field, await field.getAttribute("id"));
    assert.notEqual(await field.inputValue(), "", "A fixed-choice field keeps its selected label while open");
    await field.press("Escape");
  }
  await language.press("Space");
  await assertAnchored(page, language, "Keyboard-opened language menu");
  await page.locator(".app-shell > main").evaluate((pane) => { pane.scrollTop -= 24; });
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await assertAnchored(page, language, "Language after scrolling");
  await language.press("Tab");
  assert.equal(await language.getAttribute("aria-expanded"), "false", "Tab closes the menu and advances focus");
  assert.notEqual(await page.evaluate(() => document.activeElement?.id), "ui-language");
}

async function auditDropdowns(page) {
  let tested = 0;
  async function checkVisibleFields() {
    const fields = page.locator("main input.combo-input:enabled:visible");
    for (const field of await fields.all()) {
      const name = await field.getAttribute("id");
      await field.click();
      await assertAnchored(page, field, name);
      await field.press("Escape");
      tested++;
    }
  }
  for (const section of ["general", "audio", "reading", "models", "cleanup", "advanced"]) {
    await page.locator(`nav button[data-page="${section}"]`).click();
    if (section === "models") {
      for (const feature of ["stt", "tts", "ocr", "cleanup"]) {
        await page.locator(`[data-catalog-feature="${feature}"]`).click();
        await checkVisibleFields();
      }
    } else {
      await checkVisibleFields();
    }
  }
  return tested;
}

async function verifyShortWindowSearch(page) {
  await page.setViewportSize({ width: 900, height: 300 });
  const search = page.locator("#settings-search");
  const list = page.locator("#settings-search-results");
  await search.fill("model");
  const menu = await list.boundingBox();
  assert(menu.y + menu.height <= 300, "Search results fit in a short window");
  for (let i = 1; i < await list.getByRole("option").count(); i++) {
    await search.press("ArrowDown");
  }
  const id = await search.getAttribute("aria-activedescendant");
  const last = await list.locator(`#${id}`).boundingBox();
  assert(last.y >= menu.y && last.y + last.height <= menu.y + menu.height,
    "Keyboard navigation scrolls the last search result into view");
}

async function main() {
  await new Promise((resolve) => fixture.server.listen(0, "127.0.0.1", resolve));
  const browser = await chromium.launch();
  try {
    fixture.options.platform = "linux";
    const url = `http://127.0.0.1:${fixture.server.address().port}/#token=${fixture.token}`;
    let tested = 0;
    for (const [theme, width, locale] of [["light", 1280, "en"], ["dark", 420, "en"], ["light", 360, "ar"]]) {
      fixture.getConfig().general.ui_theme = theme;
      fixture.getConfig().general.ui_language = locale;
      const page = await browser.newPage({ viewport: { width, height: 600 } });
      await page.goto(url);
      await verifyGeneral(page);
      if (locale === "en" && process.env.SETTINGS_DROPDOWN_SCREENSHOTS) {
        await page.locator("#ui-language").click();
        await page.screenshot({ path: path.join(process.env.SETTINGS_DROPDOWN_SCREENSHOTS, `settings-dropdown-${theme}.png`) });
        await page.locator("#ui-language").press("Escape");
      }
      tested += await auditDropdowns(page);
      if (width === 1280) await verifyShortWindowSearch(page);
      await page.close();
    }
    console.log(`PASS ${tested} Settings dropdown placements across light, dark, compact, and Arabic RTL layouts`);
  } finally {
    await browser.close();
    fixture.server.close();
  }
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
