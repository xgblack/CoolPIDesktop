import { useMemo, useState } from 'react';
import { AlertCircle, ChevronDown, ChevronRight, RefreshCw, Search, X } from 'lucide-react';
import { useTrajectory } from './use-trajectory';
import { TrajectoryTimeline } from './trajectory-timeline';
import { durationLabel, findRecord, kindLabels, ledgerRows, previewText, searchRecords, statusLabels, stringify, type TrajectoryRecord } from './trajectory-model';
import './trajectory.css';
import type { TaskSnapshot } from '../../../../../../packages/host-contract/src';

export interface TrajectoryViewProps { taskId:string; run?:TaskSnapshot; onError:(error:unknown)=>void }
export function TrajectoryView({taskId,run,onError}:TrajectoryViewProps){
 const data=useTrajectory(taskId,run); const [duration,setDuration]=useState(false); const [query,setQuery]=useState(''); const [collapsed,setCollapsed]=useState<Set<number>>(new Set()); const [foldTools,setFoldTools]=useState(false); const [selected,setSelected]=useState<string|null>(null); const [range,setRange]=useState<Set<string>|null>(null);
 const matches=useMemo(()=>query?searchRecords(data.records,query):null,[data.records,query]); const rows=useMemo(()=>ledgerRows(data.records,collapsed,foldTools,matches,range),[data.records,collapsed,foldTools,matches,range]); const record=findRecord(data.records,selected);
 return <section className="trajectory-root" aria-label="轨迹视图">
  <header className="trajectory-toolbar"><button onClick={()=>setDuration(v=>!v)} aria-pressed={duration}>耗时</button><button onClick={()=>setCollapsed(new Set(collapsed.size?[]:data.records.map((r:TrajectoryRecord)=>r.turn).filter((v:number|null):v is number=>v!=null)))}>{collapsed.size?'展开轮次':'折叠轮次'}</button><button onClick={()=>setFoldTools(v=>!v)} aria-pressed={foldTools}>工具</button><span>{data.totalRecords} 条记录{data.hasMore?' · 已加载':' '}</span><input className="trajectory-search" value={query} onChange={e=>setQuery(e.target.value)} placeholder="搜索记录、参数、结果" aria-label="搜索轨迹"/><button onClick={data.refresh} title="刷新"><RefreshCw size={14}/></button></header>
  <TrajectoryTimeline records={data.records} duration={duration} onFocus={setSelected} onRange={setRange}/>
  {data.error ? <div className="trajectory-state trajectory-error"><AlertCircle size={18}/><span>{data.error.message}</span><button onClick={data.refresh}>重试</button></div> : data.loading&&data.records.length===0 ? <div className="trajectory-state">正在加载轨迹...</div> : data.records.length===0 ? <div className="trajectory-state">暂无轨迹记录</div> : <div className="trajectory-body"><div className="trajectory-ledger">
   {data.hasMore&&<div className="trajectory-loadmore" onClick={data.loadOlder}>{data.loadingOlder?'加载中...':'加载更早记录'}</div>}
   {rows.map(row=>row.type==='turn'?<div key={row.key} className="trajectory-turn" onClick={()=>{const s=new Set(collapsed);if(row.record.turn!=null){s.has(row.record.turn)?s.delete(row.record.turn):s.add(row.record.turn);setCollapsed(s)}}}><ChevronDown size={13}/>{row.label}<span>{row.count}</span></div>:row.type==='step'?<div key={row.key} className="trajectory-step">{row.label}</div>:<div key={row.key} className={`trajectory-row ${selected===row.record.id?'selected':''}`} onClick={()=>setSelected(row.record.id)}><div className="event" style={{paddingLeft:8+row.depth*14}}>{kindLabels[row.record.kind]} · {row.record.name||'未命名'}</div><div className="content">{previewText(row.record.content)||previewText(row.record.output)||statusLabels[row.record.status]}</div></div>)}
  </div>{record&&<aside className="trajectory-details"><button onClick={()=>setSelected(null)} aria-label="关闭详情"><X size={15}/></button><h3>{kindLabels[record.kind]} · {record.name||'未命名'}</h3><div className="trajectory-meta"><span>状态</span><span>{statusLabels[record.status]}</span><span>耗时</span><span>{durationLabel(record.durationMs)}</span><span>TTFT</span><span>{durationLabel(record.ttftMs)}</span>{record.model&&<><span>模型</span><span>{record.model}</span></>}</div>{record.input!==undefined&&<><h4>输入</h4><pre>{stringify(record.input)}</pre></>}{record.output!==undefined&&<><h4>输出</h4><pre>{stringify(record.output)}</pre></>}<h4>内容</h4><pre>{stringify(record.content)||'未记录'}</pre>{record.error&&<pre className="trajectory-error">{record.error}</pre>}</aside>}</div>}
 </section>
}
export default TrajectoryView;
