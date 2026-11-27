<script lang="ts">
  import {
    ui,
    set,
    markDirty,
    notice,
  } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import { fields, deliveryOptions } from "../lib/settings";
  import type { DeliveryMode } from "../lib/types";
  import Group from "../components/Group.svelte";
  import Field from "../components/Field.svelte";
  import Combo from "../components/Combo.svelte";
  import NumberInput from "../components/NumberInput.svelte";
  import IconButton from "../components/IconButton.svelte";
  import Help from "../components/Help.svelte";
  import RangeInput from "../components/RangeInput.svelte";
  import { t } from "../lib/i18n";
  import { positionToWpm, wpmToPosition } from "../lib/typing";
  let query = $state("");
  const typingPosition = $derived(ui.config?.output.paced_typing_enabled === false
    ? 100 : wpmToPosition(ui.config?.output.typing_speed_wpm ?? 450));
  const typingLabel = $derived(typingPosition === 100
    ? t("typing.instant") : `${ui.config?.output.typing_speed_wpm ?? 450}${t("ui.wpm.3fc2c6")}`);
  function setTypingPosition(position: number) {
    if (position < 100) set("output.typing_speed_wpm", positionToWpm(position));
    set("output.paced_typing_enabled", position < 100);
  }
  const linux = $derived(ui.state?.platform === "linux");
  const actionOrder: DeliveryMode[] = [
    "type", "smart_paste", "paste_ctrl_v", "paste_ctrl_shift_v",
    "paste_shift_insert", "clipboard",
  ];
  const choices = $derived(
    deliveryOptions
      .slice()
      .sort((a, b) => actionOrder.indexOf(a[0]) - actionOrder.indexOf(b[0]))
      .map(([mode, label]) => ({ mode, label, key: mode })),
  );
  const shown = $derived(
    choices.filter((c) =>
      `${t(c.label)} ${c.label}`.toLowerCase().includes(query.toLowerCase()),
    ),
  );
  function select(value: string) {
    set("output.delivery_mode", value as DeliveryMode);
  }
  function included(mode: DeliveryMode) {
    return ui.config?.output.enabled_delivery_modes.includes(mode);
  }
  function toggle(mode: DeliveryMode) {
    if (!ui.config) return;
    const modes = ui.config.output.enabled_delivery_modes;
    if (modes.includes(mode)) {
      if (modes.length === 1)
        return notice(t("ui.keep_at_least_one_delivery.2d7d41"), "error");
      set("output.enabled_delivery_modes", modes.filter((entry) => entry !== mode));
    } else {
      set("output.enabled_delivery_modes", [...modes, mode]);
    }
  }
  const ruleActions = deliveryOptions.filter(([value]) => value !== "smart_paste")
    .map(([value, label]) => ({ value, label }));
  let windows = $state<{ value: string; label: string; meta?: string; search: string }[]>([]);
  let loadingWindows = false;
  let windowListError = $state(false);
  async function loadWindows() {
    if (loadingWindows) return;
    loadingWindows = true;
    windowListError = false;
    try {
      const entries = await api<{ app_id: string; title: string }[]>("/api/desktop-windows");
      const apps = new Map<string, (typeof windows)[number]>();
      for (const { app_id, title } of entries) {
        const key = app_id.toLowerCase();
        const app = apps.get(key);
        if (app) app.search += ` ${title}`;
        else apps.set(key, { value: app_id, label: app_id, meta: title || undefined, search: title });
      }
      windows = [...apps.values()].sort((a, b) => a.label.localeCompare(b.label));
    } catch {
      windowListError = true;
    } finally {
      loadingWindows = false;
    }
  }
  function addRule(appId: string) {
    const rules = ui.config?.output.smart_paste_rules;
    const app_id = appId.trim();
    if (!rules || !app_id || rules.some(rule => rule.app_id.toLowerCase() === app_id.toLowerCase())) return;
    rules.unshift({ app_id, mode: "paste_ctrl_v" });
    markDirty();
  }
</script>

<Group
  title="ui.delivery.9631af"
  icon="output"
  paths={[
    "output.delivery_mode",
    "output.enabled_delivery_modes",
    "output.preserve_clipboard",
  ]}
