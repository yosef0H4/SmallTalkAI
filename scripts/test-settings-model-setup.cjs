const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('playwright');
const f = require('./settings-fixture.cjs');

(async () => {
  let browser;
  let finishVoice;
  try {
    f.options.modelDownloadDelay = 15000;
    f.options.modelQueueNotice = true;
    f.getConfig().general.ui_theme = 'light';
    f.getConfig().general.ui_language = 'en';
    await new Promise(resolve => f.server.listen(0, '127.0.0.1', resolve));
    browser = await chromium.launch({
      headless: true, args: ['--disable-gpu'], ignoreDefaultArgs: ['--hide-scrollbars'],
    });
    const context = await browser.newContext({ viewport: { width: 360, height: 690 } });
    await context.addInitScript(() => { window.__SMALLTALKAI_DESKTOP__ = true; });
    let draft = { done: false, step: 0, models: [null, null, null], theme: null, voice: null };
    await context.route('**/api/model-setup', async route => {
      if (route.request().method() === 'POST') draft = route.request().postDataJSON();
      await route.fulfill({ json: draft });
    });
    let voice;
    let previewId;
    let waveformReads = 0;
    await context.route('**/api/dictation-preview', async route => {
      const request = route.request().postDataJSON();
      if (request.action === 'start') {
        assert.equal(request.language, 'single');
        previewId = request.session_id;
      } else {
        assert.equal(request.session_id, previewId);
      }
      let result = { ok: true };
      if (request.action === 'stop') result = { text: 'My preview transcript.' };
      if (request.action === 'waveform') {
        result = { waveform: ++waveformReads === 1 ? '▁'.repeat(10) : '▁▂▄▅▆▆▅▄▂▁' };
      }
      await route.fulfill({ json: result });
    });
    await context.route('**/api/preview-voice', async route => {
      voice = route.request().postDataJSON();
      await new Promise(resolve => { finishVoice = resolve; });
      await route.fulfill({ json: { ok: true } });
    });
    const voiceWaveform = '▁▂▃▄▆▆▄▃▂▁';
    const voiceMeterSessions = [];
    await context.route('**/api/preview-voice-level', async route => {
      voiceMeterSessions.push(route.request().postDataJSON().session_id);
      await route.fulfill({ json: { waveform: voiceWaveform } });
    });
    const url = 'http://127.0.0.1:' + f.server.address().port + '/#token=' + f.token;
    let page = await context.newPage();
    const button = (name) => page.getByRole('button', { name, exact: true });
    const heading = (name) => page.locator('#model-setup-heading').getByText(name, { exact: true }).or(page.getByRole('heading', { name, exact: true }));
    const choose = (id, name = 'Download') => page.locator('[data-model-id="' + id + '"]').getByRole('button', { name, exact: true }).click();
    await page.goto(url);
    await heading('Appearance').waitFor();
    assert.equal(await button('Light').getAttribute('aria-pressed'), 'true');
    await button('Dark').click();
    await page.locator('html[data-theme=dark]').waitFor({ state: 'attached' });
    await page.emulateMedia({ colorScheme: 'dark' });
    await button('System').click();
    await page.locator('html[data-theme=auto]').waitFor({ state: 'attached' });
    assert.equal(draft.theme, 'auto');
    assert.equal(await page.evaluate(() => getComputedStyle(document.documentElement).colorScheme), 'dark');
    await page.emulateMedia({ colorScheme: 'light' });
    assert.equal(await page.evaluate(() => getComputedStyle(document.documentElement).colorScheme), 'light');
    await button('Light').click();
    await page.locator('html[data-theme=light]').waitFor({ state: 'attached' });
    const themePanel = page.locator('.setup-appearance');
    const fit = async () => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth
      && document.querySelector('.model-setup-actions').getBoundingClientRect().bottom <= innerHeight);
    const footerAtBottom = async label => {
      assert(await fit(), label + ' fits in the window');
      const footer = await page.locator('.model-setup-actions').boundingBox();
      assert(Math.abs(footer.y + footer.height - page.viewportSize().height) <= 1, label + ' reaches the bottom edge');
    };
    assert(await fit(), 'The theme choices and Next fit in a compact window');
    await page.setViewportSize({ width: 1024, height: 720 });
    assert((await themePanel.boundingBox()).width < 600, 'Appearance stays compact in a wide window');
    if (process.env.SETTINGS_VISUAL_REVIEW) {
      fs.mkdirSync('artifacts/windows-validation', { recursive: true });
      await page.screenshot({ path: 'artifacts/windows-validation/first-run-appearance.png' });
    }
    await page.setViewportSize({ width: 360, height: 690 });
    await button('Next').click();
    await heading('Dictation').waitFor();
    assert.equal(draft.step, 2);
    assert.equal(await page.locator('#runtime-heading').count(), 0);
    await footerAtBottom('Model navigation before a choice');
    await page.setViewportSize({ width: 360, height: 360 });
    const scroller = page.locator('.model-setup-scroll');
    await page.locator('#model-heading').waitFor();
    await footerAtBottom('Model navigation at the top of the list');
    const scrollBox = await scroller.boundingBox();
    if (process.env.SETTINGS_VISUAL_REVIEW) {
      await page.screenshot({ path: 'artifacts/windows-validation/first-run-compact-navigation.png' });
    }
    const thumb = {
      x: Math.floor(scrollBox.x + scrollBox.width - 12),
      y: Math.floor(scrollBox.y + 27),
      width: 8, height: 8,
    };
    await page.mouse.move(20, 10);
    const idleThumb = await page.screenshot({ clip: thumb });
    await page.mouse.move(thumb.x + 4, thumb.y + 4);
    assert.deepEqual(await page.screenshot({ clip: thumb }), idleThumb, 'Hover keeps the scrollbar thumb style');
    await page.mouse.down();
    assert.deepEqual(await page.screenshot({ clip: thumb }), idleThumb, 'Pressing keeps the scrollbar thumb style');
    await page.mouse.move(thumb.x + 4, thumb.y + 28);
    await page.mouse.up();
    await page.waitForFunction(() => document.querySelector('.model-setup-scroll').scrollTop > 0, null, { timeout: 2500 });
    await scroller.evaluate(element => element.scrollTop = element.scrollHeight);
    assert(await scroller.evaluate(element => element.scrollTop > 0), 'The model list scrolls independently');
    await footerAtBottom('Model navigation at the bottom of the list');
    await button('Back').click();
    await heading('Appearance').waitFor();
    assert.equal(await scroller.evaluate(element => element.scrollTop), 0);
    await button('Next').click();
    await page.setViewportSize({ width: 1024, height: 620 });
    const beforeDownload = await scroller.boundingBox();
    await choose('english-q8.gguf');
    await page.locator('.model-setup-bottom #notice').getByText('model download queued', { exact: true }).waitFor();
    await page.locator('[data-model-id="english-q8.gguf"] progress').waitFor();
    const duringDownload = await scroller.boundingBox();
    assert.equal(duringDownload.width, beforeDownload.width, 'The queued download notice does not squeeze the model list');
    await footerAtBottom('Model navigation with a download notice');
    if (process.env.SETTINGS_VISUAL_REVIEW) {
      await page.screenshot({ path: 'artifacts/windows-validation/first-run-download-navigation.png' });
    }
    await page.setViewportSize({ width: 360, height: 690 });
    await button('Next').click();
    f.installFixture('parakeet-vulkan');
    f.installFixture('english-q8.gguf');
    await f.setChoice(page.locator('#stt-mode'), 'single_model');
    await f.setChoice(page.locator('[data-setting-path="stt.single.runtime"] [role=combobox]'), 'parakeet-vulkan');
    await f.setChoice(page.locator('[data-setting-path="stt.single.model"] [role=combobox]'), 'english-q8.gguf');
    await button('Save and record').click();
    await page.locator('.preview-waveform').filter({ hasText: '▁▂▄▅▆▆▅▄▂▁' }).waitFor();
    await button('Stop').click();
    const transcript = page.getByRole('textbox', { name: 'Preview transcript' });
    await page.waitForFunction(id => document.getElementById(id)?.value === 'My preview transcript.', await transcript.getAttribute('id'));
    assert.equal(await page.locator('.preview-waveform').textContent(), '▁'.repeat(10));
    assert.equal(f.actions.some(item => item.action === 'test_model'), false);
    await f.setChoice(page.locator('#stt-mode'), 'follow_keyboard');
    await button('Next').click();
    await heading('Image text').waitFor();
    assert.equal(draft.step, 5);
    assert.equal(f.getConfig().speech.selection_mode, 'follow_keyboard');
    await page.close();
    page = await context.newPage();
    await page.goto(url);
    await heading('Image text').waitFor();
    assert.equal(await page.locator('#runtime-heading').count(), 0);
    await choose('paddle-ar-en');
    f.installFixture('sherpa-cpu');
    f.installFixture('paddle-ar-en');
    await button('Next').click();
    await page.locator('#ocr-mode').waitFor();
    await f.setChoice(page.locator('[data-setting-path="ocr.single.runtime"] [role=combobox]'), 'cpu');
    await f.setChoice(page.locator('[data-setting-path="ocr.single.model"] [role=combobox]'), 'paddle-ar-en');
    await button('Save and copy screen text').click();
    await button('Next').click();
    await heading('Read aloud').waitFor();
    assert.equal(draft.step, 8);
    await choose('kokoro');
    f.installFixture('kokoro');
    assert.equal(await button('Test voice').count(), 0, 'Model download step has no voice preview');
    await button('Next').click();
    await f.setChoice(page.locator('#tts-mode'), 'single_model');
    await f.setChoice(page.locator('[data-setting-path="tts.single.runtime"] [role=combobox]'), 'cpu');
    await f.setChoice(page.locator('[data-setting-path="tts.single.model"] [role=combobox]'), 'kokoro');
    await button('Test voice').waitFor();
    await f.setChoice(page.locator('[data-setting-path="tts.single.voice"] [role=combobox]'), 'af_bella');
    const voiceText = page.getByRole('textbox', { name: 'Text to read aloud' });
    assert.match(await voiceText.inputValue(), /SmallTalkAI/);
    await voiceText.fill('Café preview. This is my test sentence.');
    await button('Test voice').click();
    const meter = page.locator('.model-preview .preview-waveform');
    await meter.filter({ hasText: voiceWaveform }).waitFor();
    assert(await button('Testing voice…').isDisabled());
    assert.equal(voice.binding.voice, 'af_bella');
    assert.equal(voice.text, await voiceText.inputValue());
    assert(voiceMeterSessions.every(id => id === voice.session_id));
    finishVoice();
    await button('Test voice').waitFor();
    assert.equal(await meter.textContent(), '▁'.repeat(10));
    await voiceText.clear();
    assert(await button('Test voice').isDisabled());
    const help = page.getByRole('button', { name: /Kokoro may pronounce all-caps/ });
    await help.click();
    assert.equal(await help.getAttribute('aria-expanded'), 'true');
    await page.locator('.help-text').getByText(/“US” may be spoken as “us.”/).waitFor();
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('.help-text').count(), 0);
    if (process.env.SETTINGS_VISUAL_REVIEW) {
      const output = path.resolve('artifacts/windows-validation/first-run-setup.png');
      fs.mkdirSync(path.dirname(output), { recursive: true });
      await page.screenshot({ path: output, fullPage: true });
    }
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await button('Next').click();
    await heading('Keyboard setup').waitFor();
    await footerAtBottom('Finish on the keyboard step');
    const caps = page.locator('[data-setting-path="general.capslock_behavior"] [role="combobox"]');
    assert.equal(await caps.getAttribute('data-value'), 'preserve_tap');
    assert.equal(await page.locator('[id="general.cancel_hotkey"]').inputValue(), 'CapsLock+X');
    await page.getByRole('button', {name: /Choose Always off if/}).click();
    await page.locator('.help-text').getByText(/Choose Always off if/).waitFor();
    await page.keyboard.press('Escape');
    await f.setChoice(caps, 'always_off');
    f.options.hotkey = 'CapsLock+E';
    await button('Record Record shortcut').click();
    await page.waitForFunction(() => document.getElementById('general.record_hotkey').value === 'CapsLock+E');
    await button('Apply shortcuts').click();
    await button('Apply shortcuts').waitFor({ state: 'detached' });
    assert.equal(f.getConfig().general.capslock_behavior, 'always_off');
    assert.equal(f.getConfig().general.record_hotkey, 'CapsLock+E');
    await button('Finish').click();
    await page.locator('[data-section=models]').waitFor();
    await page.locator('.setup-downloads progress').first().waitFor();
    assert.equal(draft.done, true);
    await page.locator('[data-download-kind="runtime"]').click();
    await page.locator('#runtime-heading').waitFor();
    await page.locator('[data-model-id=nvidia-runtime]').waitFor();
    await page.locator('[data-model-id=nvidia-runtime]').getByText(/Recommended for faster repeat read-aloud/).waitFor();
    for (const feature of ['tts', 'ocr']) {
      await page.locator(`[data-catalog-feature="${feature}"]`).click();
      assert.equal(await page.locator('[data-model-id=nvidia-runtime]').count(), 1);
    }
    page = await context.newPage();
    await page.goto(url);
    await page.locator('#ui-language').waitFor();
    assert.equal(await page.locator('#model-setup-heading').count(), 0);
    await page.close();
    draft = { ...draft, done: false, step: 10 };
    f.options.platform = 'linux';
    await context.route('**/api/state', async route => {
      const response = await route.fetch();
      const state = await response.json();
      state.shortcut_state = { record: 'Meta+Z' };
      await route.fulfill({ response, json: state });
    });
    page = await context.newPage();
    await page.goto(url);
    await heading('Keyboard setup').waitFor();
    assert.equal(await page.locator('[data-setting-path="linux.shortcut.record"] output').textContent(), 'Meta+Z');
    await button('Refresh or register system shortcuts').click();
    await Promise.all([
      page.waitForResponse(response => response.request().method() === 'POST' &&
        response.request().postDataJSON()?.action === 'configure_shortcuts'),
      button('Set up shortcuts').click(),
    ]);
    assert(f.actions.some(item => item.action === 'sync_shortcuts'));
    assert(f.actions.some(item => item.action === 'configure_shortcuts'));
    await page.close();
    const shortcutBackend = f.getConfig().general.linux_hotkey_backend;
    f.getConfig().general.linux_hotkey_backend = 'desktop';
    page = await context.newPage();
    await page.goto(url);
    await heading('Keyboard setup').waitFor();
    const shortcutCommands = page.locator('.shortcut-card code');
    await shortcutCommands.first().waitFor();
    const commands = [
      'smalltalkai show-shortcuts', 'smalltalkai cancel', 'smalltalkai repeat',
      'smalltalkai toggle', 'smalltalkai cycle-delivery', 'smalltalkai toggle-cleanup',
      'smalltalkai read-selection', 'smalltalkai listen-clipboard', 'smalltalkai read-along',
      'smalltalkai read-region', 'smalltalkai copy-region',
    ];
    assert.deepEqual(await shortcutCommands.allTextContents(), commands);
    assert.equal(await page.locator('.shortcut-card input').count(), 0);
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    for (const command of commands) {
      await page.getByRole('button', { name: `Copy command ${command}`, exact: true }).click();
      assert.equal(await page.evaluate(() => navigator.clipboard.readText()), command);
    }
    await page.close();
    f.getConfig().general.linux_hotkey_backend = shortcutBackend;
    f.options.platform = 'windows';
    draft = { ...draft, step: 6 };
    page = await context.newPage();
    await page.goto(url);
    await heading('Image text').waitFor();
    await button('Next').click();
    await heading('Read aloud').waitFor();
    await page.locator('#model-heading').waitFor();
    assert.equal(draft.step, 8, 'Read aloud starts on its model picker');
    console.log('PASS first-run guide: theme, overlapping downloads, resume, model routes, previews, Windows shortcut capture, Linux portal actions and copyable desktop commands, and Finish');
  } finally {
    await browser?.close();
    f.closeEvents();
    await new Promise(resolve => f.server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
