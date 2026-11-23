<script lang="ts">
 import Combo from './Combo.svelte';
 import {localVoiceOptions,matchesSupertonicVoice} from '../lib/voices';
 import ModelTest from './ModelTest.svelte';
 import {moduleSearchText} from '../lib/search';
 import {runtimeHardware, runtimeLabel} from '../lib/module-labels';
 import IconButton from './IconButton.svelte';
 import Help from './Help.svelte';
 import RangeInput from './RangeInput.svelte';
 import NumberInput from './NumberInput.svelte';
 import {modules,refreshModules,type Feature,type Binding} from '../lib/modules.svelte';
 import {t} from '../lib/i18n';
 import {notice} from '../lib/state.svelte';
 let {feature,binding,language='',path,onchange}:{feature:Feature;binding:Binding;language?:string;path:string;onchange:(binding:Binding)=>void}=$props();
 const defaultRuntime=$derived(modules.runtimes.find(r=>r.feature===feature&&r.default));
 const runtimeId=$derived(binding.runtime||defaultRuntime?.id||'');
 const runtime=$derived(modules.runtimes.find(r=>r.id===runtimeId));
 const device=$derived(runtime?.backend==='cpu'||(feature==='ocr'&&binding.device==='auto')?'cpu':binding.device);
 const deviceOptions=$derived([
   ...(runtime?.backend!=='cpu'&&feature!=='ocr'?[{value:'auto',label:'ui.automatic.ac9041'}]:[]),
   {value:'cpu',label:'module.processor'},
   ...(runtime?.backend==='cpu'?[]:[{value:'gpu',label:runtime?.backend==='cuda'?'module.nvidia':runtime?.backend==='webgpu'?'module.webgpu_choose_gpu':'ui.gpu_backend'}]),
 ]);
 const runtimes=$derived([...(defaultRuntime?[]:[{value:'',label:'None'}]),...modules.runtimes.filter(r=>r.feature===feature&&r.installed).sort((a,b)=>Number(!!b.recommended)-Number(!!a.recommended)||a.label.localeCompare(b.label)).map(r=>({value:r.id,label:runtimeLabel(r),meta:[runtimeHardware(r), r.backend === 'cuda' ? t('module.runtime.cuda_tradeoff') : ''].filter(Boolean).join(' '),search:moduleSearchText(r,modules.models,modules.runtimes,modules.assets?.assets,false)}))]);
 const models=$derived([{value:'',label:'None'},...modules.models.filter(m=>m.feature===feature&&m.installed&&modules.runtimes.some(r=>r.feature===feature&&r.installed&&r.engine===m.engine)&&(!runtimeId||(runtime?.installed&&m.engine===runtime.engine))&&(!language||!m.languages.length||m.languages.includes(language))).sort((a,b)=>Number(b.recommended)-Number(a.recommended)||a.label.localeCompare(b.label)).map(m=>({value:m.id,label:m.label,search:moduleSearchText(m,modules.models,modules.runtimes,modules.assets?.assets,false)}))]);
 async function refresh(){try{await refreshModules();}catch(error){notice(t('ui.operation_failed.c4e6ed'),'error',String(error));}}
 function changeRuntime(value:string){const r=modules.runtimes.find(r=>r.id===value);const model=modules.models.find(m=>m.id===binding.model);const clear=!!(r&&model&&r.engine!==model.engine);onchange({...binding,runtime:value||null,model:clear?null:binding.model,voice:clear?null:binding.voice,steps:clear?null:binding.steps,lowercase:clear?false:binding.lowercase,currency_words:binding.currency_words,clean_text:binding.clean_text,device:r?.backend==='cpu'?'cpu':binding.device});}
 const voices=$derived(localVoiceOptions(binding.model));
</script>
<div class="engine-assignment" data-module={feature} data-language={language||'single'}>
 <Combo label="module.runtime" path={`${path}.runtime`} value={runtimeId} items={runtimes} unavailable onselect={changeRuntime}>
 {#snippet extra()}<IconButton icon="refresh" label="module.refresh" onclick={refresh}/>{/snippet}
 </Combo>
 {#if !modules.runtimes.some(r=>r.feature===feature&&r.installed)}<p class="inline-status">{t('module.install_runtime_first')}</p>{/if}
 <Combo label="ui.model.68c2cc" path={`${path}.model`} value={binding.model||''} items={models} unavailable onselect={(value)=>onchange({...binding,model:value||null,voice:value===binding.model?binding.voice:null,steps:value===binding.model?binding.steps:null,lowercase:value===binding.model?binding.lowercase:false,currency_words:binding.currency_words,clean_text:binding.clean_text})}>
 {#snippet extra()}<IconButton icon="refresh" label="module.refresh" onclick={refresh}/>{/snippet}
 </Combo>
 {#if feature==='tts' && (binding.model==='kokoro'||binding.model==='supertonic3')}
 <Combo label="reading.voice" path={`${path}.voice`} value={binding.voice||(binding.model==='kokoro'?'af_heart':'sid:0')} items={voices} searchMatcher={binding.model==='supertonic3'?matchesSupertonicVoice:undefined} onselect={(value)=>onchange({...binding,voice:value})}/>
 {#if binding.model==='kokoro'}
 <div class="field"><div class="field-copy"><label for={`${path}.lowercase`}>{t('module.lowercase')}</label><Help text="module.lowercase_help"/></div><div class="control"><input id={`${path}.lowercase`} type="checkbox" disabled={!(binding.clean_text??true)} checked={binding.lowercase??false} onchange={(e)=>onchange({...binding,lowercase:e.currentTarget.checked})}/></div></div>
 {/if}
 {#if binding.model==='supertonic3'}
 <div class="field"><div class="field-copy"><label for={`${path}.steps`}>{t('module.steps')} · {binding.steps??5}</label><Help text="module.steps_help"/></div><div class="control"><RangeInput id={`${path}.steps`} min={5} max={12} value={binding.steps??5} oninput={(steps) => onchange({...binding,steps})}/></div></div>
 {/if}
 {/if}
 <Combo label="ui.device.a5a74a" path={`${path}.device`} value={device} items={deviceOptions} onselect={value=>onchange({...binding,device:value as Binding['device']})}/>
 {#if runtime?.backend === 'webgpu' && binding.device === 'gpu'}
   <div class="field" data-setting-path={`${path}.webgpu_adapter`}>
     <div class="field-copy">
       <label for={`${path}.webgpu_adapter`}>{t('module.webgpu_adapter')}</label>
       <Help text="module.webgpu_adapter_help" />
     </div>
     <div class="control">
       <NumberInput id={`${path}.webgpu_adapter`} label={t('module.webgpu_adapter')}
         value={binding.webgpu_adapter ?? 0} min={0}
         oninput={(index) => { if (Number.isInteger(index) && index >= 0 && index <= 0xffffffff) onchange({...binding,webgpu_adapter:index}); }} />
     </div>
   </div>
 {/if}
 <ModelTest {feature} {binding} {language} />

</div>
