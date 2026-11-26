<script lang="ts" module>
  export interface SetupDraft {
    done: boolean;
    step: number;
    models: (string | null)[];
    theme: "light" | "dark" | "auto" | null;
    voice: string | null;
  }
</script>

<script lang="ts">
  import { onMount, tick, type Snippet } from "svelte";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n";
  import { modules, refreshModules, type Feature } from "../lib/modules.svelte";
  import { download, notice, refreshAutomaticSelections, save, ui } from "../lib/state.svelte";
  import General from "../pages/General.svelte";
  import Models from "../pages/Models.svelte";
  import ReaderModelSetup from "./ReaderModelSetup.svelte";
  import SttModelSetup from "./SttModelSetup.svelte";

  let { draft, updated, feedback }: {
    draft: SetupDraft;
    updated: (draft: SetupDraft) => void;
    feedback: Snippet;
  } = $props();
  const features = ["stt", "ocr", "tts"] as const;
  const labels = ["settings.dictation", "module.vision", "module.speak"];
  const themes = ["light", "dark", "auto"] as const;
  // Retain the persisted step IDs so existing profiles can resume.
  const steps = [0, 2, 3, 5, 6, 8, 9, 10];
  const index = $derived(Math.min(2, Math.max(0, Math.floor((draft.step - 1) / 3))));
  const picker = $derived(draft.step === 2 || draft.step === 5 || draft.step === 8);
  const configuring = $derived(draft.step === 3 || draft.step === 6 || draft.step === 9);
  const canSkip = $derived(picker && !draft.models[index]);
  const theme = $derived(draft.theme ?? ui.config?.general.ui_theme ?? "light");
  let busy = $state(false);
  let heading: HTMLHeadingElement;
  let stepScroll: HTMLDivElement;

  async function persist(next: SetupDraft) {
    busy = true;
    try {
      updated(await api<SetupDraft>("/api/model-setup", next));
      return true;
    } catch (error) {
      notice(t("ui.operation_failed.c4e6ed"), "error", String(error));
      return false;
    } finally {
      busy = false;
    }
  }

  async function queue(id: string, feature: Feature) {
    const entry = modules.models.find(entry => entry.id === id && entry.feature === feature);
    if (!entry || entry.installed) return;
    try {
      if (entry.asset) await api("/api/asset-action", { action: "install", id: entry.asset });
      else if (ui.state?.service_online) await download(id);
      await refreshModules();
    } catch (error) {
      notice(t("assets.failed"), "error", String(error));
    }
  }

  async function choose(id: string) {
    const current = $state.snapshot(draft);
    const feature = features[index];
    if (feature === "tts" && current.models[index] !== id) current.voice = null;
    current.models[index] = id;
    if (await persist(current)) void queue(id, feature);
  }

  async function applyDraft() {
    busy = true;
    try {
      if (ui.dirty) await save();
      return !ui.dirty;
    } finally {
      busy = false;
    }
  }

  async function advance(direction: -1 | 1, skip = false) {
    const nextIndex = steps.indexOf(draft.step) + direction + (skip ? 1 : 0);
    const done = nextIndex === steps.length;
    const step = done ? 10 : steps[nextIndex];
    const current = $state.snapshot(draft);
    if ((configuring || draft.step === 10) && !(await applyDraft())) return;
    if (step === 3 || step === 6 || step === 9 || step === 10) {
      try {
        await refreshAutomaticSelections();
      } catch (error) {
        notice(t("ui.operation_failed.c4e6ed"), "error", String(error));
      }
    }
    if (await persist({ ...current, step, done })) {
      await tick();
      if (!done) {
        stepScroll.scrollTop = 0;
        heading.focus({ preventScroll: true });
      }
    }
  }

  onMount(() => {
    const resume = $state.snapshot(draft);
    void refreshModules().then(() => {
      for (const [index, feature] of features.entries()) {
        const model = resume.models[index];
        if (model) void queue(model, feature);
      }
    }).catch(error => notice(t("assets.failed"), "error", String(error)));
  });
</script>

<section class="model-setup" aria-labelledby="model-setup-heading">
  <div class="model-setup-scroll" bind:this={stepScroll}>
    <div class:setup-appearance={draft.step === 0}>
      <header class="page-head">
        <h1 id="model-setup-heading" bind:this={heading} tabindex="-1">
          {t(draft.step === 0 ? "setup.appearance" : draft.step === 10 ? "setup.keyboard" : labels[index])}
        </h1>
      </header>
      <div class="model-setup-content">
        {#if draft.step === 0}
          <p id="setup-theme-description">{t("setup.choose_theme")}</p>
          <div class="setup-themes" role="group" aria-labelledby="setup-theme-description">
            {#each themes as option}
              <button type="button" aria-pressed={theme === option} disabled={busy}
                onclick={() => persist({ ...$state.snapshot(draft), theme: option })}>
                <span class="setup-theme-preview" data-theme-preview={option} aria-hidden="true">
                  <span></span><span></span>
                </span>
                <span>{t(`setup.theme.${option}`)}</span>
              </button>
            {/each}
          </div>
        {:else if draft.step === 10}
          <General setupShortcuts />
        {:else if configuring}
          {#if index === 0}<SttModelSetup />
          {:else}<ReaderModelSetup feature={index === 1 ? "ocr" : "tts"} />{/if}
        {:else}
          <p>{t("setup.choose_model")}</p>
          <Models setup={{ feature: features[index], selected: draft.models[index], choose, busy }} />
        {/if}
      </div>
    </div>
  </div>
  <div class="model-setup-bottom">
    {@render feedback()}
    <footer class="model-setup-actions">
      {#if draft.step > 0}<button type="button" disabled={busy} onclick={() => advance(-1)}>{t("setup.back")}</button>{/if}
      {#if canSkip}
        <button type="button" disabled={busy} onclick={() => advance(1)}>{t("setup.configure")}</button>
      {:else if draft.step === 10 && ui.dirty}
        <button type="button" disabled={busy} onclick={applyDraft}>{t("setup.apply_shortcuts")}</button>
      {/if}
      <button type="button" class="primary" disabled={busy} onclick={() => advance(1, canSkip)}>
        {t(draft.step === 10 ? "setup.finish" : canSkip ? "setup.skip" : "setup.next")}
      </button>
    </footer>
  </div>
</section>
