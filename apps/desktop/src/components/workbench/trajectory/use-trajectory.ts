import {useCallback,useEffect,useMemo,useRef,useState} from 'react';
import {trajectory,hostError} from '../../../host';
import type {TaskSnapshot,TrajectoryRecord,TrajectoryPage} from '../../../../../../packages/host-contract/src';

export function mergeRecords(old:TrajectoryRecord[],incoming:TrajectoryRecord[],prepend=false):TrajectoryRecord[]{
 const replacements=new Map(incoming.map(r=>[r.id,r]));
 const existing=new Set(old.map(r=>r.id));
 const retained=old.map(r=>replacements.get(r.id)??r);
 const added=incoming.filter(r=>!existing.has(r.id));
 return prepend?[...added,...retained]:[...retained,...added];
}
export function projectLive(history:TrajectoryRecord[],live:TrajectoryRecord[]):TrajectoryRecord[]{
 const aliases=new Map<string,TrajectoryRecord>();
 for(const r of history)for(const id of [r.id,...r.aliases])aliases.set(id,r);
 const match=(r:TrajectoryRecord)=>[r.id,...r.aliases].map(id=>aliases.get(id)).find(Boolean);
 const anchor=live.find(r=>r.turn!=null&&match(r)?.turn!=null);
 const offset=anchor?(match(anchor)!.turn!-anchor.turn!):Math.max(0,...history.map(r=>r.turn??0));
 const ids=new Map(live.map(r=>[r.id,match(r)?.id??r.id]));
 const updates:TrajectoryRecord[]=[];
 const firstResident=live.findIndex(r=>!!match(r));
 for(const [index,r] of live.entries()){const stored=match(r);
  // Already-persisted live prefix outside the resident history window is not a new tail.
  if(!stored&&firstResident>=0&&index<firstResident&&r.status!=='running')continue;
  if(stored){updates.push({...r,...stored,aliases:[...new Set([...stored.aliases,r.id,...r.aliases])],startedAt:stored.startedAt??r.startedAt,completedAt:stored.completedAt??r.completedAt,durationMs:stored.durationMs??r.durationMs,ttftMs:stored.ttftMs??r.ttftMs,input:stored.input??r.input,output:stored.output??r.output,status:stored.status==='unknown'?r.status:stored.status,timingSource:stored.durationMs!=null?'omp':r.timingSource});}
  else updates.push({...r,turn:r.turn==null?null:r.turn+offset,parentId:r.parentId?ids.get(r.parentId)??r.parentId:null});
 }
 return mergeRecords(history,updates);
}
const empty=():TrajectoryPage=>({records:[],nextCursor:null,totalRecords:0,revision:'',warnings:[]});
export function useTrajectory(taskId:string,run?:TaskSnapshot){
 const [data,setData]=useState<TrajectoryPage>(empty),[loading,setLoading]=useState(true),[loadingOlder,setLoadingOlder]=useState(false),[error,setError]=useState<ReturnType<typeof hostError>|null>(null);
 const current=useRef(data),epoch=useRef(0),busy=useRef(false),pending=useRef(false),lastAfter=useRef<string|null>(null);
 const read=useCallback(async(mode:'tail'|'older'|'new'):Promise<void>=>{
  if(busy.current){if(mode==='new')pending.current=true;return;}busy.current=true;const version=epoch.current;
  if(mode==='older')setLoadingOlder(true);else if(!current.current.records.length)setLoading(true);
  setError(null);
  try{
   let cursor=mode==='older'?current.current.nextCursor:mode==='new'?lastAfter.current:null;
   if(mode==='older'&&!cursor)return;
   do{
    const page=await trajectory.read(taskId,cursor,mode==='new'&&!!cursor);
    if(epoch.current!==version)return;
    const merged=mode==='tail'?page:{...page,records:mergeRecords(current.current.records,page.records,mode==='older'),nextCursor:mode==='older'?page.nextCursor:current.current.nextCursor};
    current.current=merged;setData(merged);
    if(mode!=='older')lastAfter.current=page.afterCursor??page.records.at(-1)?.id??null;
    const next=page.afterCursor??null;
    if(mode!=='new'||page.records.length<50||!next||next===cursor)break;
    cursor=next;
   }while(true);
  }catch(e){if(epoch.current===version)setError(hostError(e));}
  finally{if(epoch.current===version){busy.current=false;setLoading(false);setLoadingOlder(false);if(pending.current){pending.current=false;void read('new');}}}
 },[taskId]);
 useEffect(()=>{epoch.current++;busy.current=false;pending.current=false;current.current=empty();lastAfter.current=null;setData(current.current);void read('tail');return()=>{epoch.current++;};},[read]);
 // Live deltas arrive over the existing authenticated observer. Disk reconciliation is
 // throttled and only needed when an OMP message/tool has settled, not per text delta.
 const settled=run?.events.filter(e=>['message_end','tool_execution_end','auto_compaction_end'].includes(e.eventType)).at(-1)?.seq;
 useEffect(()=>{const timer=setTimeout(()=>void read('new'),350);return()=>clearTimeout(timer);},[read,run?.runId,run?.status,settled]);
 const records=useMemo(()=>projectLive(data.records,run?.trajectory??[]),[data.records,run?.trajectory]);
 return {...data,records,totalRecords:data.totalRecords+Math.max(0,records.length-data.records.length),loading,loadingOlder,error,hasMore:!!data.nextCursor,refresh:()=>read('tail'),loadOlder:()=>read('older')};
}
