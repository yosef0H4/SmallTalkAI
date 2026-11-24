<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "../lib/i18n";
  let { text }: { text: string } = $props();
  const id = $props.id();
  let open = $state(false);
  let popup = $state<HTMLSpanElement>();
  let position = $state<{ left: number; top: number; height: number } | null>(null);
  async function toggle(event: MouseEvent & { currentTarget: HTMLButtonElement }) {
    if (open) {
      open = false;
      return;
    }
    const button = event.currentTarget.getBoundingClientRect();
    position = null;
    open = true;
    await tick();
    if (!open || !popup) return;
    const { width, height } = popup.getBoundingClientRect();
    const above = Math.max(0, button.top - 12);
    const below = Math.max(0, innerHeight - button.bottom - 12);
    const flip = height > below && above > below;
    const maxHeight = flip ? above : below;
    const start = document.dir === "rtl" ? button.right - width : button.left;
    position = {
      left: Math.max(8, Math.min(start, innerWidth - width - 8)),
      top: flip ? button.top - Math.min(height, maxHeight) - 4 : button.bottom + 4,
      height: maxHeight,
    };
  }
</script>

<span class="help">
  <button type="button" aria-label={t(text)} title={open ? undefined : t(text)}
    aria-expanded={open} aria-controls={open ? id : undefined}
    onclick={toggle} onblur={() => open = false}
    onkeydown={event => { if (event.key === "Escape") open = false; }}>
    <Icon name="help" size={15} />
  </button>
  {#if open}
    <span class="help-text" id={id} role="tooltip" bind:this={popup} style:visibility={position ? undefined : "hidden"}
      style:left={(position?.left ?? 8) + "px"} style:top={(position?.top ?? 8) + "px"}
      style:max-height={position ? position.height + "px" : undefined}>{t(text)}</span>
  {/if}
</span>
