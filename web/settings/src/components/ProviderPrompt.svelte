<script lang="ts">
  import defaults from "../lib/provider-prompts.json";
  import { t } from "../lib/i18n";
  import Help from "./Help.svelte";

  let { feature, path, value = "", onchange }: {
    feature: keyof typeof defaults;
    path: string;
    value?: string;
    onchange: (value: string) => void;
  } = $props();
</script>

<div class="field" data-setting-path={path}>
  <div class="field-copy">
    <label for={path}>{t(`provider.${feature}_prompt`)}</label>
    <Help text={`provider.${feature}_prompt_help`} />
  </div>
  <div class="control">
    <textarea id={path} rows="3" dir="auto" maxlength="4096" {value}
      placeholder={defaults[feature]} oninput={event => onchange(event.currentTarget.value)}></textarea>
  </div>
</div>
