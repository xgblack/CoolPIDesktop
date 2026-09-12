import {useEffect,useLayoutEffect,useMemo,useRef,useState} from 'react';
import {useVirtualizer} from '@tanstack/react-virtual';
import {ChevronDown,ChevronRight,RefreshCw,ArrowDown} from 'lucide-react';
import {Sheet,SheetContent,SheetTitle,SheetDescription} from '@/components/ui/sheet';
import {useTrajectory} from './use-trajectory';
import {TrajectoryTimeline} from './trajectory-timeline';
import {TrajectoryDetails,type ImageCache} from './trajectory-details';
import {durationLabel,findRecord,kindLabels,ledgerRows,previewText,searchRecords,statusLabels} from './trajectory-model';
import type {TaskSnapshot} from '../../../../../../packages/host-contract/src';
import './trajectory.css';
export interface TrajectoryViewProps{taskId:string;run?:TaskSnapshot;onError:(error:unknown)=>void}
export function TrajectoryView(props:TrajectoryViewProps){return <TrajectorySession key={props.taskId} {...props}/>;}
function TrajectorySession({taskId,run,onError}:TrajectoryViewProps){
 const data=useTrajectory(taskId,run);
 const [duration,setDuration]=useState(false),[query,setQuery]=useState(''),[collapsed,setCollapsed]=useState<Set<number>>(new Set()),[foldTools,setFoldTools]=useState(false),[foldedAssistants,setFoldedAssistants]=useState<Set<string>>(new Set()),[selected,setSelected]=useState<string|null>(null),[range,setRange]=useState<Set<string>|null>(null),[following,setFollowing]=useState(true),[narrow,setNarrow]=useState(()=>matchMedia('(max-width: 900px)').matches);
 const follow=useRef(true),cache=useRef<ImageCache>(new Map()),scroll=useRef<HTMLDivElement>(null),anchor=useRef<{key:string;offset:number}|null>(null),focusPending=useRef<string|null>(null);
 useEffect(()=>{const media=matchMedia('(max-width: 900px)');const change=()=>setNarrow(media.matches);media.addEventListener('change',change);return()=>media.removeEventListener('change',change);},[]);
 const matches=useMemo(()=>query.trim()?searchRecords(data.records,query):null,[data.records,query]);
 const rows=useMemo(()=>ledgerRows(data.records,collapsed,foldTools,matches,range,foldedAssistants),[data.records,collapsed,foldTools,matches,range,foldedAssistants]);
 const record=findRecord(data.records,selected);
 const turns=useMemo(()=>[...new Set(data.records.flatMap(r=>r.turn==null?[]:[r.turn]))],[data.records]);
 const allTurns=turns.length>0&&turns.every(t=>collapsed.has(t));
 const rowSize=(index:number)=>rows[index]?.type==='record'?32:rows[index]?.type==='turn'?28:24;
 const virtual=useVirtualizer({count:rows.length,getScrollElement:()=>scroll.current,estimateSize:rowSize,getItemKey:index=>rows[index].key,overscan:10});
 const setFollow=(v:boolean)=>{follow.current=v;setFollowing(v);};
 const lastKey=rows.at(-1)?.key;
 useLayoutEffect(()=>{
  if(!scroll.current||!rows.length)return;
  if(anchor.current&&!data.loadingOlder){const a=anchor.current;anchor.current=null;const idx=rows.findIndex(r=>r.key===a.key);if(idx>=0){const offset=rows.slice(0,idx).reduce((sum,_,i)=>sum+rowSize(i),0);virtual.scrollToOffset(offset-a.offset);}}
  else if(focusPending.current){const idx=rows.findIndex(r=>r.type==='record'&&r.record.id===focusPending.current);if(idx>=0){virtual.scrollToIndex(idx,{align:'center'});focusPending.current=null;}}
  else if(follow.current&&!matches&&!range&&!data.loadingOlder){const frame=requestAnimationFrame(()=>virtual.scrollToOffset(virtual.getTotalSize()));return()=>cancelAnimationFrame(frame);}
 },[lastKey,rows.length,data.loadingOlder,selected]);
 const select=(id:string)=>{const r=findRecord(data.records,id);if(!r)return;setFollow(false);setSelected(r.id);setRange(null);setQuery('');setFoldTools(false);setFoldedAssistants(new Set());setCollapsed(old=>{const s=new Set(old);if(r.turn!=null)s.delete(r.turn);return s;});focusPending.current=r.id;};
 const older=async()=>{if(data.loadingOlder)return;setFollow(false);const el=scroll.current;const first=virtual.getVirtualItems().find(item=>item.end> (el?.scrollTop??0));if(first&&el)anchor.current={key:rows[first.index].key,offset:first.start-el.scrollTop};await data.loadOlder();};
 const tail=()=>{setQuery('');setRange(null);setCollapsed(new Set());setFoldTools(false);setFoldedAssistants(new Set());setFollow(true);requestAnimationFrame(()=>virtual.scrollToOffset(virtual.getTotalSize()));};
 const details=record?<TrajectoryDetails key={record.id} taskId={taskId} record={record} capabilities={run?.runtime?.capabilities} cache={cache.current} onError={onError} onClose={()=>setSelected(null)}/>:null;
 const childCounts=useMemo(()=>{const map=new Map<string,number>();for(const r of data.records)if(r.parentId)map.set(r.parentId,(map.get(r.parentId)??0)+1);return map;},[data.records]);
 return <section className="trajectory-root" aria-label="轨迹视图">
 <div className="trajectory-toolbar" role="toolbar" aria-label="轨迹显示选项">
 <button aria-pressed={duration} title={duration?'使用等宽记录':'使用记录耗时（压缩空闲间隔）'} onClick={()=>setDuration(v=>!v)}>耗时</button>
 <button aria-label={allTurns?'展开轮次':'折叠轮次'} aria-pressed={allTurns} onClick={()=>setCollapsed(new Set(allTurns?[]:turns))}>轮次</button>
 <button aria-label={foldTools?'展开工具调用':'折叠工具调用'} aria-pressed={foldTools} onClick={()=>{setFoldTools(v=>!v);setFoldedAssistants(new Set());}}>工具</button>
 <input type="search" className="trajectory-search" value={query} onChange={e=>{setFollow(false);setQuery(e.target.value);}} placeholder="搜索当前窗口" aria-label="搜索轨迹"/>
 <button onClick={()=>void data.refresh()} aria-label="刷新轨迹" disabled={data.loading}><RefreshCw size={14}/></button></div>
 <TrajectoryTimeline records={data.records} duration={duration} selectedId={selected} range={range} onFocus={select} onRange={ids=>{if(ids){setFollow(false);setRange(ids);const first=data.records.find(r=>ids.has(r.id));if(first){setSelected(first.id);focusPending.current=first.id;}}else{setRange(null);}}}/>
 <div className="trajectory-summary" role="status"><span>{data.records.length} / {data.totalRecords} 条记录{matches?` · ${matches.size} 条匹配`:''}</span>{range&&<button onClick={()=>setRange(null)}>清除区间筛选</button>}{duration&&<span>空闲间隔已压缩</span>}</div>
 {data.warnings.map((w,i)=><p className="trajectory-warning" key={i}>{w}</p>)}
 {data.error&&<div role="alert" className="trajectory-error">{data.error.message}<button onClick={()=>void data.refresh()}>重新加载轨迹</button></div>}
 {data.loading&&!data.records.length?<div className="trajectory-state">正在定位最新轨迹…</div>:!data.records.length?<div className="trajectory-state">暂无轨迹记录</div>:<div className="trajectory-body">
 <div className="trajectory-ledger-wrap">
 {data.hasMore&&<button className="trajectory-loadmore" disabled={data.loadingOlder} onClick={()=>void older()}>{data.loadingOlder?'正在加载更早记录…':'加载更早记录'}</button>}
 <div className="trajectory-columns" aria-hidden="true"><span>事件</span><span>摘要</span><span className="trajectory-token">输入</span><span className="trajectory-token">输出</span><span className="trajectory-token">思考</span><span>耗时</span></div>
 <div ref={scroll} className="trajectory-ledger" role="table" aria-label="轨迹记录表" aria-rowcount={rows.length} onWheel={e=>{if(e.deltaY<0)setFollow(false);}} onKeyDown={e=>{if(['ArrowUp','PageUp','Home'].includes(e.key))setFollow(false);if(e.key==='Home'){e.preventDefault();virtual.scrollToOffset(0);}if(e.key==='End'){e.preventDefault();tail();}}} onScroll={e=>{const el=e.currentTarget;const atEnd=el.scrollHeight-el.scrollTop-el.clientHeight<12;if(atEnd&&!matches&&!range)setFollow(true);else if(el.scrollTop>0)setFollow(false);}} tabIndex={0}>
 {!rows.length&&<div className="trajectory-state">没有匹配记录</div>}
 <div style={{height:virtual.getTotalSize(),position:'relative'}}>{virtual.getVirtualItems().map(item=>{const row=rows[item.index],r=row.record;const dimmed=!!range&&!range.has(r.id);return <div key={row.key} data-index={item.index} style={{position:'absolute',top:0,left:0,width:'100%',height:item.size,transform:`translateY(${item.start}px)`,opacity:dimmed?.28:1,filter:dimmed?'saturate(.45)':undefined,transition:'opacity 120ms ease'}} role="row" aria-rowindex={item.index+1}>
 {row.type==='turn'?<button className="trajectory-turn" aria-expanded={!collapsed.has(r.turn==null?-1:r.turn)} onClick={()=>{setCollapsed(old=>{const s=new Set(old);const key=r.turn==null?-1:r.turn;s.has(key)?s.delete(key):s.add(key);return s;});}}>{collapsed.has(r.turn==null?-1:r.turn)?<ChevronRight size={12}/>:<ChevronDown size={12}/>}<span>{row.label}</span><span className="trajectory-count">{row.count} 条</span></button>:row.type==='step'?<div className="trajectory-step">{row.label}</div>:<div className={`trajectory-row ${record?.id===r.id?'selected':''}`} data-kind={r.kind} data-record-id={r.id}>
 <div className="event" role="cell" style={{paddingLeft:8+row.depth*12}}>{r.kind==='assistant'&&childCounts.has(r.id)&&<button aria-label={`${foldTools||foldedAssistants.has(r.id)?'展开':'折叠'} ${r.name} 工具`} onClick={()=>{setFoldTools(false);setFoldedAssistants(old=>{const s=new Set(old);s.has(r.id)?s.delete(r.id):s.add(r.id);return s;});}}>{foldTools||foldedAssistants.has(r.id)?<ChevronRight size={12}/>:<ChevronDown size={12}/>}</button>}<span className="trajectory-kind">{kindLabels[r.kind]}</span></div>
 <button className="content" role="cell" aria-label={`检查 ${r.name} ${r.id}`} onClick={()=>{setFollow(false);setSelected(r.id);}}><span className="trajectory-record-name">{r.kind==='tool'?r.name+' · ':''}</span>{previewText(r.content)||previewText(r.output)||(r.images.length?`${r.images.length} 张图片`:statusLabels[r.status])}<span className="trajectory-status" data-status={r.status}>{statusLabels[r.status]}</span></button>
 <span role="cell" className="trajectory-token">{r.usage?.inputTokens??'—'}</span><span role="cell" className="trajectory-token">{r.usage?.outputTokens??'—'}</span><span role="cell" className="trajectory-token">{r.usage?.reasoningTokens??'—'}</span><span role="cell" className="trajectory-duration">{durationLabel(r.status==='running'?null:r.durationMs)}</span></div>}
 </div>;})}</div></div>
 {!following&&<button className="trajectory-tail" onClick={tail}><ArrowDown size={12}/>回到底部</button>}
 </div>{!narrow&&details&&<aside aria-label="轨迹记录详情">{details}</aside>}</div>}
 {narrow&&<Sheet open={!!record} onOpenChange={open=>{if(!open)setSelected(null);}}><SheetContent className="trajectory-sheet" showCloseButton={false}><SheetTitle className="sr-only">轨迹记录详情</SheetTitle><SheetDescription className="sr-only">选中记录的内容、用量、计时与附件</SheetDescription>{details}</SheetContent></Sheet>}
 </section>;
}
export default TrajectoryView;
