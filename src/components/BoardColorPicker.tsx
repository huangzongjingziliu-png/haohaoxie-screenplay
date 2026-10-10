import {tr,fmt} from '@/i18n';
import { useState } from 'react';
const COLORS=['#FFF2A8','#FFD7DE','#CDE9FF','#D7F0D2','#E7DBFF','#FFE0BC','#FFFFFF','#E6E6E6','#9FC9F0','#A9DBB8','#C6ACE8','#F1AFBE','#FFCC66','#FF9966','#FF99CC','#99CCFF','#99DDCC','#CCBBFF','#FF7777','#77AAFF','#88BB66','#BB88DD','#444444','#000000'];
export default function BoardColorPicker({label,value,disabled,onChange}:{label:string;value:string;disabled?:boolean;onChange:(color:string)=>void}) {
  const [open,setOpen]=useState(false),[draft,setDraft]=useState(value);
  return <span className="board-color-picker"><button disabled={disabled} aria-label={fmt("{0}色板",tr(label))} aria-expanded={open} title={tr("选择颜色")} style={{background:value}} onClick={()=>{setDraft(value);setOpen(!open);}} />{open && <span className="board-color-popover" role="group" aria-label={fmt("{0}选择器",tr(label))}><span className="board-color-grid">{COLORS.map(color=><button key={color} aria-label={`${tr(label)} ${color}`} title={color} style={{background:color}} onClick={()=>{onChange(color);setOpen(false);}}/>)}</span><label>{tr("自定义颜色")}<input aria-label={tr(label)} value={draft} maxLength={7} onChange={e=>{const color=e.currentTarget.value;setDraft(color);if(/^#[0-9a-f]{6}$/i.test(color))onChange(color);}} placeholder="#RRGGBB" /></label><button onClick={()=>setOpen(false)}>{tr("完成")}</button></span>}</span>;
}
