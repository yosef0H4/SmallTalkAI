const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium, webkit } = require("playwright");
const fixture = require("./settings-fixture.cjs");

const profiles = [
  { platform: "linux", locale: "en", theme: "light" },
  { platform: "linux", locale: "ar", theme: "dark" },
  { platform: "windows", locale: "en", theme: "dark" },
  { platform: "windows", locale: "ar", theme: "light" },
];
const viewports = [
  { width: 320, height: 360 },
  { width: 1024, height: 760 },
  ...[320, 420, 499, 500, 501, 599, 600, 601, 699, 700, 701, 900, 1119, 1120, 1121, 1600]
    .map(width => ({ width, height: 900 })),
  ...[600, 719, 720, 721].map(height => ({ width: 1280, height })),
];

function measure() {
  const faults = [];
  const pane = document.querySelector(".app-shell > main");
  const nav = document.querySelector(".rail nav");
  const visible = element => element?.getBoundingClientRect().width && element.getBoundingClientRect().height;
  const bounds = element => element.getBoundingClientRect();
  const tolerance = 1;
  const name = element => element.dataset?.settingPath ||
    element.getAttribute("aria-label") || element.textContent.trim().replace(/\s+/g, " ").slice(0, 60);
  const overflow = (inner, outer) => inner.left < outer.left - tolerance ||
    inner.right > outer.right + tolerance;
  const overlap = (a, b) => Math.min(a.right, b.right) - Math.max(a.left, b.left) > tolerance &&
    Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > tolerance;

  if (document.documentElement.scrollWidth > window.innerWidth + tolerance) {
    faults.push(["horizontal overflow", "Settings window"]);
  }
  if (pane.scrollWidth > pane.clientWidth + tolerance) {
    faults.push(["horizontal overflow", `Settings pane (${pane.scrollWidth - pane.clientWidth}px)`]);
  }

  for (const row of pane.querySelectorAll(".field, .control, .group-head, .reading-actions, .model-result, .download-detail")) {
    if (!visible(row)) continue;
    const children = [...row.children].filter(visible);
    for (const [index, child] of children.entries()) {
      const rect = bounds(child);
      if (overflow(rect, bounds(row))) faults.push(["row overflow", name(child)]);
      if (children.slice(index + 1).some(other => overlap(rect, bounds(other)))) {
        faults.push(["overlapping row content", name(row)]);
      }
    }
  }

  const actionGaps = [
    [".group-actions, .control, .feature-catalogs", 4],
    [".engine-filters", 6],
    [".reading-actions, .download-detail", 8],
  ];
  for (const [selector, minimum] of actionGaps) {
    for (const group of pane.querySelectorAll(selector)) {
      const children = [...group.children].filter(visible);
      for (const [index, first] of children.entries()) {
        const a = bounds(first);
        for (const second of children.slice(index + 1)) {
          const b = bounds(second);
          const gap = Math.max(a.left - b.right, b.left - a.right, a.top - b.bottom, b.top - a.bottom);
          if (gap < minimum - tolerance) {
            faults.push([`action gap below ${minimum}px`, `${name(first)} / ${name(second)}`]);
          }
        }
      }
    }
  }

  const showMore = pane.querySelector(".model-results > button");
  if (showMore) {
    const rect = bounds(showMore);
    const lastResult = showMore.previousElementSibling;
    const followingGroup = showMore.closest(".catalog").nextElementSibling;
    if (lastResult && rect.top - bounds(lastResult).bottom < 12 - tolerance) {
      faults.push(["vertical gap below 12px", "Last result / Show more"]);
    }
    if (followingGroup && bounds(followingGroup).top - rect.bottom < 12 - tolerance) {
      faults.push(["vertical gap below 12px", "Show more / Custom model locations"]);
    }
  }

  for (const button of [...nav.querySelectorAll("button"), ...pane.querySelectorAll(".control button, .group-actions button, .reading-actions button, .shortcut-quick-actions > button")]) {
    if (!visible(button)) continue;
    const walker = document.createTreeWalker(button, NodeFilter.SHOW_TEXT);
    const range = document.createRange();
    const border = bounds(button);
    while (walker.nextNode()) {
      if (!walker.currentNode.textContent.trim()) continue;
      range.selectNodeContents(walker.currentNode);
      const clipped = [...range.getClientRects()].some(rect => overflow(rect, border) ||
        rect.top < border.top - tolerance || rect.bottom > border.bottom + tolerance);
      if (clipped) faults.push(["button text outside its border", name(button)]);
    }
  }

  const showShortcuts = pane.querySelector(".shortcut-quick-actions > button");
  if (showShortcuts) {
    const rtl = document.documentElement.dir === "rtl";
    const edge = rtl ? "left" : "right";
    if (Math.abs(bounds(showShortcuts.parentElement)[edge] - bounds(showShortcuts)[edge]) > tolerance) {
      faults.push(["inline-end alignment", "Show shortcuts"]);
    }
  }
  return faults;
}

