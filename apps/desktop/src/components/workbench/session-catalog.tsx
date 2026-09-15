import {useCallback,useEffect,useRef,useState} from 'react';
import {Archive,ArchiveRestore,Clock3,GitFork,History,LoaderCircle,LockKeyhole,RefreshCw,Search,Workflow} from 'lucide-react';
import type {HostError,SessionCatalogEntry,SessionCatalogPage,SessionCatalogRefresh} from '../../../../../packages/host-contract/src';
import {sessionCatalog} from '../../host';
import {Button} from '@/components/ui/button';
import {Checkbox} from '@/components/ui/checkbox';
import {Dialog,DialogContent,DialogDescription,DialogHeader,DialogTitle} from '@/components/ui/dialog';
import {ErrorNotice} from './shared';

type CatalogService=typeof sessionCatalog;
const PAGE_SIZE=50;

function asError(value:unknown):HostError{
 const candidate=value as Partial<HostError>|undefined;
 return {code:candidate?.code??'session_catalog_failed',message:candidate?.message??String(value),suggestion:candidate?.suggestion};
}
function relativeTime(value:number){
 const seconds=Math.max(0,Math.round(Date.now()/1000-value));
 if(seconds<60)return '刚刚';if(seconds<3600)return `${Math.floor(seconds/60)} 分钟前`;if(seconds<86400)return `${Math.floor(seconds/3600)} 小时前`;if(seconds<604800)return `${Math.floor(seconds/86400)} 天前`;
 return new Date(value*1000).toLocaleDateString('zh-CN');
}
function relationIcon(entry:SessionCatalogEntry){return entry.relationKind==='subagent'?<Workflow/>:entry.relationKind==='fork'?<GitFork/>:<History/>;}

export function SessionCatalog({open,onOpenChange,onOpenTask,service=sessionCatalog}:{open:boolean;onOpenChange:(value:boolean)=>void;onOpenTask:(taskId:string)=>void;service?:CatalogService}){
 const [query,setQuery]=useState(''),[archived,setArchived]=useState(false),[page,setPage]=useState<SessionCatalogPage>({entries:[],total:0,nextOffset:null});
 const [busy,setBusy]=useState(false),[error,setError]=useState<HostError|null>(null),[notice,setNotice]=useState<SessionCatalogRefresh|null>(null);
 const sequence=useRef(0);
 const load=useCallback(async(reset=true)=>{const current=++sequence.current;setBusy(true);setError(null);try{const offset=reset?0:(page.nextOffset??0);const next=await service.list(query,archived,offset,PAGE_SIZE);if(current!==sequence.current)return;setPage(old=>reset?next:{...next,entries:[...old.entries,...next.entries]});}catch(value){if(current===sequence.current)setError(asError(value));}finally{if(current===sequence.current)setBusy(false);}},[archived,page.nextOffset,query,service]);
 useEffect(()=>{if(!open)return;const timer=window.setTimeout(()=>void load(true),query?180:0);return()=>window.clearTimeout(timer);},[open,query,archived]);
 const refresh=async(rebuild=false)=>{setBusy(true);setError(null);try{const result=await service.refresh(rebuild);setNotice(result);await load(true);}catch(value){setError(asError(value));setBusy(false);}};
 const toggleArchive=async(entry:SessionCatalogEntry)=>{setBusy(true);setError(null);try{await service.updateView(entry.sessionKey,!entry.archived);await load(true);}catch(value){setError(asError(value));setBusy(false);}};
 const openEntry=async(entry:SessionCatalogEntry)=>{if(!entry.taskId)return;try{await service.updateView(entry.sessionKey,null);onOpenTask(entry.taskId);onOpenChange(false);}catch(value){setError(asError(value));}};
 return <Dialog open={open} onOpenChange={onOpenChange}><DialogContent className="session-catalog-dialog" aria-describedby="session-catalog-description">
  <DialogHeader><DialogTitle>会话目录</DialogTitle><DialogDescription id="session-catalog-description">系统 OMP 与当前工作台中的会话索引</DialogDescription></DialogHeader>
  <div className="session-catalog-toolbar"><label><Search/><input aria-label="搜索所有会话" placeholder="搜索标题、工作目录和已信任会话正文" value={query} onChange={event=>setQuery(event.target.value)}/></label><label className="session-catalog-archive-toggle"><Checkbox checked={archived} onCheckedChange={value=>setArchived(value===true)}/>显示归档</label><Button variant="ghost" size="icon-sm" aria-label="刷新会话目录" disabled={busy} onClick={()=>void refresh(false)}>{busy?<LoaderCircle className="animate-spin"/>:<RefreshCw/>}</Button></div>
  {notice&&<p role="status" className="session-catalog-notice">发现 {notice.discovered} 个会话，更新 {notice.indexed} 个，跳过 {notice.unchanged} 个{notice.invalid?`，${notice.invalid} 个需检查`:''}</p>}
  <ErrorNotice error={error}/>
  <div className="session-catalog-list" aria-label="会话搜索结果">
   {!busy&&!page.entries.length&&<div className="session-catalog-empty"><History/><strong>{query?'没有匹配的会话':'尚未建立会话索引'}</strong><Button variant="outline" onClick={()=>void refresh(false)}>扫描系统 OMP 会话</Button></div>}
   {page.entries.map(entry=><article key={entry.sessionKey} className="session-catalog-row" data-unread={entry.unread||undefined}>
    <button className="session-catalog-open" disabled={!entry.taskId} onClick={()=>void openEntry(entry)} aria-label={entry.taskId?`打开 ${entry.title}`:`${entry.title} 尚未绑定到工作台任务`}>
     <span className="session-catalog-kind">{relationIcon(entry)}</span><span className="session-catalog-summary"><strong>{entry.title}</strong>{entry.snippet&&<span className="session-catalog-snippet">{entry.snippet}</span>}<span className="session-catalog-path">{entry.cwd||'未知工作目录'}</span></span>
    </button>
    <div className="session-catalog-meta"><span className={`catalog-status catalog-status-${entry.status}`}>{entry.running?'运行中':entry.status==='pending'?'待处理':entry.status==='complete'?'已完成':entry.status==='error'?'错误':entry.status==='interrupted'?'已中断':'未知'}</span><span><Clock3/>{relativeTime(entry.updatedAt)}</span><span>{entry.messageCount} 条消息</span>{entry.modelId&&<span>{entry.modelProvider?`${entry.modelProvider}/`:''}{entry.modelId}</span>}{!entry.trusted&&<span title="未信任目录仅展示元数据"><LockKeyhole/>只读元数据</span>}{entry.parseState!=='ready'&&<span>索引不完整</span>}</div>
    <Button variant="ghost" size="icon-sm" aria-label={entry.archived?'恢复会话目录项':'归档会话目录项'} disabled={busy||entry.running} onClick={()=>void toggleArchive(entry)}>{entry.archived?<ArchiveRestore/>:<Archive/>}</Button>
   </article>)}
   {page.nextOffset!=null&&<Button className="session-catalog-more" variant="ghost" disabled={busy} onClick={()=>void load(false)}>加载更多</Button>}
  </div>
  <footer className="session-catalog-footer"><span>{page.total} 个会话</span><span>外部会话不会自动创建任务或启动 OMP</span></footer>
 </DialogContent></Dialog>;
}
