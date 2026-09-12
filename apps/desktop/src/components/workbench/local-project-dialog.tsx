import {useState} from 'react';
import {Folder,FolderPlus,X} from 'lucide-react';
import {localProjects,hostError} from '@/host';
import type {Project,HostError} from '../../../../../packages/host-contract/src';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription,DialogFooter} from '@/components/ui/dialog';
import {Button} from '@/components/ui/button';
import {Input} from '@/components/ui/input';
import {Checkbox} from '@/components/ui/checkbox';
import {ErrorNotice} from './shared';

export function LocalProjectDialog({project,onClose,onCreated}:{project?:Project;onClose:()=>void;onCreated:(project:Project)=>Promise<void>}){
 const [name,setName]=useState(project?.name??''),[folders,setFolders]=useState<{token:string;path:string;existing?:number}[]>(project?.roots.map((path,existing)=>({token:`existing:${existing}`,path,existing}))??[]),[trusted,setTrusted]=useState(project?.trusted??false),[busy,setBusy]=useState(false),[error,setError]=useState<HostError|null>(null);
 const choose=async()=>{setBusy(true);setError(null);try{const selected=await localProjects.choose();setFolders(old=>[...old,...selected.filter(f=>!old.some(o=>o.path===f.path))]);}catch(e){setError(hostError(e));}finally{setBusy(false);}};
 return <Dialog open onOpenChange={open=>{if(!open&&!busy)onClose();}}><DialogContent><DialogHeader><DialogTitle>{project?'编辑项目':'创建项目'}</DialogTitle><DialogDescription className="sr-only">本地项目文件夹</DialogDescription></DialogHeader><form onSubmit={e=>{e.preventDefault();if(busy||!name.trim()||!folders.length||!trusted)return;setBusy(true);setError(null);void (project?localProjects.edit(project.id,name.trim(),folders.map(f=>f.existing!==undefined?{existing:f.existing}:{selected:f.token}),trusted):localProjects.create(name.trim(),folders.map(f=>f.token),trusted)).then(onCreated).then(onClose).catch(e=>setError(hostError(e))).finally(()=>setBusy(false));}}>
  <label htmlFor="project-name">项目名称</label><Input id="project-name" autoFocus value={name} maxLength={160} disabled={busy} onChange={e=>setName(e.target.value)} className="my-3"/>
  <h3>源文件夹</h3>{project&&<p className="text-xs text-muted-foreground mt-2">目录配置用于新对话，已有任务保留原目录。</p>}<div className="project-folders">{folders.map((folder,index)=><div className="project-folder-row" key={folder.token}><Folder size={18}/><span title={folder.path}>{folder.path.split('/').filter(Boolean).at(-1)}</span>{index===0?<span className="primary-folder-badge">主要</span>:<Button className="set-primary-folder" type="button" size="sm" variant="secondary" disabled={busy} onClick={()=>setFolders(old=>[folder,...old.filter(f=>f.token!==folder.token)])}>设为主要</Button>}<Button type="button" size="icon-sm" variant="ghost" disabled={busy} aria-label={`移除 ${folder.path}`} onClick={()=>setFolders(old=>old.filter(f=>f.token!==folder.token))}><X/></Button></div>)}<Button type="button" variant="ghost" disabled={busy} onClick={()=>void choose()}><FolderPlus/>添加文件夹</Button></div>
  <label className="trust-label"><Checkbox checked={trusted} disabled={busy} onCheckedChange={v=>setTrusted(v===true)}/>信任这些目录中的配置与扩展</label><ErrorNotice error={error}/><DialogFooter><Button type="button" variant="ghost" disabled={busy} onClick={onClose}>取消</Button><Button disabled={busy||!name.trim()||!folders.length||!trusted}>{busy?'处理中…':project?'保存':'创建项目'}</Button></DialogFooter>
 </form></DialogContent></Dialog>;
}