>
  <Combo
    label="ui.use_now.36690a"
    path="output.delivery_mode"
    ariaLabel={t(linux ? "Current Linux delivery method" : "Current Windows delivery method")}
    value={`${ui.config?.output.delivery_mode}`}
    items={choices.map((c) => ({
      value: c.key,
      label: t(c.label),
    }))}
    onselect={select}
  />
  <Field spec={fields.preserveClipboard} />
  <details class="delivery-picker-advanced">
    <summary>{t("ui.cycle_between.75667a")}</summary>
    <div data-setting-path="output.enabled_delivery_modes">
      <input
        type="search"
        placeholder={t("ui.search_tools_and_delivery_methods.717e7b")}
        aria-label={t("ui.search_tools_and_delivery_methods.717e7b")}
        bind:value={query}
      />{#each shown as c}<label class="cycle-row"
          ><span
            >{t(c.label)}</span
          ><input
            type="checkbox"
            aria-label={t("ui.include_method_in_delivery_cycle.d45eee", { method: t(c.label) })}
            checked={included(c.mode)}
            onchange={() => toggle(c.mode)}
          /></label
        >{/each}
    </div>
  </details>
</Group>
<Group
  title="ui.typing.5614fd"
  icon="keyboard"
  paths={["output.paced_typing_enabled", "output.typing_speed_wpm"]}
  ><div data-setting-path="output.paced_typing_enabled">
    <div class="field" data-setting-path="output.typing_speed_wpm">
      <div class="field-copy"><label for="typing-speed">{t("ui.speed.2d2cb0")}</label><Help text={t("typing.speed_help")} /></div>
      <div class="control">
        <RangeInput id="typing-speed" min={0} max={100} valueText={typingLabel} value={typingPosition} oninput={setTypingPosition} />
        <NumberInput label={t("settings.typing_exact")} min={50} max={1650}
          value={typingPosition===100 ? "" : ui.config?.output.typing_speed_wpm ?? ""} placeholder={t("typing.instant")}
          oninput={value => { set("output.typing_speed_wpm", value); set("output.paced_typing_enabled", true); }} />
        <output class="range-value" for="typing-speed">{typingPosition===100?t("typing.instant"):t("ui.wpm.3fc2c6")}</output>
      </div>
    </div>
  </div></Group
>
<Group title="settings.smart_paste_rules" icon="apps" paths={["output.smart_paste_rules"]}>
  <p class="muted">{t("settings.smart_paste_help")}</p>
  <Combo custom label="settings.find_or_enter_app" path="output.find_paste_app" value=""
    items={windows} onopen={loadWindows} onselect={addRule} />
  {#if windowListError}<p class="muted" role="status">{t("settings.window_list_unavailable")}</p>{/if}
  <div data-setting-path="output.smart_paste_rules">
    {#each ui.config?.output.smart_paste_rules || [] as entry, index}<div
        class="app-override-row"
      >
        <input
          aria-label={t("ui.application_identity.74e9a6")} maxlength="160"
          value={entry.app_id}
          placeholder={t("ui.application_id.557031")}
          oninput={(e) => {
            entry.app_id = e.currentTarget.value;
            markDirty();
          }}
        /><Combo selectOnly inline path={`output.smart_paste_rules.${index}.mode`}
          label="ui.delivery_mode_for_app.763ef6"
          ariaLabel={t("ui.delivery_mode_for_app.763ef6", { app: entry.app_id || t("ui.application.d2005c") })}
          value={entry.mode}
          items={ruleActions}
          onselect={(value) => {
            entry.mode = value as DeliveryMode;
            markDirty();
          }}
        /><IconButton
          icon="trash"
          label="ui.remove_app_override.10d229"
          onclick={() => {
            ui.config?.output.smart_paste_rules.splice(index, 1);
            markDirty();
          }}
        />
      </div>{:else}<p class="empty">{t("ui.no_overrides.473cfd")}</p>{/each}
  </div>
</Group>
<Group
  title="ui.text.c3328c"
  icon="text"
  paths={[
    "output.trailing_space",
    "output.remove_punctuation",
    "output.lowercase",
  ]}
  >{#each ["space", "punctuation", "lowercase"] as key}<Field
      spec={fields[key]}
    />{/each}</Group
>
