import {useState} from 'react';
import {RefreshCw} from 'lucide-react';
import type {HostError,RuntimeTaskInfo,TaskRecord} from '../../../../../packages/host-contract/src';
import {taskRuntime,hostError} from '@/host';
import {Button} from '@/components/ui/button';
import {ErrorNotice} from './shared';
import {runtimeLabel} from './use-task-runtime';

export function RuntimeManagement({all,tasks,error,onRefresh,onOpenTask}:{all:RuntimeTaskInfo[];tasks:TaskRecord[];error:HostError|null;onRefresh:()=>Promise<void>;onOpenTask?: (task:TaskRecord)=>void}){
 const [busy,setBusy]=useState(false),[localError,setError]=useState<HostError|null>(null),[notice,setNotice]=useState('');
 const visible=all.filter(t=>t.pid||['starting','failed','unknown'].includes(t.status));
 const perform=async(action:()=>Promise<void>)=>{setBusy(true);setError(null);setNotice('');try{await action();}catch(e){setError(hostError(e));}finally{setBusy(false);}};
 return <section className="settings-section" aria-label="本机运行管理">
  <div className="inspector-heading"><div><h3>本机 OMP · {error?'状态未知':`${all.filter(t=>t.pid).length} 个进程`}</h3><p>所有项目与任务的运行状态，每 3 秒自动刷新。</p></div><Button variant="ghost" size="icon-sm" aria-label="刷新本机运行状态" disabled={busy} onClick={()=>void perform(onRefresh)}><RefreshCw size={14}/></Button></div>
  <ErrorNotice error={error}/><ErrorNotice error={localError}/>
  {!visible.length&&!error&&<p className="inspector-empty">当前没有 OMP 运行进程。打开任务后自动连接，也可在任务的运行页手动启动。</p>}
  <div className="runtime-task-list">{visible.map(item=>{const record=tasks.find(t=>t.id===item.taskId);return <button key={item.taskId} disabled={busy||!record||!onOpenTask} onClick={()=>record&&onOpenTask?.(record)}><span>{record?.title??item.taskId}</span><small>{runtimeLabel(item)}{item.pid?` · PID ${item.pid}`:''}</small></button>;})}</div>
  <p className="inspector-note">只释放可安全回收的后台空闲进程，当前任务和忙碌任务继续保持运行。</p>
  <Button size="sm" variant="outline" disabled={busy||!!error||!all.some(t=>t.pid&&t.owner==='desktop')} onClick={()=>void perform(async()=>{const ids=await taskRuntime.releaseIdle();setNotice(ids.length?`已释放 ${ids.length} 个后台空闲进程`:'没有可安全释放的后台空闲进程');await onRefresh();})}>{busy?'处理中…':'释放后台空闲进程'}</Button>
  {notice&&<p role="status" className="inspector-note">{notice}</p>}
 </section>;
}
