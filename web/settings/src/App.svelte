<script lang="ts">
  import { onMount, tick } from "svelte";
  import { ui, load, eventLoop, save, run, dispose, refreshAutomaticSelections } from "./lib/state.svelte";
  import {catalogView,refreshModules,onAutomaticSelection} from "./lib/modules.svelte";
  import { api } from "./lib/api";
  import { pages, searchEntries, fuzzyScore } from "./lib/settings";
  import Icon from "./components/Icon.svelte";
  import IconButton from "./components/IconButton.svelte";
  import Reading from "./pages/Reading.svelte";
  import General from "./pages/General.svelte";
  import Audio from "./pages/Audio.svelte";
  import Models from "./pages/Models.svelte";
  import ModelSetup, { type SetupDraft } from "./components/ModelSetup.svelte";
  import Cleanup from "./pages/Cleanup.svelte";
  import Advanced from "./pages/Advanced.svelte";
  import type { Page } from "./lib/types";
  import { direction, locale, t, tEnglish } from "./lib/i18n";
  const desktop = "__SMALLTALKAI_DESKTOP__" in window;
  let setupDraft = $state<SetupDraft>();
  let checkingSetup = $state(desktop);
  const showingSetup = $derived(checkingSetup || !!setupDraft && !setupDraft.done);

  function updateSetup(next: SetupDraft) {
    setupDraft = next;
    if (next.theme && ui.config) ui.config.general.ui_theme = next.theme;
    if (next.done) {
      catalogView.feature = "stt";
      catalogView.panel = "models";
      ui.page = "models";
      void run(load);
    }
  }
  let maximized = $state(document.documentElement.dataset.maximized === "true");
  let closeDialog: HTMLDialogElement;
  let exitApp = $state(false);
  let closing = $state(false);
  function updateFrameState() {
    maximized = document.documentElement.dataset.maximized === "true";
  }
  function frameAction(action: "drag" | "minimize" | "maximize") {
    window.open("/__settings_frame/" + action, "_blank");
  }
  function dragFrame(event: MouseEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    frameAction(event.detail === 2 ? "maximize" : "drag");
  }
  let query = $state(""),
    searchIndex = $state(0);
  const minSidebarWidth = 160;
  let sidebarWidth = $state<number>();
  let viewportWidth = $state(window.innerWidth);
  let viewportHeight = $state(window.innerHeight);
  const maxSidebarWidth = $derived(Math.max(minSidebarWidth, Math.min(420, viewportWidth - 460)));
  const defaultSidebarWidth = $derived(viewportWidth <= 1120 || viewportHeight <= 720 ? 184 : 272);
  const visibleSidebarWidth = $derived(Math.min(sidebarWidth ?? defaultSidebarWidth, maxSidebarWidth));
  let settingsMain = $state<HTMLElement>();
  let dragOffset = 0;
  const sidebarDirection = () => direction() === "rtl" ? -1 : 1;

  function resizeSidebar(width: number) {
    sidebarWidth = Math.max(minSidebarWidth, Math.min(maxSidebarWidth, width));
  }

  function startSidebarDrag(event: PointerEvent) {
    if (event.button !== 0 || !event.isPrimary) return;
    event.preventDefault();
    const grip = event.currentTarget as HTMLElement;
    grip.setPointerCapture(event.pointerId);
    dragOffset = visibleSidebarWidth - sidebarDirection() * event.clientX;
  }

  function dragSidebar(event: PointerEvent) {
    if ((event.currentTarget as HTMLElement).hasPointerCapture(event.pointerId))
      resizeSidebar(dragOffset + sidebarDirection() * event.clientX);
  }

  function resizeSidebarWithKeys(event: KeyboardEvent) {
    const step = event.key === "ArrowLeft" ? -16 : event.key === "ArrowRight" ? 16 : 0;
    if (!step) return;
    event.preventDefault();
    resizeSidebar(visibleSidebarWidth + step * sidebarDirection());
  }

  const results = $derived(
    query.trim()
      ? searchEntries
          .filter(
            (e) =>
              (ui.state?.platform === "linux" ||
                e.path !== "diagnostics.clipboard_buffer_notices") &&
              (ui.state?.platform === "windows" ||
                e.path !== "general.show_shortcuts_mode"),
          )
          .map((e) => ({
            ...e,
            score: fuzzyScore(query, `${t(e.label)} ${tEnglish(e.label)} ${e.path} ${t(pages.find(page=>page.id===e.page)?.name||"")} ${"help" in e && e.help ? t(e.help):""} ${"aliases" in e ? e.aliases : ""}`),
          }))
          .filter((e) => e.score >= 0)
          .sort((a, b) => b.score - a.score)
          .slice(0, 7)
      : [],
  );
  const current = $derived(pages.find((p) => p.id === ui.page) || pages[0]);
  onMount(() => {
    onAutomaticSelection(refreshAutomaticSelections);
    const refreshTimer=setInterval(()=>{void refreshModules().catch(()=>{});},2000);
    void refreshModules().catch(()=>{});
    const controller = new AbortController();
    void run(async () => {
      await load();
      if (desktop) {
        try {
          await run(async () => {
            setupDraft = await api<SetupDraft>("/api/model-setup");
          });
        } finally {
          checkingSetup = false;
        }
      }
      if (!controller.signal.aborted)
        void eventLoop(controller.signal);
    });
    updateFrameState();
    window.addEventListener("settings-close-request", close);
    window.addEventListener("settings-window-state", updateFrameState);
    return () => {
      onAutomaticSelection();
      clearInterval(refreshTimer);
      controller.abort();
      dispose();
      window.removeEventListener("settings-close-request", close);
      window.removeEventListener("settings-window-state", updateFrameState);
    };
  });
  $effect(() => {
    if (ui.config)
      document.documentElement.dataset.theme = ui.config.general.ui_theme;
    document.documentElement.lang = locale();
    document.documentElement.dir = direction();
  });
  async function jump(page: Page, path = "") {
    if (page === "models") {
      catalogView.panel=path.startsWith("cleanup.")?"cleanup":"models";
      if(path.startsWith("tts."))catalogView.feature="tts";
      else if(path.startsWith("ocr."))catalogView.feature="ocr";
      else if(path.startsWith("speech."))catalogView.feature="stt";
    }
    ui.page = page;
    query = "";
    await tick();
    settingsMain?.scrollTo(0, 0);
    if (path) {
      const field = document.querySelector(`[data-setting-path="${path}"]`) || document.getElementById(path) || document.querySelector(`[data-setting-path$=".${path.split('.').at(-1)}"]`);
      if (field) {
        for (let p = field.parentElement; p; p = p.parentElement)
          if (p instanceof HTMLDetailsElement) p.open = true;
        field.scrollIntoView({ block: "center" });
        (
          (field.querySelector("input,select,textarea") ||
            field.querySelector("button")) as HTMLElement
        )?.focus();
      }
    }
  }
  function searchKey(event: KeyboardEvent) {
    if (results.length && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      searchIndex = Math.max(0, Math.min(results.length - 1, searchIndex + step));
      document.getElementById(`setting-result-${searchIndex}`)?.scrollIntoView({ block: "nearest" });
    }
    if (event.key === "Enter" && results[searchIndex]) {
      event.preventDefault();
      void jump(results[searchIndex].page, results[searchIndex].path);
    }
    if (event.key === "Escape") query = "";
  }
  function close() {
    if (closing || ui.saving || closeDialog.open) return;
    exitApp = false;
    if (ui.dirty) {
      closeDialog.showModal();
      return;
    }
    void finishClose();
  }
  function requestExit() {
    if (closing || ui.saving || closeDialog.open) return;
    exitApp = true;
    closeDialog.showModal();
  }
  async function finishClose(saveFirst = false) {
    if (closing || ui.saving) return;
    closing = true;
    try {
      if (saveFirst) {
        await save();
        if (ui.dirty || ui.noticeKind === "error") {
          closeDialog.close();
          return;
        }
      }
      await run(async () => {
        if (exitApp) await api("/api/platform-action", { action: "exit_app" });
        if (!window.dispatchEvent(new Event("settings-close-approved", { cancelable: true }))) return;
        await api("/api/close", {});
        window.close();
      });
      closeDialog.close();
    } finally {
      closing = false;
    }
  }
