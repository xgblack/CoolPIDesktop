import {useState} from 'react';
import {FolderPlus,X} from 'lucide-react';
import {localProjects,hostError} from '@/host';
import type {Project,HostError} from '../../../../../packages/host-contract/src';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription,DialogFooter} from '@/components/ui/dialog';
import {Button} from '@/components/ui/button';
import {Input} from '@/components/ui/input';
import {Checkbox} from '@/components/ui/checkbox';
import {ErrorNotice} from './shared';

export function LocalProjectDialog({onClose,onCreated}:{onClose:()=>void;onCreated:(project:Project)=>Promise<void>}){
 const [name,setName]=useState(''),[folders,setFolders]=useState<{token:string;path:string}[]>([]),[trusted,setTrusted]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState<HostError|null>(null);
 const choose=async()=>{setBusy(true);setError(null);try{const selected=await localProjects.choose();setFolders(old=>[...old,...selected.filter(f=>!old.some(o=>o.path===f.path))]);}catch(e){setError(hostError(e));}finally{setBusy(false);}};
 return <Dialog open onOpenChange={open=>{if(!open&&!busy)onClose();}}><DialogContent><DialogHeader><DialogTitle>创建项目</DialogTitle><DialogDescription className="sr-only">本地项目文件夹</DialogDescription></DialogHeader><form onSubmit={e=>{e.preventDefault();if(busy||!name.trim()||!folders.length||!trusted)return;setBusy(true);setError(null);void localProjects.create(name.trim(),folders.map(f=>f.token),trusted).then(onCreated).then(onClose).catch(e=>setError(hostError(e))).finally(()=>setBusy(false));}}>
  <label htmlFor="project-name">项目名称</label><Input id="project-name" autoFocus value={name} maxLength={160} disabled={busy} onChange={e=>setName(e.target.value)} className="my-3"/>
  <h3>源文件夹</h3><div className="project-folders">{folders.map((folder,index)=><div key={folder.token}><span title={folder.path}>{folder.path.split('/').filter(Boolean).at(-1)}</span><label><input type="radio" name="primary-folder" checked={index===0} disabled={busy} onChange={()=>setFolders(old=>[folder,...old.filter(f=>f.token!==folder.token)])}/>主要</label><Button type="button" size="icon-sm" variant="ghost" disabled={busy} aria-label={`移除 ${folder.path}`} onClick={()=>setFolders(old=>old.filter(f=>f.token!==folder.token))}><X/></Button></div>)}<Button type="button" variant="ghost" disabled={busy} onClick={()=>void choose()}><FolderPlus/>添加文件夹</Button></div>
  <label className="trust-label"><Checkbox checked={trusted} disabled={busy} onCheckedChange={v=>setTrusted(v===true)}/>信任这些目录中的配置与扩展</label><ErrorNotice error={error}/><DialogFooter><Button type="button" variant="ghost" disabled={busy} onClick={onClose}>取消</Button><Button disabled={busy||!name.trim()||!folders.length||!trusted}>{busy?'处理中…':'创建项目'}</Button></DialogFooter>
 </form></DialogContent></Dialog>;
}
