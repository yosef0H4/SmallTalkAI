<script lang="ts">
  import { onMount } from 'svelte';
  import { matchesCatalogSearch, moduleSearchText } from '../lib/search';
  import { modules, refreshModules, catalogView, type Feature, type ModuleModel } from '../lib/modules.svelte';
  import { ui, download, action, notice, markDirty, refreshState } from '../lib/state.svelte';
  import { reading } from '../lib/reading-state.svelte';
  import type { ReaderConfig } from '../lib/reader';
  import SttModelSetup from "../components/SttModelSetup.svelte";
  import ReaderModelSetup from "../components/ReaderModelSetup.svelte";
  import Cleanup from "./Cleanup.svelte";
  import ModelFiles from "../components/ModelFiles.svelte";
  import {loadReading} from "../lib/reading-state.svelte";
  import Group from '../components/Group.svelte';
  import Help from '../components/Help.svelte';
  import { api } from '../lib/api';
  import { t, languageName } from '../lib/i18n';
  import { runtimeHardware, runtimeLabel, recommendationLabel } from '../lib/module-labels';
  import Icon from '../components/Icon.svelte';
  import IconButton from '../components/IconButton.svelte';

  type Kind = 'runtime' | 'model';
  let { setup }: { setup?: {
    feature: Feature; selected: string | null; busy: boolean;
    choose: (id: string) => void;
  } } = $props();
  type CatalogFilter = { query: string; engine: string; installed: boolean; recommendedFirst: boolean; limit: number };
  const features = [
    { id: 'stt', label: 'settings.dictation', acronym: 'STT', icon: 'audio' },
    { id: 'tts', label: 'module.speak', acronym: 'TTS', icon: 'text' },
    { id: 'ocr', label: 'module.vision', acronym: 'OCR', icon: 'screen' },
  ] as const;
  const newFilter = (): CatalogFilter => ({ query: '', engine: 'all', installed: false, recommendedFirst: true, limit: 30 });
  const filters = $state<Record<`${Feature}.${Kind}`, CatalogFilter>>({
    'stt.runtime': newFilter(), 'stt.model': newFilter(),
    'tts.runtime': newFilter(), 'tts.model': newFilter(),
    'ocr.runtime': newFilter(), 'ocr.model': newFilter(),
  });
  const feature = $derived(setup?.feature ?? catalogView.feature);
  const showEfficiency = $derived(feature === 'stt');
  type SortKey = 'name' | 'size' | 'wer' | 'accuracy' | 'efficiency';
  const columns = $derived<{ key: SortKey; label: string }[]>([
    { key: 'name', label: 'module.sort_name' },
    { key: 'size', label: 'module.sort_size' },
    feature === 'ocr'
      ? { key: 'accuracy', label: 'module.sort_accuracy' }
      : { key: 'wer', label: 'module.sort_wer' },
    ...(showEfficiency ? [{ key: 'efficiency', label: 'module.sort_efficiency' } as const] : []),
  ]);
  const benchmarkHelp = {
    stt: 'module.benchmark_help',
    tts: 'module.benchmark_tts_help',
    ocr: 'module.benchmark_ocr_help',
  };
  const modelSort = $state<Partial<Record<Feature, { key: SortKey; descending: boolean }>>>({});
  const sorting = $derived(modelSort[feature] ?? (feature === 'stt'
    ? { key: 'efficiency', descending: true } as const
    : { key: 'size', descending: false } as const));
  function changeSort(key: SortKey) {
    modelSort[feature] = {
      key,
      descending: sorting.key === key ? !sorting.descending : key === 'efficiency' || key === 'accuracy',
    };
  }
  function efficiency(row: ModuleModel) {
    return row.benchmark_wer && row.size_mb ? 1000 / (row.benchmark_wer * row.size_mb) : null;
  }
  function score(model: ModuleModel) {
    return model.benchmark_score ?? model.benchmark_wer;
  }
  function compareModels(a: ModuleModel, b: ModuleModel, recommendedFirst: boolean) {
    if (recommendedFirst) {
      const recommended = Number(!!b.recommended) - Number(!!a.recommended);
      if (recommended) return recommended;
    }
    const missing = Number(score(a) == null) - Number(score(b) == null);
    if (missing) return missing;
    const { key, descending } = sorting;
    const name = a.label.localeCompare(b.label);
    const delta = key === 'name' ? name : key === 'size'
      ? modelBytes(a) - modelBytes(b)
      : key === 'efficiency' ? (efficiency(a) ?? 0) - (efficiency(b) ?? 0)
      : (score(a) ?? 0) - (score(b) ?? 0);
    return (descending ? -delta : delta) || name;
  }
  const cleanupSelected = $derived(!setup && catalogView.panel === "cleanup");
  $effect(() => { if (!setup) markDirty(); });
  const engines = $derived([...new Map(modules.runtimes
    .filter(runtime => runtime.feature === feature && runtime.installed)
    .map(runtime => [runtime.engine, {
      id: runtime.engine,
      label: runtime.label.replace(/\s*[·–—-]\s*(CPU|CUDA|GPU|Vulkan).*$/i, '').replace(/\s+\(?(CPU|CUDA|GPU|Vulkan).*$/i, ''),
    }])).values()]);
  const jobs = $derived(modules.assets?.jobs ?? (modules.assets?.progress ? [modules.assets.progress] : []));
  const activePhases = ['queued', 'downloading', 'verifying', 'extracting'];
  const activeDownloads = $derived([
    ...jobs.filter(job => activePhases.includes(job.phase)).map(job => ({
      ...job, label: modules.assets?.assets.find(asset => asset.id === (job.job_id || job.id))?.label ?? job.id,
    })),
    ...Object.entries(ui.downloads).map(([id, job]) => ({id, label: id, ...job})),
  ]);
  const installedRuntimes = $derived(modules.runtimes.filter(runtime => runtime.feature === feature && runtime.installed));
  const cuda = $derived(modules.runtimes.find(runtime => runtime.backend === 'cuda' && runtime.action_asset));
  const optionalRuntimes = $derived(modules.runtimes.filter(runtime => runtime.backend !== 'cuda' &&
    runtime.feature === feature && (!runtime.bundled || !runtime.installed)));
  function actionAsset(row: { asset: string | null; action_asset?: string | null }) {
    return row.action_asset ?? row.asset;
  }
  function runtimeInstalled(row: { installed: boolean; asset: string | null; action_asset?: string | null }) {
    return row.action_asset && row.action_asset !== row.asset
      ? modules.assets?.assets.some(asset => asset.id === row.action_asset && asset.installed) ?? false
      : row.installed;
  }
  function catalogRows(kind: Kind, filter: CatalogFilter, selectedEngine: string) {
    const rows = kind === 'runtime'
      ? [...optionalRuntimes, ...(cuda ? [{ ...cuda, id: cuda.action_asset!, feature, recommended: false }] : [])]
        .map(runtime => ({ ...runtime, kind: 'runtime' as const, languages: [] as string[] }))
      : modules.models.map(model => ({ ...model, kind: 'model' as const }));
    return rows.filter(row => row.feature === feature
      && (selectedEngine === 'all' || row.engine === selectedEngine)
      && (!filter.installed || runtimeInstalled(row))
      && (!filter.query.trim() || matchesCatalogSearch(filter.query, row,
        moduleSearchText(row, modules.models, modules.runtimes, modules.assets?.assets))))
      .sort((a, b) => a.kind === 'model' && b.kind === 'model'
        ? compareModels(a, b, filter.recommendedFirst)
        : Number(!!b.recommended) - Number(!!a.recommended) || a.label.localeCompare(b.label));
  }

  function downloadValue(bar: HTMLProgressElement, value: number | null) {
    function update(bytes: number | null) {
      if (bytes === null) bar.removeAttribute('value');
      else bar.value = bytes;
    }
    update(value);
    return { update };
  }

  async function refresh() {
    try {
      if (ui.state?.service_online) await action('refresh_models');
      await refreshModules();
    } catch (error) { notice(t('assets.failed'), 'error', String(error)); }
  }
  async function assetAction(name: string, id: string, managed = true) {
    try {
      if (managed) await api('/api/asset-action', { action: name, id });
      else {
        await action('remove_model', id);
        await refreshState(true);
      }
      await refreshModules();
    } catch (error) { notice(t(name === 'remove' ? 'ui.operation_failed.c4e6ed' : 'assets.failed'), 'error', String(error)); }
  }
  async function cancelDownload(id: string, asset: string | null) {
    try {
      if (asset) await assetAction('cancel', asset);
      else await action('cancel_model_download', id);
    } catch (error) { notice(t('assets.failed'), 'error', String(error)); }
  }
  function megabytes(bytes: number) { return (bytes / 1024 / 1024).toFixed(1); }
  function languageSummary(languages: string[]) {
    return languages.slice(0, 3).map(languageName).join(', ');
  }
  function size(id: string, asset: string | null) {
    return asset ? modules.assets?.assets.find(entry => entry.id === asset)?.bytes
      : ((ui.state?.models.find(model => model.file === id)?.size_mb || 0) * 1024 * 1024);
  }
  function modelBytes(model: ModuleModel) {
    return size(model.id, actionAsset(model)) || (model.size_mb ?? 0) * 1024 * 1024;
  }
  onMount(() => {
    void refreshModules().catch(error => notice(t('assets.failed'), 'error', String(error)));
    if (!setup) void loadReading().catch(error => notice(t('reading.failed'), 'error', String(error)));
  });
</script>

{#snippet downloadStatus(label: string, job: { phase?: string; downloaded: number; total: number }, summary = false)}
  {@const phase = job.phase || 'downloading'}
  {@const status = t(phase === 'queued' ? 'module.queued' : `assets.${phase}`)}
  {@const percent = phase === 'downloading' && job.total > 0 ? Math.min(99, Math.floor(job.downloaded / job.total * 100)) : null}
  <div class="download-detail">
    <span>{summary ? label : status}</span>
    {#if percent !== null}
      <bdi dir="ltr">{#if !summary}{megabytes(job.downloaded)} / {megabytes(job.total)} MB · {/if}{percent}%</bdi>
    {:else if summary}<span>{status}</span>{/if}
  </div>
  <progress max={100} use:downloadValue={percent} aria-label={`${status} ${label}`}></progress>
{/snippet}

{#if !setup}
{#if activeDownloads.length}
  <section class="setup-downloads" aria-label={t('settings.downloads')}>
    <h2>{t('settings.downloads')}</h2>
    {#each activeDownloads as job (job.id)}
      {@render downloadStatus(job.label, job, true)}
    {/each}
  </section>
{/if}
<div class="feature-catalogs" role="group" aria-label={t('module.feature_filter')}>
  {#each features as option}
    <button type="button" data-catalog-feature={option.id} class:chosen={!cleanupSelected && feature === option.id}
      aria-pressed={!cleanupSelected && feature === option.id} onclick={() => {catalogView.feature = option.id;catalogView.panel="models";}}>
      <Icon name={option.icon} /><span>{t(option.label)}</span>
    </button>
  {/each}
  <button type="button" data-catalog-feature="cleanup" class:chosen={cleanupSelected} aria-pressed={cleanupSelected} onclick={()=>catalogView.panel="cleanup"}><Icon name="cleanup"/><span>{t("ui.text_cleanup.52f093")}</span></button>
</div>
{/if}
{#if cleanupSelected}
<Cleanup section="provider"/>
{:else}
{#if !setup}<div class="selection-section" data-catalog-selection={feature}>
  {#if feature === 'stt'}<SttModelSetup/>
  {:else}<ReaderModelSetup {feature}/>{/if}
</div>{/if}
{@render catalog('model')}
{#if !setup}
  {#if cuda || optionalRuntimes.length}
    <details class="optional-runtimes"><summary data-download-kind="runtime">{t('module.optional_runtimes')}</summary>
      {@render catalog('runtime')}
    </details>
  {/if}
  <ModelFiles {feature}/>
{/if}
{/if}

{#snippet catalog(kind: Kind)}
{@const filter = filters[`${feature}.${kind}`]}
{@const selectedEngine = engines.some(option => option.id === filter.engine) ? filter.engine : 'all'}
{@const rows = catalogRows(kind, filter, selectedEngine)}
{@const searchLabel = t(kind === 'runtime' ? 'module.search_runtimes' : 'module.search_models')}
<section class="catalog" id="{kind}-section" aria-labelledby="{kind}-heading" data-catalog="{feature}.{kind}">
  <h2 id="{kind}-heading">{t(kind === 'runtime' ? 'module.runtimes' : 'ui.models.f3798f')}{#if kind === 'model'} <Help text={benchmarkHelp[feature]} />{/if}</h2>
  <div class="catalog-toolbar" data-setting-path="{kind}-search">
    <div class="search-input"><Icon name="search" /><input id="{kind}-search" type="search" dir="auto" aria-label={searchLabel}
      placeholder={searchLabel} bind:value={filter.query} oninput={() => filter.limit = 30} /></div>
    <IconButton icon="refresh" label="module.refresh" onclick={refresh} />
  </div>
  <div class="library-filters">
    {#if engines.length > 1}
      <div class="engine-filters" role="group" aria-label={t('module.engine_filter')}>
        <button type="button" class:chosen={selectedEngine === 'all'} aria-pressed={selectedEngine === 'all'} onclick={() => { filter.engine = 'all'; filter.limit = 30; }}>{t('module.all_engines')}</button>
        {#each engines as option (option.id)}<button type="button" class:chosen={selectedEngine === option.id} aria-pressed={selectedEngine === option.id} onclick={() => { filter.engine = option.id; filter.limit = 30; }}>{option.label}</button>{/each}
      </div>
    {/if}
    <label class="catalog-filter"><input type="checkbox" bind:checked={filter.installed} />{t('ui.installed.7bb440')}</label>
    {#if kind === 'model'}
      <label class="catalog-filter"><input type="checkbox" bind:checked={filter.recommendedFirst} onchange={() => filter.limit = 30} />{t('module.recommended_first')}</label>
    {/if}
  </div>
  <div id="{kind}-results" class="model-results" class:sortable={kind === 'model'} class:efficient={showEfficiency}>
    {#if kind === 'model'}
      <div class="model-columns" role="group" aria-label={t('module.sort_models')}>
        {#each columns as column (column.key)}
          <button type="button" aria-pressed={sorting.key === column.key} data-sort={column.key}
            aria-label={t(column.label) + (sorting.key === column.key ? ' · ' + t(sorting.descending ? 'module.descending' : 'module.ascending') : '')}
            onclick={() => changeSort(column.key)}>{t(column.label)}
            <span class:up={!sorting.descending} class:inactive={sorting.key !== column.key} aria-hidden="true"><Icon name="chevron" size={12} /></span>
          </button>
        {/each}
      </div>
    {/if}
    {#each rows.slice(0, filter.limit) as row (`${row.kind}-${row.id}`)}
      {@const actionId = actionAsset(row)}
      {@const asset = modules.assets?.assets.find(entry => entry.id === actionId)}
      {@const installed = runtimeInstalled(row)}
      {@const job = actionId ? jobs.find(job => (job.job_id || job.id) === actionId) : ui.downloads[row.id]}
      {@const phase = job?.phase || (job ? 'downloading' : '')}
      {@const busy = activePhases.includes(phase)}
      {@const title = row.kind === 'runtime' ? runtimeLabel(row) : row.label}
      {@const bytes = row.kind === 'model' ? modelBytes(row) : row.bundled || installed ? 0 : size(row.id, actionId)}
      <article class="model-result" data-model-id={row.id}>
        <div class="model-copy">
          <strong dir="auto">{title}</strong>
          {#if row.recommended}<span class="recommendation" title={recommendationLabel(row.recommendation)}><Icon name="check" size={14} />{t('ui.recommended.9ef937')}</span>{/if}
          {#if row.kind === 'model' && row.installed && !installedRuntimes.some(runtime => runtime.engine === row.engine)}<span>{t('module.runtime_missing')}</span>{/if}
          {#if row.languages.length || (row.kind === 'runtime' && bytes)}
            <span>{languageSummary(row.languages)}{#if row.languages.length > 3}{' '}<span class="language-overflow"><bdi dir="ltr">+{row.languages.length - 3}</bdi><Help text={row.languages.map(languageName).join(', ')} /></span>{/if}{#if row.kind === 'runtime' && bytes}<bdi dir="ltr">{Math.ceil(bytes / 1024 / 1024)} MB</bdi>{/if}</span>
          {/if}
          {#if row.kind === 'runtime'}
            <span dir="auto">{runtimeHardware(row)}</span>
            {#if row.backend === 'cuda'}<span>{t('module.nvidia_shared')} {t('module.runtime.cuda_tradeoff')}</span>{/if}
          {:else if row.feature !== 'stt'}
            <small dir="ltr">{row.id}</small>
          {/if}
        </div>
        {#if row.kind === 'model'}
          <bdi class="model-metric" dir="ltr">{bytes ? Math.ceil(bytes / 1024 / 1024) + ' MB' : '—'}</bdi>
          <bdi class="model-metric" dir="auto">{score(row)?.toFixed(2).concat('%') ?? t('module.score_unknown')}</bdi>
          {#if showEfficiency}
            <bdi class="model-metric" dir="ltr">{efficiency(row)?.toFixed(2) ?? '—'}</bdi>
          {/if}
        {/if}
        <div class="model-actions">
          {#if row.kind === 'runtime'}
            {#if busy}
              <button type="button" aria-label={`${t('assets.cancel')} ${title}`} onclick={() => cancelDownload(row.id, actionId)}>{t('assets.cancel')}</button>
            {:else if installed}
              {#if row.backend === 'cuda'}<span title={t('ui.installed.7bb440')}>✓</span>
              {:else}<button type="button" aria-pressed={row.selected ?? false}
                onclick={() => assetAction('choose', row.id)}>{t('setup.use')}</button>{/if}
            {:else if actionId}
              <button type="button" disabled={!asset?.available}
                onclick={() => assetAction('install', actionId)}>{t('assets.download')}</button>
            {/if}
            {#if !busy && installed && !row.bundled && actionId}
              <IconButton icon="trash" label="assets.remove" onclick={() => assetAction('remove', actionId)} />
            {/if}
          {:else if setup}
            <button type="button" aria-pressed={setup.selected === row.id}
              disabled={setup.busy || (!row.installed && (row.asset ? !asset?.available : !ui.state?.service_online))}
              onclick={() => setup?.choose(row.id)}>{t(row.installed || busy ? 'setup.use' : 'assets.download')}</button>
          {:else if busy}<button type="button" aria-label={`${t('assets.cancel')} ${title}`} onclick={() => cancelDownload(row.id, row.asset)}>{t('assets.cancel')}</button>
          {:else if row.installed}<span title={t('ui.installed.7bb440')}>✓</span>
            {#if row.asset}<IconButton icon="trash" label="assets.remove" onclick={() => assetAction('remove', row.asset!)} />
            {:else}<IconButton icon="trash" label="assets.remove" disabled={!ui.state?.service_online} onclick={() => assetAction('remove', row.id, false)} />{/if}
          {:else if row.asset}<IconButton icon="download" label="assets.download" disabled={!asset?.available} onclick={() => assetAction('install', row.asset!)} />
          {:else}<IconButton icon="download" label="assets.download" disabled={!ui.state?.service_online} onclick={() => download(row.id)} />
          {/if}
        </div>
        {#if busy && job}
          <div class="item-download" role="status">
            {@render downloadStatus(title, job)}
          </div>
        {:else if phase === 'failed' && job && 'error' in job && job.error}
          <details class="download-error"><summary>{t('assets.failed')} · {t('ui.details.dc3dec')}</summary><pre dir="auto">{job.error}</pre></details>
        {/if}
      </article>
    {/each}
    {#if !rows.length}<p class="empty" role="status">{t('module.no_results')}</p>{/if}
    {#if rows.length > filter.limit}<button type="button" onclick={() => filter.limit += 30}>{t('module.more')}</button>{/if}
  </div>
</section>
{/snippet}

<style>
  .feature-catalogs { display: flex; border-block-end: 1px solid var(--line); gap: 8px; margin-block-end: 20px; }
  .feature-catalogs button { display: flex; align-items: center; justify-content: center; gap: 8px; flex: 1; min-width: 0; padding: 12px 8px; border: 0; border-block-end: 2px solid transparent; border-radius: 0; background: transparent; }
  .feature-catalogs button.chosen { color: var(--accent); border-block-end-color: var(--accent); }
  .selection-section { margin-block-end: 28px; }
  .optional-runtimes { margin-block: 16px; }
  .optional-runtimes summary { cursor: pointer; }
  .model-results > button { margin-block: 12px; }
  .catalog h2 { margin-block: 0 16px; font-size: 17px; }
  .engine-filters button.chosen { background: var(--accent-soft); color: var(--accent); border-color: var(--accent); }
  .library-filters { display: flex; gap: 12px; flex-wrap: wrap; align-items: center; margin-block: 0 8px; }
  .engine-filters { display: flex; gap: 6px; flex-wrap: wrap; }
  .engine-filters button { padding: 5px 12px; }
  .catalog-filter { display: flex; align-items: center; gap: 8px; }
  .model-copy .recommendation { display: flex; align-items: center; gap: 4px; color: var(--accent); }
  .model-copy small { display: block; }
  .language-overflow {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    vertical-align: middle;
    white-space: nowrap;
  }
  .model-result {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
  }
  .sortable {
    display: grid;
    grid-template-columns: minmax(0, 1fr) repeat(3, max-content);
    column-gap: 6px;
  }
  .efficient { grid-template-columns: minmax(0, 1fr) repeat(4, max-content); }
  .sortable > * { grid-column: 1 / -1; }
  .sortable .model-result, .model-columns {
    display: grid;
    grid-template-columns: subgrid;
  }
  .sortable .model-result { padding-block: 8px; }
  .sortable .model-actions { justify-self: end; }
  .model-columns button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 2px;
    min-width: 0;
    padding: 4px;
    font-size: 12px;
  }
  .model-columns span { display: flex; }
  .model-columns .up { transform: rotate(180deg); }
  .model-columns .inactive { visibility: hidden; }
  .model-metric { text-align: end; font-size: 12px; font-variant-numeric: tabular-nums; }
  .item-download, .download-error { grid-column: 1 / -1; min-width: 0; }
  .download-detail { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; flex-wrap: wrap; color: var(--muted); font-size: 12px; }
  .download-detail bdi { font-variant-numeric: tabular-nums; }
  progress {
    --fill: repeating-linear-gradient(90deg, var(--range-blue) 0 8px, transparent 8px 11px);
    display: block;
    width: 100%;
    margin-top: 8px;
    height: 16px;
    appearance: none;
    border: 0;
    padding: 2px;
    background: var(--bevel-mid);
    box-shadow: var(--sunken-edge);
  }
  progress::-webkit-progress-bar { background: none; }
  progress::-webkit-progress-value { background: var(--fill); }
  progress::-moz-progress-bar { background: var(--fill); }
  progress:indeterminate {
    --fill: none;
    background-image: repeating-linear-gradient(135deg, transparent 0 5px, var(--line) 5px 10px);
  }
  pre { white-space: pre-wrap; overflow-wrap: anywhere; }
  @media (max-width: 600px) {
    .feature-catalogs { gap: 4px; }
    .feature-catalogs button { display: grid; grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(3em, auto) auto; gap: 4px; padding-inline: 4px; line-height: 1.5; }
    .feature-catalogs button :global(svg) { display: none; }
  }
</style>
