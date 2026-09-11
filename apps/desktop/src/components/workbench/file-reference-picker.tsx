import {useEffect,useState} from 'react';
import {ArrowLeft,File,Folder} from 'lucide-react';
import {resources,hostError} from '@/host';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';
import {Button} from '@/components/ui/button';
import {ErrorNotice} from './shared';
import type {DirectoryPage,HostError} from '../../../../../packages/host-contract/src';

/** References identify a trusted task root and a relative path; no file contents are copied. */
export function FileReferencePicker({taskId,roots,onSelect,onClose}:{taskId:string;roots:string[];onSelect:(reference:string)=>void;onClose:()=>void}) {
 const [root,setRoot]=useState(0),[path,setPath]=useState(''),[page,setPage]=useState<DirectoryPage>(),[error,setError]=useState<HostError>();
 useEffect(()=>{let active=true;setPage(undefined);setError(undefined);void resources.list(taskId,root,path).then(v=>{if(active)setPage(v);}).catch(e=>{if(active)setError(hostError(e));});return()=>{active=false;};},[taskId,root,path]);
 return <Dialog open onOpenChange={v=>{if(!v)onClose();}}><DialogContent className="file-reference-dialog"><DialogHeader><DialogTitle>引用工作区文件</DialogTitle><DialogDescription>插入目录编号和相对路径，文件内容由 Agent 按任务需要读取。</DialogDescription></DialogHeader>
  <label className="text-xs">任务目录<select className="reference-root" aria-label="引用文件根目录" value={root} onChange={e=>{setRoot(Number(e.target.value));setPath('');}}>{roots.map((p,i)=><option key={i} value={i}>{i===0?'主目录':`附加目录 ${i}`} · {p}</option>)}</select></label>
  <div className="flex items-center gap-2"><Button variant="ghost" size="icon-sm" aria-label="返回引用上级目录" disabled={!path} onClick={()=>setPath(path.split('/').slice(0,-1).join('/'))}><ArrowLeft/></Button><code className="truncate text-xs">{path||'/'}</code></div>
  <ErrorNotice error={error}/><div className="reference-files">{!page&&!error?<p role="status">正在加载文件…</p>:page?.entries.length===0?<p>此目录为空</p>:page?.entries.map(entry=><button key={entry.path} disabled={!['directory','file'].includes(entry.kind)} onClick={()=>entry.kind==='directory'?setPath(entry.path):onSelect(`[${root===0?'主目录':`附加目录 ${root}`}: ${JSON.stringify(entry.path)}]`)}>{entry.kind==='directory'?<Folder size={15}/>:<File size={15}/>}<span>{entry.name}</span></button>)}</div>{page?.truncated&&<p className="text-xs text-muted-foreground">只显示前 1000 项，请进入子目录查找。</p>}
 </DialogContent></Dialog>;
}
