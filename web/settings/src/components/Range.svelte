<script lang="ts">
import Help from "./Help.svelte";
import NumberInput from "./NumberInput.svelte";
import RangeInput from "./RangeInput.svelte";
import {t} from "../lib/i18n";
let {label,path,value,min,max,step=1,unit="",help="",disabled=false,onchange}:{label:string;path:string;value:number|null;min:number;max:number;step?:number;unit?:string;help?:string;disabled?:boolean;onchange:(value:number)=>void}=$props();
</script>
<div class="field" class:field-disabled={disabled} data-setting-path={path}>
 <div class="field-copy"><label for={path}>{t(label)}</label>{#if help}<Help text={help}/>{/if}</div>
 <div class="control range-control">
   <RangeInput id={path} {min} {max} {step} {disabled} value={value??min} valueText={value===null?t("settings.mixed"):`${value}${unit}`} oninput={onchange}/>
   <NumberInput label={`${t(label)} ${t("settings.exact_value")}`} {min} {max} {step} {disabled} value={value??""} placeholder={t("settings.mixed")} oninput={onchange}/>
   {#if unit}<span class="range-value">{unit}</span>{/if}
 </div>
</div>
