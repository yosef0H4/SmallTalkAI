<script lang="ts">
  import {reading,loadReading} from "../lib/reading-state.svelte";
  import {onMount} from "svelte";
  import ReaderAdmin from "../components/ReaderAdmin.svelte";
  import Config from "./Config.svelte";
  onMount(()=>{void loadReading().catch(()=>{});});
  import { t } from "../lib/i18n";
  import { ui, notice, run } from "../lib/state.svelte";
  import { fields } from "../lib/settings";
  import Group from "../components/Group.svelte";
  import Field from "../components/Field.svelte";
  import RangeInput from "../components/RangeInput.svelte";
  import { api } from "../lib/api";
  type Agent = { id: string; name: string; detected: boolean; installed: boolean };
  let agents = $state<Agent[]>([]);
  let installing = $state("");
  let markdown = $state("");
  let preview = $state<HTMLTextAreaElement>();
  let saving = $state(false);
  const skillExport = "/api/agent-skills/export";
  let resetDialog: HTMLDialogElement;
  let resetError = $state("");
  let resetting = $state(false);
  function confirmReset() {
    resetError = "";
    resetDialog.showModal();
  }
  async function factoryReset() {
    if (resetting) return;
    resetError = "";
    resetting = true;
    try {
      await api("/api/factory-reset", { confirm: true });
      window.location.reload();
    } catch (error) {
      resetError = error instanceof Error ? error.message : String(error);
      resetting = false;
    }
  }
  onMount(() => { void api<Agent[]>("/api/agent-skills").then((items) => agents = items).catch(() => {}); });
  async function install(agent: Agent) {
    installing = agent.id;
    await run(async () => {
      await api("/api/agent-skills", agent.id);
      agent.installed = true;
    });
    installing = "";
  }
  async function copySkill() {
    try {
      await navigator.clipboard.writeText(markdown);
    } catch {
      preview?.focus();
      preview?.select();
      if (!document.execCommand("copy")) {
        notice(t("agent_skills.copy_failed"), "error");
        return;
      }
    }
    notice(t("agent_skills.copied"));
  }
  async function saveSkill() {
    saving = true;
    await run(async () => {
      const path = await api<string>(skillExport, {});
      notice(t("agent_skills.saved", { path }));
    });
    saving = false;
  }
</script>

<Group title="settings.memory" icon="advanced" paths={["speech.idle_worker_timeout_secs","speech.worker_shutdown_grace_ms"]}>
 <Field spec={fields.idle}/><Field spec={fields.grace}/>
 {#if reading.config}
 {@const { playback, ocr } = reading.config}
 <div class="field"><label for="reading-idle">{t("reading.idle_timeout")}</label><div class="control">
   <RangeInput id="reading-idle" min={0} max={3600} value={playback.keep_warm_seconds} oninput={(value) => playback.keep_warm_seconds = value} />
   <output class="range-value" for="reading-idle">{playback.keep_warm_seconds}s</output>
 </div></div>
 <div class="field"><label for="ocr-idle">{t("module.ocr_idle")}</label><div class="control">
   <RangeInput id="ocr-idle" min={0} max={3600} value={ocr.idle_timeout_secs} oninput={(value) => ocr.idle_timeout_secs = value} />
   <output class="range-value" for="ocr-idle">{ocr.idle_timeout_secs}s</output>
 </div></div>
 {/if}
</Group>
<ReaderAdmin/>
{#if agents.length}
<details class="guide" ontoggle={(event) => {
  if (event.currentTarget.open && !markdown)
    void run(async () => { markdown = await api<string>(skillExport); });
}}>
  <summary>{t("agent_skills.title")}</summary>
  {#each agents as agent (agent.id)}
    <div class="field"><span>{agent.name} · {t(agent.installed ? "ui.installed.7bb440" : agent.detected ? "agent_skills.detected" : "agent_skills.undetected")}</span>
      <div class="control"><button type="button" disabled={!!installing} onclick={() => install(agent)}>{t(agent.installed ? "agent_skills.update" : "agent_skills.install")}</button></div>
    </div>
  {/each}
  {#if markdown}
    <div class="skill-export">
      <label for="skill-markdown" class="json-label">SKILL.md</label>
      <textarea id="skill-markdown" bind:this={preview} dir="ltr" rows="9"
        value={markdown} readonly spellcheck="false"></textarea>
      <div class="control skill-actions">
        <button type="button" onclick={copySkill}>{t("agent_skills.copy")}</button>
        <button type="button" disabled={saving} onclick={saveSkill}>{t("agent_skills.save")}</button>
      </div>
    </div>
  {/if}
</details>
{/if}
<Group
  title="ui.diagnostics.3af227"
  icon="diagnostics"
  paths={[
    "diagnostics.log_level",
    "diagnostics.diagnostic_overlay",
    "diagnostics.log_transcripts",
    ...(ui.state?.platform === "linux"
      ? ["diagnostics.clipboard_buffer_notices"]
      : []),
  ]}
  >{#each ["logs", "overlay", "transcripts"] as key}<Field
      spec={fields[key]}
    />{/each}{#if ui.state?.platform === "linux"}<Field
      spec={fields.clipboardBufferNotices}
    />{/if}</Group
>

<details class="guide" data-settings-json><summary>{t("settings.configuration")}</summary><Config/></details>
<div class="reset-setting">
  <button type="button" onclick={confirmReset}>{t("system.factory_reset")}</button>
</div>
<dialog bind:this={resetDialog} class="reset-confirmation" aria-labelledby="reset-title" aria-describedby="reset-description"
  oncancel={event => { if (resetting) event.preventDefault(); }}>
  <h2 id="reset-title">{t("system.factory_reset")}</h2>
  <div class="reset-content">
    <p id="reset-description">{t("system.factory_reset_confirm")}</p>
    {#if resetError}<p class="reset-error" role="alert">{resetError}</p>{/if}
    <div class="reset-actions" aria-busy={resetting}>
      <button type="button" disabled={resetting} onclick={() => resetDialog.close()}>{t("assets.cancel")}</button>
      <button type="button" class="reset-destructive" disabled={resetting} onclick={factoryReset}>
        {t(resetting ? "system.factory_reset_busy" : "system.factory_reset_apply")}
      </button>
    </div>
  </div>
</dialog>

<style>
  .reset-setting { margin-block-start: 12px; display: flex; }
  .reset-confirmation {
    inline-size: min(412px, calc(100% - 24px));
    max-block-size: calc(100% - 24px);
    background: var(--paper);
    padding: 3px;
    box-shadow: var(--raised-edge), 0 6px 18px #0003;
    text-align: start;
  }
  .reset-confirmation h2 {
    margin: 0;
    padding: 5px 8px;
    background: var(--title-bar);
    color: var(--title-ink);
    font-size: 14px;
  }
  .reset-content { padding: 12px; }
  .reset-content p { margin-block: 0 12px; line-height: 1.45; overflow-wrap: anywhere; }
  .reset-error, .reset-destructive { color: var(--danger); }
  .reset-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }
</style>
