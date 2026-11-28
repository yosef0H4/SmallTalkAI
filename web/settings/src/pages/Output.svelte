<script lang="ts">
  import {
    ui,
    set,
    markDirty,
    notice,
  } from "../lib/state.svelte";
  import { matchesSearch } from "../lib/search";
  import { fields, deliveryOptions } from "../lib/settings";
  import type { DeliveryMode } from "../lib/types";
  import Group from "../components/Group.svelte";
  import Field from "../components/Field.svelte";
  import Combo from "../components/Combo.svelte";
  import NumberInput from "../components/NumberInput.svelte";
  import IconButton from "../components/IconButton.svelte";
  import Icon from "../components/Icon.svelte";
  import Help from "../components/Help.svelte";
  import RangeInput from "../components/RangeInput.svelte";
  import { t } from "../lib/i18n";
  import { MIN_TYPING_WPM, MIDDLE_TYPING_WPM, MAX_TYPING_WPM, positionToWpm, wpmToPosition } from "../lib/typing";
  let query = $state("");
  const typingPosition = $derived(ui.config?.output.paced_typing_enabled
    ? wpmToPosition(ui.config.output.typing_speed_wpm) : 100);
  const typingLabel = $derived(typingPosition === 100
    ? t("typing.instant") : `${ui.config?.output.typing_speed_wpm ?? MIDDLE_TYPING_WPM}${t("ui.wpm.3fc2c6")}`);
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
  const ruleActions = [
    ...deliveryOptions.filter(([value]) => value !== "smart_paste").reverse(),
    ["none", "ui.none.6eef66"],
  ] as const;
  const rules = $derived(ui.config?.output.smart_paste_rules ?? []);
  const pickerItems = $derived(rules.map(({ app_id, mode }) => ({
    value: app_id,
    label: app_id,
    meta: t(deliveryOptions.find(([value]) => value === mode)?.[1] ?? ""),
  })));
  function newRuleOption(appId: string) {
    if (Array.from(appId).length > 160) return;
    const key = appId.toLowerCase();
    if (rules.some(({ app_id }) => [key, `*${key}*`].includes(app_id.toLowerCase()))) return;
    return { value: appId, label: t("settings.add_paste_app", { app: appId }) };
  }
  function assignRule(app_id: string, mode: DeliveryMode | "none") {
    if (!ui.config) return;
    const index = rules.findIndex(rule => rule.app_id === app_id);
    if (mode === "none") {
      if (index < 0) return;
      rules.splice(index, 1);
    } else if (index >= 0) {
      if (rules[index].mode === mode) return;
      rules[index].mode = mode;
    } else {
      rules.unshift({ app_id, mode });
    }
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
        <NumberInput label={t("settings.typing_exact")} min={MIN_TYPING_WPM} max={MAX_TYPING_WPM}
          value={typingPosition===100 ? "" : ui.config?.output.typing_speed_wpm ?? ""} placeholder={t("typing.instant")}
          oninput={value => { set("output.typing_speed_wpm", value); set("output.paced_typing_enabled", true); }} />
        <output class="range-value" for="typing-speed">{typingPosition===100?t("typing.instant"):t("ui.wpm.3fc2c6")}</output>
      </div>
    </div>
  </div></Group
>
<Group title="settings.smart_paste_rules" icon="apps" paths={["output.smart_paste_rules"]}>
  <p class="muted">{t("settings.smart_paste_help")}</p>
  <div class="paste-groups" data-setting-path="output.smart_paste_rules">
    {#each ruleActions as [mode, label] (mode)}
      {@const entries = rules.filter(rule => rule.mode === mode)}
      {@const pickerLabel = mode === "none" ? "settings.clear_paste_rule" : "settings.find_or_enter_app"}
      <details class="paste-group" data-paste-mode={mode}>
        <summary>
          <span>{t(label)}</span>
          {#if mode !== "none"}<span class="muted">{entries.length}</span>{/if}
        </summary>
        <div class="paste-group-content">
          <Combo inline
            label={pickerLabel} placeholder={pickerLabel}
            path={`output.find_paste_app.${mode}`} value=""
            items={pickerItems} searchMatcher={(query, item) => matchesSearch(query, item.value)}
            createOption={mode === "none" ? undefined : newRuleOption}
            onselect={(app) => assignRule(app, mode)} />
          {#if entries.length}
            <ul class="paste-rule-list">
              {#each entries as entry (entry.app_id)}
                <li><span dir="auto">{entry.app_id}</span>
                  <button type="button" title={t("ui.remove_app_override.10d229")}
                    aria-label={t("settings.remove_paste_rule", { app: entry.app_id })}
                    onclick={() => assignRule(entry.app_id, "none")}><Icon name="close" size={13} /></button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      </details>
    {/each}
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
