<script lang="ts">
  import {reading,loadReading} from "../lib/reading-state.svelte";
  import {onMount} from "svelte";
  import ReaderAdmin from "../components/ReaderAdmin.svelte";
  import Config from "./Config.svelte";
  onMount(()=>{void loadReading().catch(()=>{});});
  import { t } from "../lib/i18n";
  import { ui, action, run } from "../lib/state.svelte";
  import { fields } from "../lib/settings";
  import Group from "../components/Group.svelte";
  import Field from "../components/Field.svelte";
  import IconButton from "../components/IconButton.svelte";
  import RangeInput from "../components/RangeInput.svelte";
  import { api } from "../lib/api";
  type Agent = { id: string; name: string; detected: boolean; installed: boolean };
  let agents = $state<Agent[]>([]);
  let installing = $state("");
  onMount(() => { void api<Agent[]>("/api/agent-skills").then((items) => agents = items).catch(() => {}); });
  async function install(agent: Agent) {
    installing = agent.id;
    await run(async () => {
      await api("/api/agent-skills", agent.id);
      agent.installed = true;
    });
    installing = "";
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
<details class="guide">
  <summary>{t("agent_skills.title")}</summary>
  {#each agents as agent (agent.id)}
    <div class="field"><span>{agent.name} · {t(agent.installed ? "ui.installed.7bb440" : agent.detected ? "agent_skills.detected" : "agent_skills.undetected")}</span>
      <div class="control"><button type="button" disabled={!!installing} onclick={() => install(agent)}>{t(agent.installed ? "agent_skills.update" : "agent_skills.install")}</button></div>
    </div>
  {/each}
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
