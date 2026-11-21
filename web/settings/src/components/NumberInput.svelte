<script lang="ts">
  import { t } from "../lib/i18n";

  let { id, label, value, min, max, step = 1, disabled = false, placeholder, oninput }: {
    id?: string;
    label: string;
    value: number | string;
    min?: number;
    max?: number;
    step?: number;
    disabled?: boolean;
    placeholder?: string;
    oninput: (value: number) => void;
  } = $props();

  let input: HTMLInputElement;
  const columns = $derived(Math.max(3, String(min ?? "").length, String(max ?? "").length));

  function change() {
    if (Number.isFinite(input.valueAsNumber)) oninput(input.valueAsNumber);
  }

  function nudge(direction: number) {
    if (Number.isFinite(input.valueAsNumber)) input.stepUp(direction);
    else input.value = String(min ?? Math.min(0, max ?? 0));
    change();
  }
</script>

<div class="number-field" dir="ltr" style:--number-columns={`${columns}ch`}>
  <input bind:this={input} {id} type="number" aria-label={label} class="range-number"
    {value} {min} {max} {step} {disabled} oninput={change}
    placeholder={placeholder ? "–" : undefined} aria-valuetext={value === "" ? placeholder : undefined} />
  <div class="number-arrows">
    <button type="button" aria-label={t("settings.number_increase", { field: label })}
      disabled={disabled || (max !== undefined && value !== "" && Number(value) >= max)}
      onclick={() => nudge(1)}></button>
    <button type="button" aria-label={t("settings.number_decrease", { field: label })}
      disabled={disabled || (min !== undefined && value !== "" && Number(value) <= min)}
      onclick={() => nudge(-1)}></button>
  </div>
</div>
