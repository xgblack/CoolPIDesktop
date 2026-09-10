import {useCallback,useEffect,useRef,useState} from 'react';
import {host,hostError,projects,records} from '../../host';
import {HistoryFeed} from '../../history';
import type {HostError,Project,TaskRecord,TaskSnapshot} from '../../../../../packages/host-contract/src';

export function useWorkbench(){
 const [ps,setProjects]=useState<Project[]>([]),[tasks,setTasks]=useState<TaskRecord[]>([]),[runs,setRuns]=useState<TaskSnapshot[]>([]);
 const [projectId,setProjectId]=useState(''),[taskId,setTaskId]=useState('');
 const [loading,setLoading]=useState(true),[busy,setBusy]=useState(false),[error,setError]=useState<HostError|null>(null);
 const [historyError,setHistoryError]=useState<HostError|null>(null),[historyBusy,setHistoryBusy]=useState(false),[,render]=useState(0);
 const [drafts,setDrafts]=useState<Record<string,string>>({});
 const active=useRef(false),generation=useRef(0),operation=useRef(false),selection=useRef('');
 const feed=useRef(new HistoryFeed());
 const refresh=useCallback(async()=>{
  const epoch=generation.current;
  const [p,t,r]=await Promise.all([projects.list(),records.list(),host.list()]);
  if(active.current&&epoch===generation.current){setProjects(p);setTasks(t);setRuns(r);setLoading(false);}
 },[]);
 useEffect(()=>{active.current=true;let inFlight=false;const poll=async()=>{if(inFlight||operation.current)return;inFlight=true;try{await refresh();}catch(e){if(active.current){setError(hostError(e));setLoading(false);}}finally{inFlight=false;}};void poll();const timer=setInterval(poll,800);return()=>{active.current=false;feed.current.invalidate();clearInterval(timer);};},[refresh]);
 const act=async(fn:()=>Promise<unknown>)=>{
  if(operation.current)return false;operation.current=true;generation.current++;setBusy(true);setError(null);
  const origin=selection.current;let success=false;
  try{await fn();success=true;}catch(e){if(active.current&&selection.current===origin)setError(hostError(e));}
  finally{operation.current=false;generation.current++;if(active.current){setBusy(false);await refresh().catch(e=>setError(hostError(e)));}}
  return success;
 };
 const chooseTask=(task:TaskRecord)=>{selection.current=task.id;feed.current.reset();setHistoryError(null);setHistoryBusy(false);setError(null);setTaskId(task.id);setProjectId(task.projectId);};
 const chooseProject=(id:string)=>{selection.current='';feed.current.reset();setHistoryError(null);setHistoryBusy(false);setError(null);setProjectId(id);setTaskId('');};
 const task=tasks.find(t=>t.id===taskId),run=runs.find(r=>r.taskId===taskId),project=ps.find(p=>p.id===projectId);
 const runId=run?.runId;
 const historyEpoch=useRef(0);
 const history=async(more=false)=>{
  const scope=selection.current;if(!scope)return;const epoch=++historyEpoch.current;
  setHistoryBusy(true);setHistoryError(null);
  try{await feed.current.load(cursor=>records.history(scope,cursor),more,()=>{if(active.current&&scope===selection.current)render(n=>n+1);});}
  catch(e){if(active.current&&scope===selection.current&&epoch===historyEpoch.current)setHistoryError(hostError(e));}
  finally{if(active.current&&scope===selection.current&&epoch===historyEpoch.current)setHistoryBusy(false);}
 };
 useEffect(()=>{historyEpoch.current++;feed.current.reset();setHistoryBusy(false);setHistoryError(null);render(n=>n+1);},[taskId,runId]);
 const turn=run?.events.filter(e=>e.eventType==='user_message').at(-1)?.seq??0;
 useEffect(()=>{if(!run)return;if(run.status==='running'){feed.current.invalidate();setHistoryBusy(false);}else if(['ready','idle','interrupted'].includes(run.status)){void history();}},[taskId,runId,run?.status,turn]);
 const setDraft=(id:string,value:string)=>setDrafts(old=>({...old,[id]:value}));
 const send=async(id:string)=>{const text=drafts[id]??'';if(!text.trim())return;await act(async()=>{await host.prompt(id,text);setDrafts(old=>old[id]===text?{...old,[id]:''}:old);});};
 return{projects:ps,tasks,runs,project,task,run,projectId,taskId,chooseTask,chooseProject,loading,busy,error,setError,act,refresh,history,historyBusy,historyError,messages:feed.current.messages,cursor:feed.current.cursor,drafts,setDraft,send};
}
