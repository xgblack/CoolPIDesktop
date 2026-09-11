import {ModelSettings} from './model-settings';
import {useState} from 'react';
import {Copy,FolderOpen,Monitor,Sun,Moon,Search,LoaderCircle,GitBranch,Users} from 'lucide-react';
import {Button} from '@/components/ui/button';
import {Input} from '@/components/ui/input';
import {Label} from '@/components/ui/label';
import {Checkbox} from '@/components/ui/checkbox';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription,DialogFooter} from '@/components/ui/dialog';
import {host,hostError,records,recoverSession} from '../../host';
import {ErrorNotice} from './shared';
import type {HostError,RuntimeInfo,TaskRecord} from '../../../../../packages/host-contract/src';
import type {Theme} from './theme';

export function NameDialog({kind,initial,onClose,onSubmit,restoreFocus}:{kind:'project'|'task'|'rename';initial:string;onClose:()=>void;onSubmit:(name:string,trusted:boolean,mode:'shared'|'isolated')=>Promise<void>;restoreFocus:()=>void}){
 const [value,setValue]=useState(initial),[trusted,setTrusted]=useState(false),[mode,setMode]=useState<'shared'|'isolated'>('shared'),[busy,setBusy]=useState(false),[error,setError]=useState<HostError|null>(null);
 const title=kind==='project'?'添加项目':kind==='task'?'创建任务':'重命名';
 return <Dialog open onOpenChange={v=>{if(!v&&!busy)onClose();}}><DialogContent onCloseAutoFocus={e=>{e.preventDefault();restoreFocus();}}><DialogHeader><DialogTitle>{title}</DialogTitle><DialogDescription>{kind==='project'?'通过系统目录选择器登记主目录及附加目录。第一项为主目录。':kind==='task'?'任务保留自己的会话和目录快照，创建后不会自动启动 OMP。':'修改显示名称，不改变任务或会话身份。'}</DialogDescription></DialogHeader><form onSubmit={e=>{e.preventDefault();if(!value.trim()){setError({code:'invalid_name',message:'请输入名称'});return;}setBusy(true);setError(null);void onSubmit(value.trim(),trusted,mode).then(onClose).catch(e=>setError(hostError(e))).finally(()=>setBusy(false));}}>
  <Label htmlFor="name-field">名称</Label><Input id="name-field" autoFocus value={value} onChange={e=>setValue(e.target.value)} maxLength={160} aria-invalid={!!error} className="mt-2"/>
  {kind==='project'&&<label className="trust-label"><Checkbox checked={trusted} onCheckedChange={v=>setTrusted(v===true)}/><span>我信任所选目录中的配置与扩展，允许 OMP 加载它们。</span></label>}
  {kind==='task'&&<div className="space-y-2"><Label>执行目录</Label><div className="flex gap-2"><Button type="button" variant={mode==='shared'?'secondary':'outline'} onClick={()=>setMode('shared')}><Users/>共享目录</Button><Button type="button" variant={mode==='isolated'?'secondary':'outline'} onClick={()=>setMode('isolated')}><GitBranch/>隔离 worktree</Button></div>{mode==='isolated'&&<p className="text-xs text-muted-foreground">仅支持 Git 根目录。同一仓库会共用一个 worktree；未提交改动不包含在新任务中。</p>}</div>}
  <ErrorNotice error={error}/><DialogFooter className="mt-5"><Button type="button" variant="outline" disabled={busy} onClick={onClose}>取消</Button><Button disabled={busy||!value.trim()||kind==='project'&&!trusted}>{busy?<LoaderCircle className="animate-spin"/>:kind==='project'?<FolderOpen/>:null}{kind==='project'?'选择目录':busy?'保存中':'保存'}</Button></DialogFooter>
 </form></DialogContent></Dialog>;
}

