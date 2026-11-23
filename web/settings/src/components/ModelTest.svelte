<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n";
  import { modules, type Binding } from "../lib/modules.svelte";
  import { reading } from "../lib/reading-state.svelte";
  import { notice, save, ui } from "../lib/state.svelte";

  let { feature, binding, language }: {
    feature: "stt" | "tts" | "ocr";
    binding: Binding;
    language?: string;
  } = $props();
  let busy = $state(false);
  let session = $state<number | null>(null);
  let transcript = $state("");
  let waveform = $state("");
  let previewError = $state("");
  let mounted = false;
  const transcriptId = $props.id();
  const runtime = $derived(modules.runtimes.find(row => row.id === binding.runtime));
  const model = $derived(modules.models.find(row => row.id === binding.model));
  const voice = $derived(binding.voice ?? (binding.model === "supertonic3" ? "sid:0" : "af_heart"));
  const ocrChanged = $derived(feature === "ocr" && reading.config && reading.baseline
    && JSON.stringify(reading.config.ocr) !== JSON.stringify(JSON.parse(reading.baseline).ocr));
  async function test() {
    busy = true;
    try {
      if (feature === "ocr") {
        if (ocrChanged) {
          await save();
          if (ocrChanged) return;
        }
        await api("/api/reader-action", { action: "copy_region" });
      }
      else {
        const code = language || model?.languages[0] || "en";
        await api("/api/preview-voice", {
          binding: { ...binding, voice }, language: code,
          text: t(code.startsWith("ar") ? "setup.sample.ar" : "setup.sample.en"),
        });
      }
    } catch (error) {
      notice(t("reading.failed"), "error", String(error));
    } finally {
      busy = false;
    }
  }

  const preview = (action: string, session_id: number, keepalive = false) =>
    api<{ text: string }>("/api/dictation-preview",
      { action, session_id, language: language || "single" }, undefined, keepalive);
  function cancelPreview() {
    if (session !== null) void preview("cancel", session, true).catch(() => {});
  }
  onMount(() => {
    if (feature !== "stt") return;
    mounted = true;
    window.addEventListener("pagehide", cancelPreview);
    return () => {
      mounted = false;
      cancelPreview();
      window.removeEventListener("pagehide", cancelPreview);
    };
  });

  async function record() {
    busy = true;
    previewError = "";
    waveform = "";
    try {
      if (session !== null) {
        transcript = (await preview("stop", session)).text;
        session = null;
      } else {
        if (ui.dirty) {
          await save();
          if (ui.dirty || !mounted) return;
        }
        transcript = "";
        const id = crypto.getRandomValues(new Uint32Array(1))[0] + 2 ** 52;
        await preview("start", id);
        if (!mounted) await preview("cancel", id, true);
        else session = id;
      }
    } catch (error) {
      session = null;
      previewError = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    if (session === null || busy) return;
    const sessionId = session;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    async function readWaveform() {
      try {
        const result = await api<{ waveform: string }>("/api/dictation-preview", {
          action: "waveform", session_id: sessionId,
        }, controller.signal);
        if (!controller.signal.aborted) {
          waveform = result.waveform;
          timer = setTimeout(readWaveform, 80);
        }
      } catch {
        if (!controller.signal.aborted) waveform = "";
      }
    }
    void readWaveform();
    return () => { controller.abort(); clearTimeout(timer); };
  });
</script>

{#if runtime?.installed && model?.installed}
  {#if feature === "stt"}
    <div class="dictation-preview">
      <div class="reading-actions">
        <button type="button" disabled={busy || !ui.state?.service_online} onclick={record}>
          {t(session !== null ? "setup.stop_recording" : ui.dirty ? "setup.save_record" : "ui.record.1c5413")}
        </button>
        {#if session !== null && !busy && waveform}
          <span class="preview-waveform" dir="ltr" aria-hidden="true">{waveform}</span>
        {/if}
        <span role="status">{busy && session !== null ? t("setup.processing_recording") : previewError}</span>
      </div>
      <label for={transcriptId}>{t("setup.transcript")}</label>
      <textarea id={transcriptId} rows="3" readonly value={transcript} dir="auto"></textarea>
    </div>
  {:else}
    <div class="reading-actions">
      <button type="button" disabled={busy} aria-busy={busy} onclick={test}>
        {t(feature === "tts" ? "setup.test_voice" : ocrChanged ? "setup.save_copy" : "reading.copy_region")}
      </button>
    </div>
  {/if}
{/if}

<style>
  .preview-waveform {
    align-self: center;
    color: var(--accent);
    font: 16px/1 ui-monospace, "Cascadia Mono", Consolas, monospace;
    letter-spacing: 1px;
    white-space: nowrap;
  }
</style>
