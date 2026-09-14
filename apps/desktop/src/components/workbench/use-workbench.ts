import {useCallback,useEffect,useRef,useState} from 'react';
import {host,hostError,modelConfig,projects,records} from '../../host';
import {HistoryFeed} from '../../history';
import {applyEvent,mergeSnapshot} from '../../live-events';
import {sendPrompt,type FileReference} from '../../host';
import type {HostEvent,Message} from '../../../../../packages/host-contract/src';
import type {HostError,Project,TaskRecord,TaskSnapshot} from '../../../../../packages/host-contract/src';

const messageText=(m:Message)=>typeof m.content==='string'?m.content:Array.isArray(m.content)?m.content.map(b=>b?.text??'').join(''):'';
const matchesSent=(m:Message,text:string)=>m.role==='user'&&(messageText(m)===text||messageText(m).startsWith(text+'\nReferenced workspace file:'));

export function useWorkbench(){
 const [ps,setProjects]=useState<Project[]>([]),[tasks,setTasks]=useState<TaskRecord[]>([]),[runs,setRuns]=useState<TaskSnapshot[]>([]);
 const [models,setModels]=useState<import('../../../../../packages/host-contract/src').DiscoveredModel[]>([]);
 const [projectId,setProjectId]=useState(''),[taskId,setTaskId]=useState('');
 const [loading,setLoading]=useState(true),[busy,setBusy]=useState(false),[error,setError]=useState<HostError|null>(null);
 const [historyError,setHistoryError]=useState<HostError|null>(null),[historyBusy,setHistoryBusy]=useState(false),[,render]=useState(0);
 const [drafts,setDrafts]=useState<Record<string,string>>({});
 const [attachments,setAttachments]=useState<Record<string,string[]>>({});
 const [references,setReferences]=useState<Record<string,FileReference[]>>({});
 const [outbox,setOutbox]=useState<Record<string,{message:Message;text:string;ids:string[];refs:FileReference[];failed:boolean;baseline:number}>>({});
 const active=useRef(false),generation=useRef(0),operation=useRef(false),selection=useRef('');
 const feed=useRef(new HistoryFeed());
 const refresh=useCallback(async()=>{
  const epoch=generation.current;
  const [p,t,r]=await Promise.all([projects.list(),records.list(),host.list()]);
  if(active.current&&epoch===generation.current){setProjects(p);setTasks(t);setRuns(old=>r.reduce(mergeSnapshot,old));setLoading(false);}
 },[]);
 const [modelsLoading,setModelsLoading]=useState(true),[modelsError,setModelsError]=useState<HostError|null>(null);
 const [projectModels,setProjectModels]=useState<Record<string,string>>({});
 const modelRequest=useRef<{projectId:string;promise:Promise<void>}|null>(null);
 const modelGeneration=useRef(0);
 const [modelsProject,setModelsProject]=useState<string|null>(null);
 const refreshModels=useCallback(()=>{
  if(modelRequest.current?.projectId===projectId)return modelRequest.current.promise;
  const epoch=++modelGeneration.current;
  setModelsLoading(true);setModelsError(null);setModelsProject(null);
  const request=modelConfig.verify(null,null,projectId||null).then(result=>{
   if(active.current&&epoch===modelGeneration.current){
    setModels(result.models);setModelsProject(projectId);
    const available=(value?:string|null)=>!!value&&result.models.some(m=>m.provider+'/'+m.id===value);
    setProjectModels(old=>({...old,[projectId]:available(result.projectModel)?result.projectModel!:available(result.defaultModel)?result.defaultModel!:''}));
   }
  }).catch(e=>{if(active.current&&epoch===modelGeneration.current){setModelsError(hostError(e));setModelsProject(projectId);}}).finally(()=>{
   if(epoch===modelGeneration.current){modelRequest.current=null;if(active.current)setModelsLoading(false);}
  });
  modelRequest.current={projectId,promise:request};return request;
 },[projectId]);
 const selectProjectModel=async(provider:string,id:string)=>{
  const selectedProject=projectId;
  await projects.model(selectedProject,provider,id);
  setProjectModels(old=>({...old,[selectedProject]:provider+'/'+id}));
 };
 useEffect(()=>{void refreshModels();},[refreshModels]);
 useEffect(()=>{active.current=true;let inFlight=false;const poll=async()=>{if(inFlight||operation.current)return;inFlight=true;try{await refresh();}catch(e){if(active.current){setError(hostError(e));setLoading(false);}}finally{inFlight=false;}};void poll();const timer=setInterval(poll,5000);return()=>{active.current=false;feed.current.invalidate();clearInterval(timer);};},[refresh]);
 useEffect(()=>{
  let disposed=false,socket:WebSocket|undefined,retry:ReturnType<typeof setTimeout>,frame=0;
  let queue:HostEvent[]=[];const fetching=new Set<string>();
  const recover=(id:string)=>{if(fetching.has(id))return;fetching.add(id);void host.snapshot(id).then(s=>{if(!disposed)setRuns(old=>mergeSnapshot(old,s));}).catch(e=>{if(!disposed)setError(hostError(e));}).finally(()=>fetching.delete(id));};
  const flush=()=>{frame=0;const events=queue;queue=[];let titleChanged=false;setRuns(old=>{let next=old;for(const event of events){const s=next.find(s=>s.taskId===event.taskId);if(!s||s.runId!==event.runId||event.seq>s.seq+1){recover(event.taskId);continue;}next=next.map(s=>s.taskId===event.taskId?applyEvent(s,event):s);if(event.eventType==='session_info_update')titleChanged=true;if(!['message_update','stderr'].includes(event.eventType))recover(event.taskId);}return next;});if(titleChanged)void refresh();};
  const connect=async()=>{try{const info=await host.observer();if(disposed)return;socket=new WebSocket(info.url);socket.onopen=()=>socket?.send(JSON.stringify({token:info.token}));socket.onmessage=e=>{try{const data=JSON.parse(e.data);if(data.type==='snapshot'){setError(old=>old?.code==='observer_disconnected'?null:old);setRuns(old=>(data.tasks as TaskSnapshot[]).reduce(mergeSnapshot,old));}else if(data.type==='event'){queue.push(data.event);if(!frame)frame=requestAnimationFrame(flush);}else if(data.type==='error')throw new Error(data.code);}catch(e){setError(hostError(e));}};socket.onclose=()=>{if(!disposed){setError({code:'observer_disconnected',message:'实时连接已断开，正在重连'});retry=setTimeout(connect,1000);}};socket.onerror=()=>socket?.close();}catch(e){if(!disposed){setError(hostError(e));retry=setTimeout(connect,2000);}}};
  void connect();return()=>{disposed=true;clearTimeout(retry);cancelAnimationFrame(frame);socket?.close();};
 },[]);
 const act=async(fn:()=>Promise<unknown>)=>{
  if(operation.current)return false;operation.current=true;generation.current++;setBusy(true);setError(null);
  const origin=selection.current;let success=false;
  try{await fn();success=true;}catch(e){if(active.current&&selection.current===origin)setError(hostError(e));}
  finally{operation.current=false;generation.current++;if(active.current){setBusy(false);await refresh().catch(e=>setError(hostError(e)));}}
  return success;
 };
 const chooseTask=(task:TaskRecord)=>{selection.current=task.id;feed.current.reset();setHistoryError(null);setHistoryBusy(false);setError(null);setTaskId(task.id);setProjectId(task.projectId);};
 const fork=async(id:string,timestamp:number)=>{await act(async()=>{const created=await records.fork(id,timestamp);await refresh();if(selection.current===id)chooseTask(created);});};
 const chooseProject=(id:string)=>{if(id===projectId)void refreshModels();selection.current='';feed.current.reset();setHistoryError(null);setHistoryBusy(false);setError(null);setProjectId(id);setTaskId('');};
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
 useEffect(()=>{historyEpoch.current++;feed.current.reset();setHistoryBusy(false);setHistoryError(null);render(n=>n+1);},[taskId]);
 const turn=run?.events.filter(e=>e.eventType==='user_message').at(-1)?.seq??0;
 // History belongs to the persisted OMP session, not to the currently running
 // process. A stopped task must remain readable without starting OMP first.
 useEffect(()=>{if(run?.status==='running'){feed.current.invalidate();setHistoryBusy(false);return;}if(taskId)void history();},[taskId,runId,run?.status,turn]);
 const setDraft=(id:string,value:string)=>setDrafts(old=>({...old,[id]:value}));
 const refreshUsage=async(id:string)=>{
  try { const snapshot=await records.usage(id); if(active.current&&selection.current===id)setRuns(old=>old.map(run=>run.taskId===id&&run.runId===snapshot.runId?snapshot:run)); }
  catch(e){if(active.current&&selection.current===id)setError(hostError(e));}
 };
 const send=async(id:string,retry=false,input?:string,inputRefs?:FileReference[])=>{const previous=retry?outbox[id]:undefined;const text=previous?.text??input??drafts[id]??'';const ids=previous?.ids??attachments[id]??[];if(operation.current||(!text.trim()&&!ids.length))return;
  const pending=previous??{message:{role:'user',content:text,timestamp:Date.now()},text,ids,refs:inputRefs??references[id]??[],failed:false,baseline:feed.current.messages.filter(m=>matchesSent(m,text)).length};
  setOutbox(old=>({...old,[id]:{...pending,failed:false}}));if(!retry){setDrafts(old=>({...old,[id]:''}));setAttachments(old=>({...old,[id]:[]}));}
  if(!retry)setReferences(old=>({...old,[id]:[]}));
  const ok=await act(async()=>{const state=runs.find(r=>r.taskId===id);if(!state||!['ready','idle','interrupted'].includes(state.status)){const resumed=await records.resume(id);setRuns(old=>mergeSnapshot(old,resumed));}const result=await sendPrompt(id,text,ids,pending.refs);setRuns(old=>mergeSnapshot(old,result));});
  if(!ok)setOutbox(old=>({...old,[id]:{...pending,failed:true}}));
 };
 const pending=outbox[taskId];
 const acknowledged=!!pending&&!pending.failed&&feed.current.messages.filter(m=>matchesSent(m,pending.text)).length>pending.baseline;
 useEffect(()=>{if(acknowledged)setOutbox(old=>{const next={...old};delete next[taskId];return next;});},[acknowledged,taskId]);
 const addAttachment=(id:string,resource:string)=>setAttachments(old=>({...old,[id]:[...new Set([...(old[id]??[]),resource])].slice(0,8)}));
 const removeAttachment=(id:string,resource:string)=>setAttachments(old=>({...old,[id]:(old[id]??[]).filter(value=>value!==resource)}));
 return{projects:ps,tasks,runs,models,modelsLoading:modelsLoading||modelsProject!==projectId,modelsError,refreshModels,projectModels,setProjectModels,selectProjectModel,project,task,run,projectId,taskId,chooseTask,chooseProject,fork,loading,busy,error,setError,act,refresh,refreshUsage,history,historyBusy,historyError,messages:pending&&!acknowledged?[...feed.current.messages,pending.message]:feed.current.messages,sendFailed:pending?.failed,cursor:feed.current.cursor,drafts,setDraft,attachments,addAttachment,removeAttachment,send,references,setReferences};
}
