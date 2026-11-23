<script lang="ts" module>
  export interface SetupDraft {
    done: boolean;
    step: number;
    skip_reader?: boolean;
    runtimes: (string | null)[];
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
  const index = $derived(Math.min(2, Math.max(0, Math.floor((draft.step - 1) / 3))));
  const phase = $derived((draft.step - 1) % 3);
  const kind = $derived(phase === 0 ? "runtime" : "model");
  const selectedRuntime = $derived(modules.runtimes.find(entry => entry.id === draft.runtimes[index]));
  const reconciledSpeechRuntime = $derived(modules.runtimes.find(entry =>
    entry.feature === "tts" && entry.asset === selectedRuntime?.asset));
  const sharedRuntime = $derived(draft.step === 4);
  const separateSpeechRuntime = $derived(draft.runtimes[1] === "paddle-webgpu" && draft.runtimes[2] !== "sherpa-webgpu");
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

  async function queue(id: string, kind: "runtime" | "model", feature: Feature) {
    const entry = (kind === "runtime" ? modules.runtimes : modules.models)
      .find(entry => entry.id === id && entry.feature === feature);
    if (!entry || entry.installed) return;
    try {
      const managedAsset = kind === "runtime"
        ? modules.runtimes.find(runtime => runtime.id === id && runtime.feature === feature)?.action_asset ?? entry.asset
        : entry.asset;
      if (managedAsset) await api("/api/asset-action", { action: "install", id: managedAsset });
      else if (kind === "model" && ui.state?.service_online) await download(id);
      await refreshModules();
    } catch (error) {
      notice(t("assets.failed"), "error", String(error));
    }
  }

  async function choose(id: string) {
    const current = $state.snapshot(draft);
    const feature = features[index];
    const selection = kind;
    if (selection === "runtime") {
      current.runtimes[index] = id;
      if (sharedRuntime) {
        const item = modules.runtimes.find(entry => entry.id === id);
        for (const i of [1, 2]) {
          current.runtimes[i] = modules.runtimes.find(entry =>
            entry.feature === features[i] && entry.asset === item?.asset)?.id ?? null;
        }
      }
    }
    else {
      if (feature === "tts" && current.models[index] !== id) current.voice = null;
      current.models[index] = id;
    }
    if (await persist(current)) void queue(id, selection, feature);
  }

  async function cancelRuntime(id: string) {
    const feature = features[index];
    const runtime = modules.runtimes.find(entry => entry.id === id && entry.feature === feature);
    const asset = runtime?.action_asset ?? runtime?.asset;
    if (!asset) return;
    try {
      await api("/api/asset-action", { action: "cancel", id: asset });
      const fallback = modules.runtimes.find(entry => entry.feature === feature && entry.default);
      if (draft.runtimes[index] === id && fallback) await choose(fallback.id);
      await refreshModules();
    } catch (error) {
      notice(t("assets.failed"), "error", String(error));
    }
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

  async function advance(direction: number, configure = false) {
    let step = draft.step + direction;
    const current = $state.snapshot(draft);
    if (draft.step === 4 && direction > 0 && current.runtimes[1] && !current.runtimes[2]) {
      current.runtimes[2] = reconciledSpeechRuntime?.id ?? null;
    }
    if (draft.step === 4 && direction > 0) {
      current.skip_reader = !configure && !current.runtimes[1];
    }
    if (configure) step = draft.step === 4 ? 6 : draft.step + 2;
    else if (draft.step === 4 && direction > 0 && !current.runtimes[1]) step = 10;
    else if (draft.step === 6 && direction > 0) step = current.runtimes[2] ? 8 : separateSpeechRuntime ? 7 : 9;
    else if (draft.step === 8 && direction < 0) step = separateSpeechRuntime ? 7 : 6;
    else if (draft.step === 9 && direction < 0 && !current.runtimes[2]) step = separateSpeechRuntime ? 7 : 6;
    else if (draft.step === 10 && direction < 0 && current.skip_reader) step = 4;
    else if (direction > 0 && draft.step > 0 && draft.step < 10 && phase === 0 && !draft.runtimes[index]) step += 2;
    if (direction < 0 && step > 0 && step % 3 === 2 && !draft.runtimes[Math.floor((step - 1) / 3)]) step--;
    if ((phase === 2 || draft.step === 10) && !(await applyDraft())) return;
    const done = step > 10;
    if (step === 10 || (step > 0 && step % 3 === 0)) {
      try {
        await refreshAutomaticSelections();
      } catch (error) {
        notice(t("ui.operation_failed.c4e6ed"), "error", String(error));
      }
    }
    if (await persist({ ...current, step: Math.min(step, 10), done })) {
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
        const runtime = resume.runtimes[index];
        const model = resume.models[index];
        if (runtime) void queue(runtime, "runtime", feature);
        if (model) void queue(model, "model", feature);
      }
    }).catch(error => notice(t("assets.failed"), "error", String(error)));
  });
</script>

<section class="model-setup" aria-labelledby="model-setup-heading">
  <div class="model-setup-scroll" bind:this={stepScroll}>
    <div class:setup-appearance={draft.step === 0}>
      <header class="page-head">
        <h1 id="model-setup-heading" bind:this={heading} tabindex="-1">
          {t(draft.step === 0 ? "setup.appearance" : draft.step === 10 ? "setup.keyboard" : sharedRuntime ? "setup.reader_runtime" : labels[index])}
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
        {:else if phase === 2}
          {#if index === 0}<SttModelSetup />
          {:else}<ReaderModelSetup feature={index === 1 ? "ocr" : "tts"} />{/if}
        {:else}
          <p>{t(sharedRuntime ? "setup.shared_runtime_help" : kind === "runtime" ? "setup.choose_runtime" : "setup.choose_model")}</p>
          <Models setup={{ feature: features[index], kind, engine: kind === "model" ? selectedRuntime?.engine : undefined,
            selected: (kind === "runtime" ? draft.runtimes : draft.models)[index], choose, cancelRuntime, busy }} />
        {/if}
      </div>
    </div>
  </div>
  <div class="model-setup-bottom">
    {@render feedback()}
    <footer class="model-setup-actions">
      {#if draft.step > 0}<button type="button" disabled={busy} onclick={() => advance(-1)}>{t("setup.back")}</button>{/if}
      {#if draft.step > 0 && draft.step < 10 && phase === 0 && !draft.runtimes[index]}
        <button type="button" disabled={busy} onclick={() => advance(1, true)}>{t("setup.configure")}</button>
      {:else if draft.step === 10 && ui.dirty}
        <button type="button" disabled={busy} onclick={applyDraft}>{t("setup.apply_shortcuts")}</button>
      {/if}
      <button type="button" class="primary" disabled={busy} onclick={() => advance(1)}>
        {t(draft.step === 10 ? "setup.finish"
          : draft.step > 0 && phase === 0 && !draft.runtimes[index] ? sharedRuntime ? "setup.skip_both" : "setup.skip" : "setup.next")}
      </button>
    </footer>
  </div>
</section>