export function RuntimeSettings({open,onClose,theme,onTheme,themeError,restoreFocus,onSaved}:{open:boolean;onClose:()=>void;theme:Theme;onTheme:(t:Theme)=>void;themeError:string;restoreFocus:()=>void;onSaved:()=>Promise<unknown>}){
 const [tab,setTab]=useState<'general'|'models'>('models');
 const [modelDirty,setModelDirty]=useState(false),[confirmClose,setConfirmClose]=useState(false);
 const [path,setPath]=useState(''),[runtime,setRuntime]=useState<RuntimeInfo>(),[busy,setBusy]=useState(false),[error,setError]=useState<HostError|null>(null);
 return <Dialog open={open} onOpenChange={v=>{if(!v){if(modelDirty)setConfirmClose(true);else onClose();}}}><DialogContent className="settings-dialog" onCloseAutoFocus={e=>{e.preventDefault();restoreFocus();}}><DialogHeader><DialogTitle>设置</DialogTitle><DialogDescription>管理工作台外观和系统安装的 OMP。</DialogDescription></DialogHeader>
  {confirmClose&&<div role="alert"><p>有未保存的修改或操作正在执行。完成操作后再关闭，或明确放弃当前输入。</p><Button variant="outline" onClick={()=>setConfirmClose(false)}>继续编辑</Button><Button variant="destructive" onClick={()=>{setConfirmClose(false);setModelDirty(false);onClose();}}>放弃输入并关闭</Button></div>}
  <div className="settings-navigation"><Button aria-pressed={tab==='models'} variant={tab==='models'?'secondary':'ghost'} onClick={()=>setTab('models')}>模型配置</Button><Button aria-pressed={tab==='general'} variant={tab==='general'?'secondary':'ghost'} onClick={()=>setTab('general')}>外观与运行时</Button></div>
  <div className="settings-body" hidden={tab!=='models'}><h2>模型配置</h2><p className="settings-intro">连接你的模型，开始工作。</p>{open&&<ModelSettings onSaved={onSaved} onDirtyChange={setModelDirty}/>}</div><div className="settings-body" hidden={tab!=='general'}>
  <section className="settings-section"><h3>外观</h3><div className="theme-options">{([['light','浅色',Sun],['dark','深色',Moon],['system','跟随系统',Monitor]] as const).map(([value,label,Icon])=><Button key={value} variant={theme===value?'secondary':'outline'} aria-pressed={theme===value} onClick={()=>onTheme(value)}><Icon/>{label}</Button>)}</div>{themeError&&<p role="alert">{themeError}</p>}</section>
  <section className="settings-section"><h3>OMP 运行时</h3><p className="text-muted-foreground text-xs mb-4">使用本机已安装的 OMP。检测会短暂启动探测进程，不运行任务。</p><Label htmlFor="runtime-path">可执行文件路径</Label><Input id="runtime-path" value={path} onChange={e=>setPath(e.target.value)} placeholder="留空使用已保存路径或 PATH" className="my-2 font-mono text-xs"/><Button variant="outline" disabled={busy} onClick={()=>{setBusy(true);setError(null);void host.runtime(path).then(setRuntime).catch(e=>setError(hostError(e))).finally(()=>setBusy(false));}}>{busy?<LoaderCircle className="animate-spin"/>:<Search/>}{busy?'检测中':'检测 OMP'}</Button>
  {runtime&&<div className="runtime-result"><p>{runtime.status} · {runtime.version??'未知版本'} · RPC {runtime.protocol??'—'}</p><code className="break-all text-xs">{runtime.executable}</code><ErrorNotice error={runtime.error}/></div>}<ErrorNotice error={error}/></section>
  </div>
 </DialogContent></Dialog>;
}

export function RecoveryDialog({task,roots,onClose,onChanged,restoreFocus}:{task?:TaskRecord;roots:string[];onClose:()=>void;onChanged:()=>Promise<unknown>;restoreFocus:()=>void}){
 const [trusted,setTrusted]=useState(false),[candidate,setCandidate]=useState(''),[error,setError]=useState<HostError|null>(null),[busy,setBusy]=useState(false),[copied,setCopied]=useState('');
 const action=async(f:()=>Promise<unknown>)=>{setBusy(true);setError(null);try{await f();await onChanged();}catch(e){setError(hostError(e));}finally{setBusy(false);}};
 return <Dialog open onOpenChange={v=>{if(!v&&!busy)onClose();}}><DialogContent className="sm:max-w-xl" onCloseAutoFocus={e=>{e.preventDefault();restoreFocus();}}><DialogHeader><DialogTitle>{task?'目录与会话恢复':'项目目录'}</DialogTitle><DialogDescription>主目录决定 OMP 工作位置。附加目录保持独立，不代表隔离沙箱。</DialogDescription></DialogHeader>
  <div className="space-y-3">{roots.map((root,i)=><div key={root} className="root-path"><span>{i===0?'主目录':`附加目录 ${i}`}</span><code>{root}</code><Button size="icon-sm" variant="ghost" aria-label={`复制${i===0?'主目录':'附加目录 '+i}`} onClick={()=>void navigator.clipboard.writeText(root).then(()=>setCopied(root)).catch(e=>setError(hostError(e)))}><Copy/></Button>{copied===root&&<small role="status">已复制</small>}</div>)}</div>
  {task&&<><Label className="trust-label"><Checkbox checked={trusted} onCheckedChange={v=>setTrusted(v===true)}/>信任重新选择的目录及其配置</Label><Button variant="outline" disabled={busy||!trusted} onClick={()=>void action(async()=>{await records.relocate(task.id,trusted);setTrusted(false);})}><FolderOpen/>重新定位目录</Button><p className="text-xs text-muted-foreground">需要先停止任务。会话和任务记录不会被删除。</p><div className="border-t pt-4"><p className="text-xs break-all text-muted-foreground">会话：{task.sessionId??'尚未绑定'}</p>{!task.sessionId&&<Button className="mt-3" variant="outline" disabled={busy} onClick={()=>void action(async()=>setCandidate(await recoverSession(task.id,false)))}>查找未绑定会话</Button>}{candidate&&<div className="mt-3"><p className="text-xs break-all">唯一候选：{candidate}</p><Button disabled={busy} className="mt-2" onClick={()=>void action(async()=>{await recoverSession(task.id,true);setCandidate('');})}>确认恢复此会话</Button></div>}</div></>}
  <ErrorNotice error={error}/>
 </DialogContent></Dialog>;
}
