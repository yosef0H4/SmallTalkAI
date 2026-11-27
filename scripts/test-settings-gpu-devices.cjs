const assert = require("node:assert/strict");
const { chromium } = require("playwright");
const fixture = require("./settings-fixture.cjs");

async function main() {
  const config = fixture.makeConfig();
  config.speech.selection_mode = "single_model";
  config.speech.inference_device = "gpu";
  config.speech.runtime_dir = "/assets/parakeet-vulkan";
  fixture.setConfig(config);
  await new Promise(resolve => fixture.server.listen(0, "127.0.0.1", resolve));

  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(8000);
    const dedicated = { id: "a".repeat(32), name: "AMD Radeon RX 6600", vendor_id: 0x1002, kind: 1 };
    const integrated = { id: "b".repeat(32), name: "AMD Radeon Graphics", vendor_id: 0x1002, kind: 2 };
    const adapters = [dedicated];
    await page.route("**/api/modules", route => {
      const catalog = fixture.inventory();
      catalog.vulkan_adapters = adapters;
      catalog.runtimes.push({
        id: "sherpa-webgpu", feature: "tts", backend: "webgpu",
        engine: "sherpa", label: "Dawn", installed: true,
      });
      return route.fulfill({ json: catalog });
    });
    let dawnRequests = 0;
    const nvidiaName = "NVIDIA GeForce RTX 3050 Ti Laptop GPU";
    const intelName = "Intel Iris Xe Graphics";
    await page.route("**/api/webgpu-adapters", route => {
      dawnRequests += 1;
      return route.fulfill({ json: { adapters: [
        { index: 1, name: nvidiaName, kind: 1, available: true },
        { index: 3, name: null, kind: null, available: true },
        { index: 4, name: integrated.name, kind: 2, available: true },
        { index: 6, name: intelName, kind: 2, available: true },
      ] } });
    });

    await page.goto(`http://127.0.0.1:${fixture.server.address().port}/#token=${fixture.token}`);
    await page.locator("[data-page=models]").click();
    await page.locator('[data-catalog-feature="stt"]').click();

    const card = page.locator('[data-setting-path="stt.single.vulkan_adapter"] [role=combobox]');
    await card.waitFor();
    assert.equal(await card.inputValue(), dedicated.name);
    const cards = page.locator('[data-combo-list="stt.single.vulkan_adapter"]');
    await card.click();
    assert.equal(await cards.getByRole("option").count(), 1);
    await card.press("Escape");
    adapters.push(integrated);
    await page.locator('[data-setting-path="stt.single.runtime"]').getByRole("button", { name: /refresh/i }).click();
    await card.waitFor();
    assert.equal(await card.inputValue(), dedicated.name);
    await card.click();
    assert.equal(await cards.getByRole("option").count(), 2);
    await cards.getByRole("option", { name: integrated.name }).click();
    assert.equal(await card.inputValue(), integrated.name);

    await page.locator('[data-catalog-feature="tts"]').click();
    await fixture.setChoice(page.locator("#tts-mode"), "single_model");
    const voiceRuntime = page.locator('[data-setting-path="tts.single.runtime"] [role=combobox]');
    assert.equal(dawnRequests, 0);
    await fixture.setChoice(voiceRuntime, "sherpa-webgpu");
    await page.getByText(/card names come from the graphics driver/i).waitFor();
    const webgpu = page.locator('[data-setting-path="tts.single.webgpu_adapter"] [role=combobox]');
    await webgpu.waitFor();
    assert.equal(dawnRequests, 1);
    assert.equal(await webgpu.inputValue(), nvidiaName);
    await webgpu.click();
    const webgpuCards = page.locator('[data-combo-list="tts.single.webgpu_adapter"]');
    assert.equal(await webgpuCards.getByRole("option").count(), 4);
    assert.equal(await webgpuCards.getByRole("option", { name: integrated.name }).count(), 1);
    assert.equal(await webgpuCards.getByRole("option", { name: intelName }).count(), 1);
    await webgpuCards.getByRole("option", { name: "Adapter 3" }).click();
    assert.equal(await webgpu.inputValue(), "Adapter 3");
    await page.locator('[data-catalog-feature="stt"]').click();
    await page.locator('[data-catalog-feature="tts"]').click();
    await webgpu.waitFor();
    assert.equal(dawnRequests, 1);
    await Promise.all([
      page.waitForResponse(response => response.url().endsWith("/api/webgpu-adapters")),
      page.getByRole("button", { name: "Scan Dawn adapters" }).click(),
    ]);
    assert.equal(dawnRequests, 2);
    assert.equal(await webgpu.inputValue(), "Adapter 3");
    console.log("PASS single- and multi-GPU Vulkan picker, automatic Dawn scan, cached reopening, and manual refresh");
  } finally {
    await browser.close();
    await new Promise(resolve => fixture.server.close(resolve));
  }
}

main().catch(error => {
  fixture.server.close();
  console.error(error);
  process.exitCode = 1;
});
