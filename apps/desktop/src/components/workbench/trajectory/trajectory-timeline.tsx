import { useMemo, useRef, useState } from 'react';
import { durationLabel, overlapping, timelineItems, type TrajectoryRecord } from './trajectory-model';
export function TrajectoryTimeline({records,duration,onFocus,onRange}:{records:TrajectoryRecord[];duration:boolean;onFocus:(id:string)=>void;onRange:(ids:Set<string>|null)=>void}) {
 const items=useMemo(()=>timelineItems(records,duration),[records,duration]);const total=Math.max(1,items.at(-1)?.end??1);
 const [window,setWindow]=useState({start:0,end:1});const [selection,setSelection]=useState<[number,number]|null>(null);
 const gesture=useRef<{x:number;fraction:number;button:number;start:number;end:number}|null>(null);
 const span=window.end-window.start;
 const reset=()=>{setWindow({start:0,end:1});setSelection(null);onRange(null);};
 const zoom=(factor:number,anchor=.5)=>setWindow(w=>{const s=Math.max(.0001,Math.min(1,(w.end-w.start)*factor));const p=w.start+(w.end-w.start)*anchor;const start=Math.max(0,Math.min(1-s,p-s*anchor));return {start,end:start+s};});
 const visible=items.filter(i=>i.end/total>=window.start&&i.start/total<=window.end);
 // Aggregate the overview into screen-sized bins; the ledger retains every record.
 const bins=new Map<string,typeof items[number]>();for(const i of visible){const bin=Math.floor((i.start/total-window.start)/span*1200);const key=`${i.lane}:${bin}`;if(!bins.has(key))bins.set(key,i);}
 return <div className="trajectory-overview"><div className="trajectory-lane-labels"><span>消息</span><span>生成</span><span>工具</span></div><div className="trajectory-timeline" tabIndex={0} role="group" aria-label="轨迹时间概览。加减键缩放，左右键平移，Escape 重置。拖动选择区间。"
 onKeyDown={e=>{if(e.key==='Escape')reset();if(e.key==='+'||e.key==='=')zoom(.7);if(e.key==='-')zoom(1.4);if(e.key==='ArrowLeft'||e.key==='ArrowRight'){e.preventDefault();const start=Math.max(0,Math.min(1-span,window.start+(e.key==='ArrowLeft'?-.1:.1)*span));setWindow({start,end:start+span});}}}
 onWheel={e=>{e.preventDefault();const box=e.currentTarget.getBoundingClientRect();zoom(e.deltaY>0?1.2:.8,(e.clientX-box.left)/box.width);}}
 onContextMenu={e=>e.preventDefault()}
 onPointerDown={e=>{if(e.button!==0&&e.button!==2)return;e.currentTarget.setPointerCapture(e.pointerId);const box=e.currentTarget.getBoundingClientRect();gesture.current={x:e.clientX,fraction:window.start+(e.clientX-box.left)/box.width*span,button:e.button,...window};}}
 onPointerMove={e=>{const g=gesture.current;if(!g)return;const width=e.currentTarget.getBoundingClientRect().width;const delta=(e.clientX-g.x)/width*(g.end-g.start);if(g.button===2){const start=Math.max(0,Math.min(1-(g.end-g.start),g.start-delta));setWindow({start,end:start+g.end-g.start});}else setSelection([g.fraction*total,Math.max(0,Math.min(1,g.fraction+delta))*total]);}}
 onPointerUp={e=>{const g=gesture.current;gesture.current=null;if(!g)return;if(Math.abs(e.clientX-g.x)<3){if(g.button===2)reset();else{const fraction=g.fraction*total;const lane=Math.min(2,Math.floor((e.clientY-e.currentTarget.getBoundingClientRect().top)/16));const item=items.find(i=>i.lane===lane&&i.start<=fraction&&i.end>=fraction)??items.find(i=>i.start<=fraction&&i.end>=fraction);if(item)onFocus(item.record.id);}}else if(g.button===0&&selection)onRange(overlapping(items,...selection));}}
 onPointerCancel={()=>{gesture.current=null;setSelection(null);}}>
 {Array.from(bins.values()).map(i=><span key={i.record.id} className="trajectory-mark" data-kind={i.record.kind} data-status={i.record.status} title={`${i.record.name} · ${durationLabel(i.record.durationMs)}${i.record.ttftMs!=null?` · TTFT ${durationLabel(i.record.ttftMs)}`:''}`} style={{left:`${(i.start/total-window.start)/span*100}%`,width:`max(2px, ${(i.end-i.start)/total/span*100}%)`,top:2+i.lane*16}}>{duration&&i.record.ttftMs!=null&&!!i.record.durationMs&&<i style={{width:`${Math.min(100,i.record.ttftMs/i.record.durationMs*100)}%`}}/>}</span>)}
 {selection&&<span className="trajectory-range" style={{left:`${(Math.min(...selection)/total-window.start)/span*100}%`,width:`${Math.abs(selection[1]-selection[0])/total/span*100}%`}}/>}
 </div><button className="trajectory-reset" onClick={reset} title="重置缩放和区间">重置</button></div>;
}