</script>

<svelte:window
  onresize={() => { viewportWidth = window.innerWidth; viewportHeight = window.innerHeight; }}
  onbeforeunload={(event) => {
    // Native title-bar commands use intercepted navigation in the desktop WebView.
    // Its explicit close event owns the save warning.
    if (!desktop && !closing && ui.dirty) {
      event.preventDefault();
      event.returnValue = "";
    }
  }}
/>
{#if desktop}
  <header class="settings-frame">
    <div class="settings-frame-drag" role="presentation" onmousedown={dragFrame}>
      <img src="/smalltalkai-titlebar.png" alt="" height="22" />
      <span>SmallTalk</span>
    </div>
    <button type="button" aria-label={t("ui.minimize_settings")} onclick={() => frameAction("minimize")}>
      <span class="frame-minimize" aria-hidden="true"></span>
    </button>
    <button type="button" aria-label={t(maximized ? "ui.restore_settings" : "ui.maximize_settings")} onclick={() => frameAction("maximize")}>
      <span class={maximized ? "frame-restore" : "frame-maximize"} aria-hidden="true"></span>
    </button>
    <button type="button" aria-label={t("ui.close_settings.5bd91a")} onclick={close}>
      <span class="frame-close" aria-hidden="true"></span>
    </button>
  </header>
{/if}
{#snippet settingsNotice()}
  {#if ui.notice}<div
    id="notice"
    class="notice"
    data-kind={ui.noticeKind}
    role="status"
    aria-live="polite"
  >
    <span>{ui.notice}</span>{#if ui.noticeDetails}<details class="notice-detail" open><summary>{t("ui.details.dc3dec")}</summary><div dir="auto">{ui.noticeDetails}</div></details>{/if}
  </div>{/if}
{/snippet}

<div class="app-shell" class:showing-setup={showingSetup} style:--sidebar-width={visibleSidebarWidth + "px"}>
  {#if !showingSetup}
  <aside id="settings-sidebar" class="rail">
    <div class="brand">
      <img src="/smalltalkai.png" alt="" width="24" height="24" /><strong
        >SmallTalkAI</strong
      >
    </div>
    <div class="settings-search">
      <Icon name="search" size={17} /><input
        id="settings-search"
        type="search"
        role="combobox"
        aria-autocomplete="list"
        aria-label={t("ui.search_all_settings.46e7c7")}
        aria-controls="settings-search-results"
        aria-expanded={results.length > 0}
        aria-activedescendant={results[searchIndex]
          ? `setting-result-${searchIndex}`
          : undefined}
        placeholder={t("ui.find_a_setting.fff44d")}
        bind:value={query}
        oninput={() => (searchIndex = 0)}
        onkeydown={searchKey}
      />{#if results.length}<div
          id="settings-search-results"
          class="settings-search-results"
          role="listbox"
          aria-label={t("ui.settings_search.b702dd")}
        >
          {#each results as result, i}<button
              type="button"
              role="option"
              id={`setting-result-${i}`}
              aria-selected={i === searchIndex}
              onclick={() => jump(result.page, result.path)}
              >{t(result.label)}<small
                >{t(pages.find((p) => p.id === result.page)?.name || "")}</small
              ></button
            >{/each}
        </div>{/if}
    </div>
    <nav aria-label={t("ui.settings_pages.032a4a")}>
      {#each pages as page}<button
          type="button"
          data-page={page.id}
          aria-label={t(page.name)}
          title={t(page.name)}
          aria-current={ui.page === page.id ? "page" : undefined}
          class:active={ui.page === page.id}
          onclick={() => jump(page.id)}
          ><Icon name={page.icon} /><span class="nav-copy"><span>{t(page.name)}</span><small>{t(page.description)}</small></span></button
        >{/each}
    </nav>
    {#if !desktop}<div class="rail-bottom">
      <span
        id="service"
        class="service-pill"
        data-state={ui.state?.service_online ? "online" : "offline"}
        title={ui.state?.service_online
          ? t("ui.capture_service_connected.45ad0f")
          : t("ui.offline_editing.aba8c0")}
        ><i></i><span>{t(ui.state?.service_online ? "Connected" : "Offline")}</span
        ></span
      ><IconButton icon="close" label="ui.close_settings.5bd91a" onclick={close} />
    </div>{/if}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (ARIA window splitters are focusable and interactive) -->
    <div class="sidebar-grip" role="separator" tabindex="0"
      aria-label={t("ui.resize_sidebar")} aria-orientation="vertical" aria-controls="settings-sidebar"
      aria-valuemin={minSidebarWidth} aria-valuemax={maxSidebarWidth} aria-valuenow={visibleSidebarWidth}
      onpointerdown={startSidebarDrag} onpointermove={dragSidebar} onkeydown={resizeSidebarWithKeys}></div>
  </aside>
  {/if}
  <main bind:this={settingsMain}>
    {#if setupDraft && !setupDraft.done}
      <ModelSetup draft={setupDraft} updated={updateSetup} feedback={settingsNotice} exit={desktop ? exitButton : undefined} />
    {:else}
    <header class="page-head">
      <h1>{t(current.name)}</h1>
    </header>
    <form
      id="settings"
      inert={ui.configLoading}
      aria-busy={ui.configLoading}
      onsubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      {#if ui.ready && !checkingSetup}<section data-section={ui.page} class="page-content">
          {#if ui.page === "general"}<General
            />{:else if ui.page === "audio"}<Audio
            />{:else if ui.page === "models"}<Models
            />{:else if ui.page === "reading"}<Reading />{:else if ui.page === "cleanup"}<Cleanup />{:else}<Advanced />{/if}
        </section>{:else}<div
          class="loading-skeleton"
          aria-label={t("ui.configLoading_settings.2ee974")}
        >
          <span></span><span></span><span></span>
        </div>{/if}
    </form>
    {/if}
  </main>
{#if !showingSetup && (desktop || ui.notice || ui.dirty)}<div class="settings-feedback" class:desktop-feedback={desktop}>
{#if ui.noticeDetails}{@render settingsNotice()}{/if}
{#if desktop || ui.dirty || (ui.notice && !ui.noticeDetails)}
<footer class="savebar" id={ui.dirty ? "savebar" : undefined}>
  {#if desktop}{@render exitButton()}{/if}
  <div class="footer-status">
    {#if !ui.noticeDetails}{@render settingsNotice()}{/if}
    {#if ui.dirty}<span id="dirty"><i></i>{t("ui.unsaved_changes.292672")}</span>{/if}
  </div>
  {#if ui.dirty}
    <button
      type="submit"
      form="settings"
      class="primary"
      disabled={ui.saving}>{t(ui.saving ? "Saving…" : "Save")}</button
    >
  {/if}
</footer>
{/if}
</div>{/if}
</div>
{#snippet exitButton()}
  <button class="settings-exit" type="button" aria-label={t("settings.exit_app")} onclick={requestExit}>
    <Icon name="system" size={16} />{t("menu.exit")}
  </button>
{/snippet}
<dialog bind:this={closeDialog} class="close-confirmation" aria-labelledby="close-title"
  aria-describedby="close-description" oncancel={event => { if (closing) event.preventDefault(); }}>
  <h2 id="close-title">{t(exitApp ? "settings.exit_app" : "ui.unsaved_changes.292672")}</h2>
  <div class="close-content">
    <p id="close-description">{t(exitApp
      ? (ui.dirty ? "settings.exit_unsaved_warning" : "settings.exit_warning")
      : "settings.close_unsaved_warning")}</p>
    <div class="close-actions" aria-busy={closing}>
      <button type="button" disabled={closing} onclick={() => closeDialog.close()}>{t("settings.keep_editing")}</button>
      <button type="button" disabled={closing} onclick={() => finishClose()}>{t(exitApp
        ? (ui.dirty ? "settings.discard_and_exit" : "settings.exit_app")
        : "settings.discard_and_close")}</button>
      {#if !exitApp || ui.dirty}<button type="button" class="primary" disabled={closing} onclick={() => finishClose(true)}>
        {t(ui.saving ? "Saving…" : (exitApp ? "settings.save_and_exit" : "settings.save_and_close"))}
      </button>{/if}
    </div>
  </div>
</dialog>

<style>
  .close-confirmation {
    inline-size: min(440px, calc(100% - 24px));
    max-block-size: calc(100% - 24px);
    background: var(--paper);
    padding: 3px;
    box-shadow: var(--raised-edge), 0 6px 18px #0003;
    text-align: start;
  }
  .close-confirmation h2 {
    margin: 0;
    padding: 5px 8px;
    background: var(--title-bar);
    color: var(--title-ink);
    font-size: 14px;
  }
  .close-content { padding: 12px; }
  .close-content p { margin-block: 0 12px; line-height: 1.45; }
  .close-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }
</style>
