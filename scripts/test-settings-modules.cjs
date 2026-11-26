const { setChoice } = require("./settings-fixture.cjs");
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {chromium} = require('playwright');
const f = require('./settings-fixture.cjs');

(async () => {
  let browser;
  try {
    f.options.recommendations = {'parakeet-cuda':'nvidia_driver_detected','sherpa-cuda':'nvidia_driver_detected','sherpa-cpu':null,'paddle-cpu':'small_capture_cpu','kokoro':null,'supertonic3':'cpu_voice','paddle-ar-en':'available_ocr'};
    const nvidia = f.assets.find(asset => asset.id === 'nvidia-runtime');
    nvidia.bytes = nvidia.dependencies.reduce((total, id) => total + f.assets.find(asset => asset.id === id).bytes, 0);
    await new Promise(resolve => f.server.listen(0, '127.0.0.1', resolve));
    browser = await chromium.launch({headless:true});
    const page = await browser.newPage({viewport:{width:360,height:900}});
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    let whisperInstalled = false;
    await page.route('**/api/modules', route => {
      const catalog = f.inventory();
      catalog.runtimes.push({id:'whisper-gpu',feature:'stt',engine:'whisper',backend:'vulkan',label:'Whisper Vulkan',asset:null,installed:whisperInstalled,directory:null});
      catalog.models.push({id:'whisper-tiny',feature:'stt',engine:'whisper',label:'Whisper Tiny',languages:['en'],asset:null,installed:false});
      return route.fulfill({json:catalog});
    });
    await page.goto(`http://127.0.0.1:${f.server.address().port}/#token=${f.token}`);
    await page.getByRole('heading',{name:'General'}).waitFor();
    assert.equal(await page.locator('[role=dialog]').count(),0,'empty installation does not open setup');
    await page.locator('[data-page=models]').click();
    let search, section;
    const choose = async (feature, kind) => {
      await page.locator(`[data-catalog-feature=${feature}]`).click();
      section = page.locator(`[data-catalog="${feature}.${kind}"]`);
      search = page.locator(`#${kind}-search`);
      if (kind === 'runtime' && !await section.isVisible()) {
        await page.locator('[data-download-kind="runtime"]').click();
      }
      await section.waitFor();
    };
    const refresh = async () => {await page.locator('#model-section .catalog-toolbar button').click();await page.waitForFunction(()=>document.querySelector('#model-section .catalog-toolbar button')?.getAttribute('aria-busy')==='false');};
    const row = id => page.locator(`[data-model-id="${id}"]`);
    for (const feature of ['stt','tts','ocr']) {
      await choose(feature,'model');
      const runtime = page.locator(`[data-catalog="${feature}.runtime"]`);
      const model = page.locator(`[data-catalog="${feature}.model"]`);
      assert(await model.isVisible(), 'models are the primary catalog');
      assert.equal(await page.locator('[data-download-kind="runtime"]').count(), 1);
      const selection = page.locator(`[data-catalog-selection="${feature}"]`);
      await selection.locator(`#${feature}-mode`).waitFor();
      const assignments = await selection.locator('.engine-assignment [role="combobox"][id]').evaluateAll(nodes => nodes.map(node => node.id));
      assert.equal(new Set(assignments).size, assignments.length, `${feature} has one editor per binding`);
      assert(await selection.evaluate((el, runtimeEl) => !!(el.compareDocumentPosition(runtimeEl) & Node.DOCUMENT_POSITION_FOLLOWING), await runtime.elementHandle()), 'selection precedes runtime installation');
      assert.equal(await selection.locator('.module-download-link').count(), 0, 'no link back to this page');
      assert(await model.evaluate((el, runtimeEl) => !!(el.compareDocumentPosition(runtimeEl) & Node.DOCUMENT_POSITION_FOLLOWING), await runtime.elementHandle()), 'models precede the optional runtimes');
      assert.equal(await runtime.locator('h2').textContent(),'Runtimes');
      assert.equal((await model.locator('h2').textContent()).trim(),'Models');
      assert.equal(await page.locator('[data-catalog-kind]').count(),0,'no runtime/model tabs');
      assert(await model.locator('.model-result').count() > 0, 'models remain discoverable before a runtime is assigned');
      assert.equal(await runtime.locator('.model-result').count(),feature==='stt'?3:2);
      assert.equal(await runtime.locator('[data-model-id="nvidia-runtime"]').count(), 1, 'one shared optional CUDA download');
    }
    await choose('stt','runtime');
    await setChoice(page.locator('#stt-mode'), 'single_model');
    await choose('tts','runtime');
    await setChoice(page.locator('#tts-mode'), 'single_model');
    await choose('ocr','runtime');
    await choose('tts','runtime');
    assert.equal(await page.locator('#tts-mode').getAttribute("data-value"),'single_model','reader draft survives feature switch');
    await choose('stt','runtime');
    assert.equal(await page.locator('#stt-mode').getAttribute("data-value"),'single_model','dictation draft survives feature switch');
    await page.locator('#savebar button').click();
    await page.locator('#savebar').waitFor({state:'detached'});
    assert.equal(f.getConfig().speech.selection_mode,'single_model');
    assert.equal(f.getReaderConfig().speech.selection.mode,'single_model');
    await setChoice(page.locator('#stt-mode'), 'follow_keyboard');
    await page.locator('#savebar button').click();
    await page.locator('#savebar').waitFor({state:'detached'});
    assert.equal(await row('nvidia-runtime').locator('strong').textContent(),'CUDA');
    assert.match(await row('nvidia-runtime').textContent(),/more memory/i,'the optional runtime explains its memory trade-off');
    await search.fill('NVIDIA'); assert.equal(await row('nvidia-runtime').count(),1);
    assert.equal(await page.locator('#model-search').inputValue(),'','runtime and model searches are independent');
    await search.fill('');
    await row('nvidia-runtime').getByRole('button',{name:'Download',exact:true}).click();
    await row('parakeet-vulkan').getByRole('button',{name:'Download',exact:true}).click();
    await row('parakeet-vulkan').locator('progress').waitFor();
    assert.equal(await page.locator('.model-result progress').count(),2,'independent progress belongs to each item');
    assert.equal(await page.locator('.download-summary').count(),0,'no detached global banner');
    const percent = Math.floor(Math.floor(nvidia.bytes / 4) * 100 / nvidia.bytes);
    assert.equal(await row('nvidia-runtime').locator('progress').getAttribute('value'), String(percent));
    if(process.env.SETTINGS_VISUAL_REVIEW==='1') {
      const output=path.resolve('artifacts/catalog-ui');fs.mkdirSync(output,{recursive:true});
      for(const locale of ['en','ar']) for(const theme of ['light','dark']) {
        await page.locator('[data-page=general]').click();await setChoice(page.locator('#ui-language'), locale);await setChoice(page.locator('[data-setting-path="general.ui_theme"] [role=combobox]'), theme);await page.locator('[data-page=models]').click();await choose('stt','runtime');
        for(const width of [360,1280]) {await page.setViewportSize({width,height:900});await page.evaluate(()=>document.fonts.ready);assert.equal(await page.locator('.model-result progress').count(),2);assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));assert(await page.locator('.model-result progress').evaluateAll(nodes=>nodes.every(node=>{const bar=node.getBoundingClientRect(),row=node.closest('article').getBoundingClientRect();return bar.y>=row.y&&bar.bottom<=row.bottom&&bar.x>=row.x&&bar.right<=row.right+1;})),'progress must stay inside its downloading item');await page.screenshot({path:path.join(output,`${locale}-${theme}-${width}-downloading.png`),fullPage:true});}
      }
      await page.locator('[data-page=general]').click();await setChoice(page.locator('#ui-language'), 'en');await setChoice(page.locator('[data-setting-path="general.ui_theme"] [role=combobox]'), 'auto');await page.setViewportSize({width:360,height:900});await page.locator('[data-page=models]').click();
    }
    await choose('tts','runtime');
    await row('sherpa-cpu').getByRole('button',{name:'Download',exact:true}).click();
    await row('sherpa-cpu').getByText('Queued',{exact:true}).waitFor();
    await choose('stt','runtime');
    await row('nvidia-runtime').getByRole('button',{name:'Cancel CUDA',exact:true}).click();
    await row('nvidia-runtime').locator('progress').waitFor({state:'detached'});
    assert(f.actions.some(action=>action.action==='cancel'&&action.id==='nvidia-runtime'));
    assert.equal(await row('parakeet-vulkan').locator('progress').count(),1,'cancel one does not cancel another');
    await choose('tts','runtime');
    await row('sherpa-cpu').getByText('Downloading…',{exact:true}).waitFor();
    f.installFixture('sherpa-cpu'); await refresh();
    await choose('tts','model');
    await row('supertonic3').waitFor();
    await search.fill('recommended'); assert.equal(await row('supertonic3').count(),1); assert.equal(await row('kokoro').count(),0,'CPU recommendation comes from server policy');
    await search.fill('kokoro');
    await row('kokoro').getByRole('button',{name:'Download',exact:true}).click();
    await choose('ocr','model');
    await search.fill('paddle'); assert.equal(await row('paddle-ar-en').count(),1,'shared runtime enables compatible OCR models');
    await choose('tts','model'); assert.equal(await search.inputValue(),'kokoro','each catalog retains its own search');
    f.installFixture('kokoro'); await refresh();
    await choose('tts','runtime');
    await row('sherpa-cpu').getByRole('button',{name:'Remove',exact:true}).click();
    await choose('tts','model');
    await row('kokoro').getByText('Runtime missing',{exact:true}).waitFor();
    if(process.env.SETTINGS_VISUAL_REVIEW==='1')await page.screenshot({path:path.resolve('artifacts/catalog-ui/en-orphan-model.png'),fullPage:true});
    await search.fill('supertonic');
    await row('supertonic3').waitFor();
    await search.fill('kokoro');
    await row('kokoro').getByRole('button',{name:'Remove',exact:true}).click();
    await row('kokoro').getByTitle('Installed').waitFor({state:'detached'});
    assert(f.actions.some(action=>action.action==='remove'&&action.id==='kokoro'),'orphan model remains removable');
    await search.fill('');
    assert.equal(await row('kokoro').getByTitle('Installed').count(), 0, 'the removed model stays discoverable');
    f.installFixture('parakeet-vulkan'); await choose('stt','runtime'); await refresh();
    await choose('stt','model');
    assert.equal(await row('english-q8.gguf').count(),1);
    f.options.modelDownloadDelay=10000;
    await row('english-q8.gguf').getByRole('button',{name:'Download',exact:true}).click();
    await row('english-q8.gguf').locator('progress').waitFor();
    await row('english-q8.gguf').getByRole('button',{name:'Cancel English Q8',exact:true}).click();
    await row('english-q8.gguf').locator('progress').waitFor({state:'detached'});
    assert(f.actions.some(action=>action.action==='cancel_model_download'&&action.filename==='english-q8.gguf'));
    assert.equal(f.models.find(model=>model.file==='english-q8.gguf').installed,false,'cancelled speech download never becomes installed');
    whisperInstalled=true; await refresh();
    await row('whisper-tiny').waitFor();
    await section.getByRole('button',{name:'Whisper',exact:true}).click();
    assert.equal(await row('whisper-tiny').count(),1); assert.equal(await row('english-q8.gguf').count(),0,'installed runtime engine pill scopes models');
    await section.getByRole('button',{name:'All engines',exact:true}).click();
    f.installFixture('paddle-ar-en'); await refresh();
    await choose('ocr','model');
    for(const query of ['paddle','installed Arabic','Arabic installed','عَرَبِي مُثَبَّت']) {
      await search.fill(query); assert.equal(await row('paddle-ar-en').count(),1,query);
    }
    await search.fill('');
    await section.getByRole('checkbox',{name:'Installed',exact:true}).check();
    await choose('stt','model'); assert.equal(await section.getByRole('checkbox',{name:'Installed',exact:true}).isChecked(),false,'installed filter is scoped');
    // Visual batch: both languages/themes, all six views and supported widths.
    f.options.recommendations.kokoro='nvidia_voice'; f.options.recommendations.supertonic3='multilingual_voice';
    f.installFixture('sherpa-cuda'); f.installFixture('english-q8.gguf');
    const output=path.resolve('artifacts/catalog-ui');if(process.env.SETTINGS_VISUAL_REVIEW==='1')fs.mkdirSync(output,{recursive:true});
    for(const locale of ['en','ar']) {
      await page.locator('[data-page=general]').click();await setChoice(page.locator('#ui-language'), locale);
      for(const theme of ['light','dark']) {
        await setChoice(page.locator('[data-setting-path="general.ui_theme"] [role=combobox]'), theme);
        for(const width of [360,768,1280]) {
          await page.setViewportSize({width,height:900});await page.locator('[data-page=models]').click();
          for(const feature of ['stt','tts','ocr']) for(const kind of ['runtime','model']) {
            await choose(feature,kind);await search.fill('');await section.locator('.catalog-filter input').first().uncheck();
            const all=section.getByRole('button',{name:locale==='en'?'All engines':'كل المحركات',exact:true});if(await all.count())await all.click();
            await page.evaluate(()=>document.fonts.ready);
            assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),`${locale}/${theme}/${width}/${feature}/${kind} overflow`);
            const controls=await page.locator('.feature-catalogs button').evaluateAll(nodes=>nodes.map(node=>({width:node.clientWidth,scroll:node.scrollWidth})));assert(controls.every(node=>node.scroll<=node.width+2),'category labels fit');
            if(process.env.SETTINGS_VISUAL_REVIEW==='1')await page.screenshot({path:path.join(output,`${locale}-${theme}-${width}-${feature}-${kind}.png`),fullPage:true});
          }
          await page.locator('[data-page=general]').click();
        }
      }
    }
    assert.deepEqual(errors,[]);
    console.log('PASS model catalogs and optional runtimes, orphan removal, recommendation search, independent downloads/cancel/queue, filters and EN/AR layouts');
  } finally {await browser?.close();f.closeEvents();await new Promise(resolve=>f.server.close(resolve));}
})().catch(error=>{console.error(error.stack||error);process.exitCode=1;});