async function main() {
  const engine = process.env.SETTINGS_LAYOUT_BROWSER || "chromium";
  const browser = await { chromium, webkit }[engine].launch();
  fixture.installFixture("parakeet-vulkan");
  await new Promise(resolve => fixture.server.listen(0, "127.0.0.1", resolve));
  const findings = new Map();
  let checks = 0;
  try {
    for (const { platform, locale, theme } of profiles) {
      fixture.options.platform = platform;
      Object.assign(fixture.getConfig().general, { ui_theme: theme, ui_language: locale });
      fixture.getReaderConfig().speech.provider = platform === "windows"
        ? { kind: "open_ai_compatible", base_url: "", model: "", voice: "", format: "wav" }
        : { kind: "local_kokoro" };
      const page = await browser.newPage();
      await page.addInitScript(() => { window.__SMALLTALKAI_DESKTOP__ = true; });
      await page.route("**/api/modules", route => {
        const catalog = fixture.inventory();
        for (let index = 1; index <= 32; index++) {
          catalog.models.push({
            id: `layout-test-${index}.gguf`, feature: "stt", engine: "parakeet",
            label: `Catalog test model ${index}`, languages: ["en"], asset: null, installed: false,
          });
        }
        return route.fulfill({ json: catalog });
      });
      await page.goto(`http://127.0.0.1:${fixture.server.address().port}/#token=${fixture.token}`);
      await page.waitForFunction(expected => document.documentElement.lang === expected, locale);
      await page.evaluate(() => document.fonts.ready);
      await page.locator('[data-section="general"]').waitFor();
      for (const viewport of viewports) {
        await page.setViewportSize(viewport);
        const context = `${platform}/${locale}/${theme}/${viewport.width}×${viewport.height}`;
        async function check(surface) {
          checks++;
          for (const [problem, label] of await page.evaluate(measure)) {
            const key = `${surface}: ${problem} · ${label}`;
            if (!findings.has(key)) {
              findings.set(key, { first: context, count: 0 });
              if (process.env.SETTINGS_LAYOUT_SCREENSHOTS && findings.size <= 5) {
                await page.screenshot({ path: path.join(process.env.SETTINGS_LAYOUT_SCREENSHOTS, `layout-${findings.size}.png`) });
              }
            }
            findings.get(key).count++;
          }
        }
        for (const section of ["general", "audio", "reading", "models", "cleanup", "advanced"]) {
          await page.locator(`nav button[data-page="${section}"]`).click();
          const panel = page.locator(`[data-section="${section}"]`);
          await panel.waitFor();
          if (section === "models") {
            for (const [kind, features] of [["model", ["stt", "tts", "ocr", "cleanup"]], ["runtime", ["stt", "tts", "ocr"]]]) {
              for (const feature of features) {
                await panel.locator(`[data-catalog-feature="${feature}"]`).click();
                const disclosure = panel.locator('.optional-runtimes');
                if (kind === "runtime" && await disclosure.count() && await disclosure.getAttribute("open") === null) {
                  await disclosure.locator("summary").click();
                }
                if (kind === "model" && feature === "stt") {
                  const more = panel.locator(".model-results > button");
                  await more.waitFor({ state: "attached" });
                  if (platform === "linux" && locale === "en" && viewport.width === 420 &&
                      process.env.SETTINGS_LAYOUT_SCREENSHOTS) {
                    await more.evaluate(button => button.scrollIntoView({ block: "center" }));
                    const button = await more.boundingBox();
                    const catalog = await panel.locator('[data-catalog="stt.model"]').boundingBox();
                    await page.screenshot({
                      path: path.join(process.env.SETTINGS_LAYOUT_SCREENSHOTS, "catalog-spacing-420.png"),
                      clip: { x: catalog.x, y: button.y - 74, width: catalog.width, height: button.height + 114 },
                    });
                  }
                }
                await check(`${kind}/${feature}`);
              }
            }
          } else {
            await check(section);
            if (section === "general" && platform === "linux" && locale === "en" &&
                [420, 900].includes(viewport.width) && process.env.SETTINGS_LAYOUT_SCREENSHOTS) {
              const header = await panel.locator(".group-head").first().boundingBox();
              const actions = await panel.locator(".shortcut-quick-actions").boundingBox();
              await page.screenshot({
                path: path.join(process.env.SETTINGS_LAYOUT_SCREENSHOTS, `shortcut-alignment-${viewport.width}.png`),
                clip: { x: header.x, y: header.y, width: header.width, height: actions.y + actions.height - header.y },
              });
            }
          }
        }
      }
      await page.close();
    }
  } finally {
    await browser.close();
    fixture.server.close();
  }
  for (const [issue, { first, count }] of findings) console.error(`${issue} (${count} occurrences, first at ${first})`);
  assert.equal(findings.size, 0, `${findings.size} distinct layout problems in ${checks} surface checks`);
  console.log(`PASS ${checks} responsive Settings surface checks in ${engine}`);
}

main().catch(error => { console.error(error); process.exitCode = 1; });
