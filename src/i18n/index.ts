import {useSyncExternalStore} from 'react';
import catalog from './en.json';
export type UiLanguage = 'system' | 'zh' | 'en';
const KEY='haohaoxie.ui-language.v1';
const listeners=new Set<()=>void>();
let preference: UiLanguage=readPreference();
function readPreference():UiLanguage {try {const value=localStorage.getItem(KEY);return value==='zh'||value==='en'?value:'system';}catch{return 'system';}}
export function effectiveLanguage(value:UiLanguage=preference, systemLanguage=navigator.language):'zh'|'en' {return value==='system'?(systemLanguage.toLowerCase().startsWith('zh')?'zh':'en'):value;}
export function getUiLanguage(){return preference;}
function notify(){document.documentElement.lang=effectiveLanguage()==='zh'?'zh-Hans':'en';document.title=tr('好好写剧本');listeners.forEach(fn=>fn());}
export function setUiLanguage(value:UiLanguage){preference=value;try{localStorage.setItem(KEY,value);}catch{/* Session preference still works when storage is unavailable. */}notify();}
function subscribe(fn:()=>void){listeners.add(fn);return()=>{listeners.delete(fn);};}
export function useUiLanguage(){useSyncExternalStore(subscribe,()=>`${preference}:${effectiveLanguage()}`);return preference;}
export function tr(text:string):string {if(effectiveLanguage()==='zh')return text;const trimmed=text.trim();const translated=(catalog as Record<string,string>)[trimmed];if(!translated)return text;return text.replace(trimmed,translated);}
if(typeof window!=='undefined'){window.addEventListener('languagechange',notify);window.addEventListener('storage',e=>{if(e.key===KEY){preference=readPreference();notify();}});notify();}
/** Translate interface templates, leaving user-supplied values unchanged. */
export function fmt(text:string,...values:(string|number|undefined)[]){return tr(text).replace(/\{(\d+)\}/g,(_,index)=>String(values[Number(index)]??''));}
