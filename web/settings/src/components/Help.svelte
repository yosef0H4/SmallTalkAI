<script lang="ts">
  import Icon from "./Icon.svelte";
  import { t } from "../lib/i18n";
  let { text }: { text: string } = $props();
  const id = $props.id();
  let position = $state<{ left: number; width: number; top?: number; bottom?: number } | null>(null);
  function toggle(event: MouseEvent & { currentTarget: HTMLButtonElement }) {
    if (position) {
      position = null;
      return;
    }
    const button = event.currentTarget.getBoundingClientRect();
    const width = Math.min(330, innerWidth - 16);
    const left = Math.max(8, Math.min(button.left, innerWidth - width - 8));
    position = {
      left,
      width,
      ...(innerHeight - button.bottom > 128 ? { top: button.bottom + 4 } : { bottom: innerHeight - button.top + 4 }),
    };
  }
</script>

<span class="help">
  <button type="button" aria-label={t(text)} title={position ? undefined : t(text)}
    aria-expanded={!!position} aria-controls={position ? id : undefined}
    onclick={toggle} onblur={() => position = null}
    onkeydown={event => { if (event.key === "Escape") position = null; }}>
    <Icon name="help" size={15} />
  </button>
  {#if position}
    <span class="help-text" id={id} style:left={position.left + "px"} style:width={position.width + "px"}
      style:top={position.top === undefined ? undefined : position.top + "px"}
      style:bottom={position.bottom === undefined ? undefined : position.bottom + "px"}>{t(text)}</span>
  {/if}
</span>
