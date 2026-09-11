import {useEffect,useRef,useState} from 'react';
import {GitCommitHorizontal,Plus,Minus,Undo2} from 'lucide-react';
import {gitWrites,host,hostError} from '@/host';
import {Button} from '@/components/ui/button';
import {Dialog,DialogContent,DialogTitle,DialogDescription} from '@/components/ui/dialog';
import {Textarea} from '@/components/ui/textarea';
import {ErrorNotice,IconButton} from './shared';
import type {CommitPreview,GitStatus,HostError,TaskRecord} from '../../../../../packages/host-contract/src';

export function GitWrites({task,root,status,onChanged}:{task:TaskRecord;root:number;status:GitStatus;onChanged:()=>void}) {
 const [writable,setWritable]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState<HostError>(),[preview,setPreview]=useState<CommitPreview>(),[discard,setDiscard]=useState<string>(),[message,setMessage]=useState(''),[commit,setCommit]=useState('');
 const alive=useRef(true),lock=useRef(false);
 useEffect(()=>{alive.current=true;void host.roots(task.id).then(roots=>{if(alive.current)setWritable(roots[root]?.mode==='isolated')}).catch(e=>{if(alive.current)setError(hostError(e))});return()=>{alive.current=false}},[task.id,root]);
 const act=async(f:()=>Promise<void>)=>{if(lock.current)return;lock.current=true;setBusy(true);setError(undefined);try{await f()}catch(e){if(alive.current)setError(hostError(e))}finally{lock.current=false;if(alive.current)setBusy(false)}};
 const change=(path:string,action:'stage'|'unstage'|'discard')=>act(async()=>{await gitWrites.change(task.id,root,path,action,action==='discard');if(alive.current){setDiscard(undefined);onChanged()}});
 if(!writable)return <ErrorNotice error={error}/>;
 return <div className="mt-3 border-t pt-3"><ErrorNotice error={error}/>{commit&&<p role="status" className="break-all text-xs">已提交 {commit}</p>}
  {status.changes.map(c=><div key={c.path} className="flex min-w-0 items-center gap-1 py-1"><code className="min-w-0 flex-1 truncate text-xs" title={c.path}>{c.path}</code>
   <IconButton label={`暂存 ${c.path}`} disabled={busy||task.archived||!c.worktreeStatus||c.kind==='conflicted'} onClick={()=>void change(c.path,'stage')}><Plus size={14}/></IconButton>
   <IconButton label={`取消暂存 ${c.path}`} disabled={busy||task.archived||!c.indexStatus||c.indexStatus==='?'||c.kind==='conflicted'} onClick={()=>void change(c.path,'unstage')}><Minus size={14}/></IconButton>
   <IconButton label={`撤销未暂存修改 ${c.path}`} disabled={busy||task.archived||c.worktreeStatus!=='M'||!!c.originalPath||c.kind==='conflicted'} onClick={()=>setDiscard(c.path)}><Undo2 size={14}/></IconButton>
  </div>)}
  <Button size="sm" variant="outline" disabled={busy||task.archived||!status.changes.some(c=>c.indexStatus&&c.indexStatus!=='?')} onClick={()=>void act(async()=>{const value=await gitWrites.preview(task.id,root);if(alive.current)setPreview(value)})}><GitCommitHorizontal size={14}/>提交暂存改动</Button>
  <Dialog open={!!discard} onOpenChange={open=>{if(!open&&!busy)setDiscard(undefined)}}><DialogContent><DialogTitle>撤销未暂存修改</DialogTitle><DialogDescription className="break-all">{discard} 的当前文件将移入系统回收站，再恢复为暂存版本。暂存内容不变。</DialogDescription><Button disabled={busy} variant="destructive" onClick={()=>discard&&void change(discard,'discard')}>移入回收站并恢复</Button></DialogContent></Dialog>
  <Dialog open={!!preview} onOpenChange={open=>{if(!open&&!busy)setPreview(undefined)}}><DialogContent><DialogTitle>确认提交</DialogTitle><DialogDescription className="break-all">{preview?.root}<br/>分支：{preview?.branch}</DialogDescription><ul className="max-h-48 overflow-auto text-xs">{preview?.paths.map(p=><li className="break-all" key={p}>{p}</li>)}</ul><Textarea aria-label="提交信息" value={message} maxLength={16384} disabled={busy} onChange={e=>setMessage(e.target.value)}/><ErrorNotice error={error}/><Button disabled={busy||!message.trim()} onClick={()=>preview&&void act(async()=>{const sha=await gitWrites.commit(task.id,root,message,preview);if(alive.current){setCommit(sha);setPreview(undefined);setMessage('');onChanged()}})}>确认提交</Button></DialogContent></Dialog>
 </div>;
}
