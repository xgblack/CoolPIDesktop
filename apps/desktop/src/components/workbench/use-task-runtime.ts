import {useCallback,useEffect,useRef,useState} from 'react';
import {taskRuntime,hostError} from '@/host';
import type {HostError,RuntimeTaskInfo} from '../../../../../packages/host-contract/src';

export function useTaskRuntime(taskId:string|null){
 const [tasks,setTasks]=useState<RuntimeTaskInfo[]>([]),[error,setError]=useState<HostError|null>(null);
 const active=useRef(false),inFlight=useRef(false);
 const refresh=useCallback(async()=>{
  if(inFlight.current)return;
  inFlight.current=true;
  try{const data=await taskRuntime.list();if(active.current){setTasks(data);setError(null);}}
  catch(e){if(active.current){setError(hostError(e));setTasks(old=>old.map(t=>({...t,status:'unknown'})));}}
  finally{inFlight.current=false;}
 },[]);
 useEffect(()=>{active.current=true;void refresh();const timer=setInterval(refresh,3000);return()=>{active.current=false;clearInterval(timer);};},[refresh]);
 useEffect(()=>{
  let disposed=false;
  const timer=setTimeout(()=>{void taskRuntime.focus(taskId).then(()=>{if(!disposed)void refresh();}).catch(e=>{if(!disposed)setError(hostError(e));});},taskId?300:0);
  return()=>{disposed=true;clearTimeout(timer);};
 },[taskId,refresh]);
 useEffect(()=>()=>{void taskRuntime.focus(null).catch(e=>console.warn('Unable to release runtime focus',hostError(e).code));},[]);
 return {tasks,error,refresh};
}
export function runtimeLabel(info?:RuntimeTaskInfo,pendingApproval=false){
 if(!info)return '未启动';
 if(info.status==='unknown')return '状态未知';
 if(info.owner==='terminal')return '终端接管中';
 if(info.autoStartSuppressed&&info.status==='stopped')return '已手动停止';
 if(pendingApproval)return '等待审批';
 return {starting:'正在启动',ready:'已就绪',idle:'已就绪',running:'生成中',interrupted:'已就绪',stopped:'已停止',failed:'启动失败',external:'终端接管中',unknown:'状态未知'}[info.status];
}
