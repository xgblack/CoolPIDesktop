import {useEffect,useMemo,useRef,useState} from 'react';
import {Tooltip,TooltipContent,TooltipTrigger} from '@/components/ui/tooltip';
import {durationLabel,overlapping,timelineItems,type TrajectoryRecord} from './trajectory-model';
export function TrajectoryTimeline({records,duration,selectedId=null,range=null,onFocus,onRange}:{records:TrajectoryRecord[];duration:boolean;selectedId?:string|null;range?:Set<string>|null;onFocus:(id:string)=>void;onRange:(ids:Set<string>|null)=>void}){
 const items=useMemo(()=>timelineItems(records,duration),[records,duration]);
 const total=Math.max(1,...items.map(i=>i.end));
 const [window,setWindow]=useState({start:0,end:1}),[selection,setSelection]=useState<[number,number]|null>(null);
 const [hover,setHover]=useState<number|null>(null);
 const element=useRef<HTMLDivElement>(null),gesture=useRef<{x:number;start:number;end:number;fraction:number;button:number}|null>(null);
 const span=window.end-window.start;
 const clear=()=>{setSelection(null);onRange(null);};
 const reset=()=>{setWindow({start:0,end:1});clear();};
 const zoom=(factor:number,anchor=.5)=>setWindow(w=>{const s=Math.max(.0001,Math.min(1,(w.end-w.start)*factor));const p=w.start+(w.end-w.start)*anchor;const start=Math.max(0,Math.min(1-s,p-s*anchor));return{start,end:start+s};});
 useEffect(()=>{reset();},[duration]);
 useEffect(()=>{if(!range)setSelection(null);},[range,selectedId]);
 useEffect(()=>{const item=items.find(i=>i.record.id===selectedId);if(!item)return;setWindow(w=>{if(item.end/total>=w.start&&item.start/total<=w.end)return w;const span=w.end-w.start;const start=Math.max(0,Math.min(1-span,item.start/total));return{start,end:start+span};});},[selectedId,items,total]);
 useEffect(()=>{const el=element.current;if(!el)return;const wheel=(e:WheelEvent)=>{e.preventDefault();const box=el.getBoundingClientRect();if(Math.abs(e.deltaX)>Math.abs(e.deltaY)||e.shiftKey){const delta=(e.deltaX||e.deltaY)*(e.deltaMode===1?16:e.deltaMode===2?box.width:1);setWindow(w=>{const span=w.end-w.start;const start=Math.max(0,Math.min(1-span,w.start+delta/box.width*span));return{start,end:start+span};});}else if(e.deltaY!==0)zoom(e.deltaY>0?1.2:.8,(e.clientX-box.left)/box.width);};el.addEventListener('wheel',wheel,{passive:false});return()=>el.removeEventListener('wheel',wheel);},[]);
 const visible=items.filter(i=>i.end/total>=window.start&&i.start/total<=window.end);
 // Dense marks are coalesced only for painting; hit testing and selection use every record.
 const active=selection?overlapping(items,...selection):range;
 const bins=new Map<string,typeof items[number]>();for(const i of visible){const key=i.record.id===selectedId?`selected:${selectedId}`:`${i.lane}:${active?.has(i.record.id)}:${Math.floor((i.start/total-window.start)/span*1200)}`;const previous=bins.get(key);if(!previous||i.end>previous.end)bins.set(key,i);}
 const fraction=(x:number)=>{const box=element.current!.getBoundingClientRect();return Math.max(0,Math.min(1,window.start+(x-box.left)/box.width*span));};
 return <div className="trajectory-overview"><div className="trajectory-lane-labels"><span>消息</span><span>生成</span><span>工具</span></div><div ref={element} className="trajectory-timeline" data-window-start={window.start} data-window-span={span} tabIndex={0} role="group" aria-label="轨迹时间概览：加减键缩放，左右键平移，拖动选择区间，右键清除区间"
 onKeyDown={e=>{if(e.key==='Escape')reset();if(e.key==='+'||e.key==='='){e.preventDefault();zoom(.7);}if(e.key==='-'){e.preventDefault();zoom(1.4);}if(e.key==='ArrowLeft'||e.key==='ArrowRight'){e.preventDefault();const start=Math.max(0,Math.min(1-span,window.start+(e.key==='ArrowLeft'?-.1:.1)*span));setWindow({start,end:start+span});}}}
 onContextMenu={e=>e.preventDefault()}
 onPointerDown={e=>{if(e.button!==0&&e.button!==2)return;e.currentTarget.setPointerCapture(e.pointerId);gesture.current={x:e.clientX,fraction:fraction(e.clientX),button:e.button,...window};}}
 onPointerLeave={()=>setHover(null)}
 onPointerMove={e=>{setHover(fraction(e.clientX)*total);const g=gesture.current;if(!g)return;const delta=(e.clientX-g.x)/e.currentTarget.getBoundingClientRect().width*(g.end-g.start);if(g.button===2){const start=Math.max(0,Math.min(1-(g.end-g.start),g.start-delta));setWindow({start,end:start+g.end-g.start});}else if(Math.abs(e.clientX-g.x)>=3)setSelection([g.fraction*total,fraction(e.clientX)*total]);}}
 onPointerUp={e=>{
  const g=gesture.current;gesture.current=null;if(!g)return;
  const click=Math.abs(e.clientX-g.x)<3;
  if(g.button===2){if(click)clear();return;}
  const point=g.fraction*total;
  if(click){
   const lane=Math.min(2,Math.floor((e.clientY-e.currentTarget.getBoundingClientRect().top)/16));
   const tolerance=total*span*2/e.currentTarget.getBoundingClientRect().width;
   const hit=items.filter(i=>i.lane===lane&&i.start-tolerance<=point&&i.end+tolerance>=point).sort((a,b)=>Math.abs(a.start-point)-Math.abs(b.start-point))[0];
   if(hit){clear();onFocus(hit.record.id);return;}
  }
  const end=fraction(e.clientX)*total;
  const minimum=total*span*8/e.currentTarget.getBoundingClientRect().width;
  const center=(point+end)/2;
  const selected:[number,number]=Math.abs(end-point)<minimum?[Math.max(0,center-minimum/2),Math.min(total,center+minimum/2)]:[point,end];
  setSelection(selected);onRange(overlapping(items,...selected));
 }}
 onPointerCancel={()=>{gesture.current=null;clear();}}>
 {Array.from(bins.values()).map(i=><Tooltip key={i.record.id} delayDuration={500}><TooltipTrigger asChild><span className="trajectory-mark" data-record-id={i.record.id} data-selected={i.record.id===selectedId} data-dimmed={active!=null&&!active.has(i.record.id)} data-kind={i.record.kind} data-status={i.record.status} style={{left:`${(i.start/total-window.start)/span*100}%`,width:`max(2px, ${(i.end-i.start)/total/span*100}%)`,top:2+i.lane*16}}>{duration&&i.record.ttftMs!=null&&!!i.record.durationMs&&i.record.status!=='running'&&<i style={{width:`${Math.min(100,i.record.ttftMs/i.record.durationMs*100)}%`}}/>}</span></TooltipTrigger><TooltipContent><div>{i.record.name}</div><div>开始：{i.record.startedAt==null?'未记录':new Date(i.record.startedAt).toISOString()}</div><div>耗时：{durationLabel(i.record.status==='running'?null:i.record.durationMs)} · TTFT：{durationLabel(i.record.ttftMs)}</div></TooltipContent></Tooltip>)}
 {selection&&<span className="trajectory-range" style={{left:`${(Math.min(...selection)/total-window.start)/span*100}%`,width:`${Math.abs(selection[1]-selection[0])/total/span*100}%`}}/>}
 {hover!=null&&<span className="trajectory-crosshair" style={{left:`${(hover/total-window.start)/span*100}%`}}><span style={{transform:hover/total>window.start+span*.75?'translateX(-100%)':undefined}}>{duration?durationLabel(hover):`#${Math.floor(hover)+1}`}</span></span>}
 </div><button className="trajectory-reset" onClick={reset}>重置</button></div>;
}
