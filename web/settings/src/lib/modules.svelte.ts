import {api} from './api';
import type {AssetState} from './reader';
export type Feature='stt'|'tts'|'ocr';
function rememberedPage():'runtime'|'model' {try {return localStorage.getItem('smalltalkai-model-page')==='runtime'?'runtime':'model';}catch {return 'model';}}
export const catalogView=$state<{feature:Feature;kind:'runtime'|'model';panel:'models'|'cleanup'}>({feature:'stt',kind:rememberedPage(),panel:'models'});
export interface Runtime {id:string;feature:Feature;engine:string;backend:string;label:string;asset:string|null;installed:boolean;directory:string|null;recommended?:boolean;recommendation?:string|null}
export interface ModuleModel {id:string;feature:Feature;engine:string;label:string;languages:string[];installed:boolean;asset:string|null;recommended?:boolean;recommendation?:string|null}
export interface Connection {base_url:string;model:string;voice:string;format:string;prompt?:string;key_slot:string|null;timeout_ms:number}
export interface RemoteRoute {enabled:boolean;primary:Connection;fallback_to_local:boolean}
export interface Binding {remote?:RemoteRoute|null;runtime:string|null;model:string|null;device:'auto'|'cpu'|'gpu';webgpu_adapter?:number;voice?:string|null;steps?:number|null;speed_percent?:number|null;lowercase?:boolean;currency_words?:boolean;clean_text?:boolean}
export interface Selection {mode:'single_model'|'follow_keyboard'|'auto_detect';single:Binding;languages:Record<string,Binding>}
export const modules=$state({runtimes:[] as Runtime[],models:[] as ModuleModel[],assets:null as AssetState|null});
let selectionRevision:number|undefined;
let selectionRefresh:(()=>Promise<boolean>)|undefined;
export function onAutomaticSelection(refresh?:()=>Promise<boolean>){selectionRefresh=refresh;}
let catalogStamp = '', assetStamp = '', requestedRefresh = 0, appliedRefresh = 0;
export async function refreshModules(){
 const request = ++requestedRefresh;
 const [catalog,assets]=await Promise.all([api<{runtimes:Runtime[];models:ModuleModel[]}>('/api/modules'),api<AssetState>('/api/assets')]);
 if (request < appliedRefresh) return;
 appliedRefresh = request;
 const nextCatalog = JSON.stringify(catalog), nextAssets = JSON.stringify(assets);
 if (nextCatalog !== catalogStamp) {
  modules.runtimes = catalog.runtimes;
  modules.models = catalog.models;
  catalogStamp = nextCatalog;
 }
 if (nextAssets !== assetStamp) {
  modules.assets = assets;
  assetStamp = nextAssets;
 }
 const revision=assets.selection_revision??0;
 if(selectionRevision===undefined)selectionRevision=revision;
 else if(revision!==selectionRevision&&selectionRefresh&&await selectionRefresh())selectionRevision=revision;
}
export const emptyBinding=():Binding=>({runtime:null,model:null,device:'auto',clean_text:true,currency_words:true});
