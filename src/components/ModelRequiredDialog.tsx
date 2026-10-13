import {useEffect,useState} from 'react';
import {createPortal} from 'react-dom';
import {tr} from '@/i18n';
import ModalBackdrop from './ModalBackdrop';
import ModelPicker from './ModelPicker';
export default function ModelRequiredDialog(){
 const [message,setMessage]=useState(''),[configure,setConfigure]=useState(false);
 useEffect(()=>{const required=(e:Event)=>setMessage((e as CustomEvent<string>).detail||tr('此功能需要配置模型'));window.addEventListener('model-required',required);return()=>window.removeEventListener('model-required',required);},[]);
 return <>{configure&&<ModelPicker standalone onClose={()=>setConfigure(false)}/>} {message&&createPortal(<ModalBackdrop style={{zIndex:2200}} onDismiss={()=>setMessage('')}><section className="modal" role="dialog" aria-modal="true" aria-label={tr('需要配置模型')}><h3>{tr('需要配置模型')}</h3><p>{tr(message)}</p><p>{tr('配置完成后可返回重试，当前剧本和输入不会被替换。')}</p><div className="modal-actions"><button onClick={()=>setMessage('')}>{tr('取消')}</button><button className="primary" onClick={()=>{setMessage('');setConfigure(true);}}>{tr('去配置模型')}</button></div></section></ModalBackdrop>,document.body)}</>;
}
