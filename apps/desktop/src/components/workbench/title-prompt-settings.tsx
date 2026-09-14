import {useEffect,useState} from 'react';
import {LoaderCircle,RotateCcw,Save} from 'lucide-react';
import {Button} from '@/components/ui/button';
import {Label} from '@/components/ui/label';
import {Textarea} from '@/components/ui/textarea';
import {hostError,titlePrompt} from '../../host';
import type {HostError,TitlePromptSettings as Settings} from '../../../../../packages/host-contract/src';
import {ErrorNotice} from './shared';

const MAX_PROMPT_CHARS=16_000;

export function TitlePromptSettings({onDirtyChange}:{onDirtyChange?:(dirty:boolean)=>void}){
 const [settings,setSettings]=useState<Settings>(),[value,setValue]=useState('');
 const [busy,setBusy]=useState<'load'|'save'|'reset'|''>(''),[error,setError]=useState<HostError|null>(null),[notice,setNotice]=useState('');
 const dirty=!!settings&&value!==settings.prompt;
 useEffect(()=>onDirtyChange?.(dirty||!!busy),[dirty,busy,onDirtyChange]);
 const load=async()=>{setBusy('load');setError(null);setNotice('');try{const result=await titlePrompt.load();setSettings(result);setValue(result.prompt);}catch(e){setError(hostError(e));}finally{setBusy('');}};
 useEffect(()=>{void load();},[]);
 const apply=(result:Settings,message:string)=>{setSettings(result);setValue(result.prompt);setNotice(message);};
 const save=async()=>{if(!value.trim()||busy)return;setBusy('save');setError(null);setNotice('');try{apply(await titlePrompt.save(value),'标题提示词已保存。');}catch(e){setError(hostError(e));}finally{setBusy('');}};
 const reset=async()=>{if(busy)return;setBusy('reset');setError(null);setNotice('');try{apply(await titlePrompt.save(null),'已恢复 OMP 官方默认提示词。');}catch(e){setError(hostError(e));}finally{setBusy('');}};
 return <section className="settings-section title-prompt-settings" aria-label="标题生成提示词">
  <div className="model-toolbar"><div><h3>标题生成提示词</h3>{settings&&<p role="status">{settings.isDefault?'OMP 官方默认':'自定义提示词'}</p>}</div></div>
  {busy==='load'&&!settings?<div className="h-64 animate-pulse rounded-md bg-background" aria-label="正在加载标题提示词"/>:settings?<>
   <Label htmlFor="title-prompt-field">系统提示词</Label>
   <Textarea id="title-prompt-field" className="mt-2 min-h-64 resize-y bg-background font-mono text-xs" value={value} maxLength={MAX_PROMPT_CHARS} disabled={!!busy} aria-invalid={!!error||!value.trim()} onChange={e=>{setValue(e.target.value);setNotice('');setError(null);}}/>
   <div className="mt-3 flex flex-wrap items-center gap-2"><Button disabled={!!busy||!dirty||!value.trim()} onClick={()=>void save()}>{busy==='save'?<LoaderCircle className="animate-spin"/>:<Save/>}{busy==='save'?'保存中':'保存提示词'}</Button><Button variant="outline" disabled={!!busy||settings?.isDefault===true&&!dirty} onClick={()=>void reset()}>{busy==='reset'?<LoaderCircle className="animate-spin"/>:<RotateCcw/>}{busy==='reset'?'恢复中':'恢复默认'}</Button></div>
  </>:<Button variant="outline" onClick={()=>void load()}>重新加载</Button>}
  {notice&&<p role="status" className="mt-3 text-xs text-muted-foreground">{notice}</p>}<ErrorNotice error={error}/>
 </section>;
}
