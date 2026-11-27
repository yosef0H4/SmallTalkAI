<script lang="ts">
  import type { ComboItem } from "../lib/types";
  import { matchesSearch } from "../lib/search";
  import Icon from "./Icon.svelte";
  import { tick, type Snippet } from "svelte";
  import Help from "./Help.svelte";
  import { t } from "../lib/i18n";
  let {
    label,
    ariaLabel,
    path,
    id: inputId,
    value,
    selectedLabel,
    items,
    onselect,
    onopen,
    custom = false,
    disabled = false,
    unavailable = false,
    help = "",
    searchMatcher,
    extra,
    selectOnly = false,
    inline = false,
  }: {
    label: string;
    ariaLabel?: string;
    path: string;
    id?: string;
    value: string;
    selectedLabel?: string;
    items: (ComboItem & { search?: string })[];
    onselect: (value: string) => void;
    onopen?: () => void;
    custom?: boolean;
    disabled?: boolean;
    unavailable?: boolean;
    help?: string;
    searchMatcher?: (query: string, item: ComboItem & { search?: string }) => boolean;
    extra?: Snippet;
    selectOnly?: boolean;
    inline?: boolean;
  } = $props();
  const id = $derived(inputId ?? `combo-${path}`);
  let open = $state(false),
    query = $state(""),
    active = $state(-1),
    input: HTMLInputElement,
    root: HTMLDivElement;
  const selected = $derived(
    selectedLabel ?? (items.find((i) => i.value === value)?.label ||
      (unavailable ? `${t("ui.unavailable.2c9c1f")} · ${value}` : value)),
  );
  const filtered = $derived(
    items.filter((i) =>
      searchMatcher ? searchMatcher(query, i) : matchesSearch(query, t(i.label), i.label, i.value, i.meta, i.search),
    ),
  );
  function expand() {
    if (disabled) return;
    open = true;
    query = "";
    active = selectOnly ? items.findIndex(item => item.value === value) : -1;
    onopen?.();
  }
  function anchorList(list: HTMLDivElement) {
    const pane = input.closest("main") ?? document.documentElement;
    const gap = 4;
    const maxHeight = 280;
    let frame = 0;

    function update() {
      frame = 0;
      const field = input.getBoundingClientRect();
      const { top, bottom } = pane.getBoundingClientRect();
      const above = field.top - top - gap;
      const below = bottom - field.bottom - gap;

      if (field.bottom <= top || field.top >= bottom || Math.max(above, below) <= 0) {
        close();
        return;
      }

      list.style.left = `${field.left}px`;
      list.style.width = `${field.width}px`;
      const fullHeight = list.scrollHeight + list.offsetHeight - list.clientHeight;
      const upward = below < Math.min(maxHeight, fullHeight) && above > below;
      list.style.top = upward ? "auto" : `${field.bottom + gap}px`;
      list.style.bottom = upward ? `${window.innerHeight - field.top + gap}px` : "auto";
      list.style.maxHeight = `${Math.min(maxHeight, upward ? above : below)}px`;
    }

    function scheduleUpdate() {
      if (!frame) frame = requestAnimationFrame(update);
    }

    function onScroll(event: Event) {
      if (event.target === pane || event.target === document) scheduleUpdate();
    }

    const observer = new ResizeObserver(scheduleUpdate);
    observer.observe(input);
    observer.observe(pane);
    document.addEventListener("scroll", onScroll, { capture: true, passive: true });
    update();

    return {
      destroy() {
        observer.disconnect();
        document.removeEventListener("scroll", onScroll, true);
        cancelAnimationFrame(frame);
      },
    };
  }
  function choose(item: ComboItem) {
    onselect(item.value);
    open = false;
    query = "";
    input.focus();
  }
  function close() {
    if (custom && query.trim()) onselect(query.trim());
    open = false;
    query = "";
    active = -1;
  }
  async function key(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      open = false;
      query = "";
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) expand();
      active = Math.max(
        0,
        Math.min(
          filtered.length - 1,
          active + (event.key === "ArrowDown" ? 1 : -1),
        ),
      );
      await tick();
      document
        .getElementById(`option-${path}-${active}`)
        ?.scrollIntoView({ block: "nearest" });
    } else if (event.key === "Enter" || (selectOnly && event.key === " ")) {
      event.preventDefault();
      if (selectOnly && !open) expand();
      else if (active >= 0 && filtered[active]) choose(filtered[active]);
      else if (custom) close();
      else if (filtered.length === 1) choose(filtered[0]);
    }
  }
</script>

<svelte:document
  onpointerdown={(event) => {
    if (
      open &&
      !root.contains(event.target as Node) &&
      !(event.target as Element).closest?.(`[data-combo-list="${path}"]`)
    )
      close();
  }}
/>
<div class={inline ? "inline-combo" : "field"} data-setting-path={path} bind:this={root}>
  {#if !inline}
  <div class="field-copy">
    <label for={id}>{t(label)}</label>{#if help}<Help
        text={t(help)}
      />{/if}
  </div>
  {/if}
  <div class="control assignment-control">
    <div class="combo model-combo">
      <div class="combo-shell">
        <input
          bind:this={input}
          {id}
          class="combo-input"
          {disabled}
          readonly={selectOnly}
          type="text"
          dir="auto"
          role="combobox"
          aria-label={t(ariaLabel || label)}
          aria-autocomplete={selectOnly ? undefined : "list"}
          aria-expanded={open}
          aria-controls={`list-${path}`}
          aria-activedescendant={open && active >= 0
            ? `option-${path}-${active}`
            : undefined}
          autocomplete="off"
          value={open && !selectOnly ? query : t(selected)}
          data-value={value}
          onfocus={() => { if (!selectOnly) expand(); }}
          onclick={() => {
            if (!open) expand();
            else if (selectOnly) close();
          }}
          oninput={(event) => {
            query = event.currentTarget.value;
            open = true;
            active = -1;
          }}
          onkeydown={key}
          onblur={(event) => {
            if (
              !(event.relatedTarget as HTMLElement)?.closest(
                `[data-combo-list="${path}"]`,
              )
            )
              close();
          }}
        /><Icon name="chevron" size={14} />
      </div>
    </div>
    {#if extra}{@render extra()}{/if}
  </div>
</div>
{#if open}<div
    class="combo-list"
    use:anchorList
    id={`list-${path}`}
    data-combo-list={path}
    role="listbox"
    aria-label={`${t(label)} ${t("ui.choices.153eb4")}`}
  >
    {#each filtered as item, i (item.value)}<button
        type="button"
        tabindex="-1"
        id={`option-${path}-${i}`}
        role="option"
        aria-label={t(item.label)}
        aria-describedby={item.meta ? `meta-${path}-${i}` : undefined}
        aria-selected={value === item.value}
        data-value={item.value}
        class:highlighted={active === i}
        onpointerdown={(event) => event.preventDefault()}
        onclick={() => choose(item)}
        ><span class="option-copy"><span>{t(item.label)}</span>{#if item.meta}<small id={`meta-${path}-${i}`} dir="auto">{item.meta}</small>{/if}</span>{#if item.value === value}<Icon
            name="check"
            size={15}
          />{/if}</button
      >{:else}<span class="empty">{t("ui.no_matches.cd0af6")}</span>{/each}
  </div>{/if}
<style>
  .option-copy { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .option-copy small { color: var(--muted); font-size: 11px; overflow-wrap: anywhere; }
</style>
