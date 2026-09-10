import { useEffect, useRef, useState } from 'react';
import { host, hostError } from './host';
import { applySnapshot, eventText } from './task-state';
import type { ObserverInfo, PendingUiRequest, RuntimeInfo, TaskSnapshot } from '../../../packages/host-contract/src';
import './style.css';

function UiRequest({request,complete}:{request:PendingUiRequest;complete:(value:string|null,confirmed:boolean|null,cancelled:boolean)=>void}) {
  const [value,setValue]=useState('');
  return <fieldset className="ui-request"><legend>{request.title||request.method}</legend><p>{request.message}</p>
    {request.method==='confirm'&&<><button onClick={()=>complete(null,true,false)}>允许</button><button onClick={()=>complete(null,false,false)}>拒绝</button></>}
    {request.method==='select'&&<select value={value} onChange={event=>setValue(event.target.value)}><option value="">选择一项</option>{request.options?.map(option=><option key={option}>{option}</option>)}</select>}
    {(request.method==='input'||request.method==='editor')&&<textarea value={value} onChange={event=>setValue(event.target.value)}/>}
    {request.method!=='confirm'&&<button disabled={request.method==='select'&&!value} onClick={()=>complete(value,null,false)}>提交</button>}
    <button onClick={()=>complete(null,null,true)}>取消</button>
  </fieldset>;
}

export function App(){
  const generation=useRef(0);
  const [runtime,setRuntime]=useState<RuntimeInfo>(); const [executable,setExecutable]=useState('');
  const [tasks,setTasks]=useState<Record<string,TaskSnapshot>>({}); const [selected,setSelected]=useState('');
  const [message,setMessage]=useState(''); const [busy,setBusy]=useState(false); const [error,setError]=useState(''); const [observer,setObserver]=useState<ObserverInfo>();
  useEffect(()=>{
    let active=true, inFlight=false;
    const refresh=async()=>{
      if(inFlight)return;
      inFlight=true;
      const revision=generation.current;
      try {
        const list=await host.list();
        if(active&&revision===generation.current)setTasks(current=>list.reduce((next,snapshot)=>applySnapshot(next,snapshot),current));
      } catch(reason) {if(active)setError([hostError(reason).message,hostError(reason).suggestion].filter(Boolean).join(' '));}
      finally {inFlight=false;}
    };
    void refresh();const timer=setInterval(refresh,700);
    return()=>{active=false;clearInterval(timer);};
  },[]);
  const task=tasks[selected];
  const act=async(action:()=>Promise<unknown>)=>{generation.current++;setBusy(true);setError('');try{const result=await action();if(result&&typeof result==='object'&&'taskId' in result){const snapshot=result as TaskSnapshot;setTasks(current=>applySnapshot(current,snapshot));setSelected(snapshot.taskId);}}catch(reason){setError([hostError(reason).message,hostError(reason).suggestion].filter(Boolean).join(' '));}finally{generation.current++;setBusy(false);}};
  const inspect=()=>act(async()=>{const result=await host.runtime(executable);setRuntime(result);return result;});
  const respond=(request:PendingUiRequest,value:string|null,confirmed:boolean|null,cancelled:boolean)=>act(()=>host.respond(task.taskId,request.id,value,confirmed,cancelled));
  return <main><header><h1>OMP Desktop</h1><span>本机 Runtime</span></header>
    {error&&<p className="error" role="alert">{error}</p>}
    <section><h2>系统运行时</h2><label>OMP executable <input value={executable} placeholder="留空则从 PATH 检测" onChange={event=>setExecutable(event.target.value)}/></label><button disabled={busy} onClick={inspect}>检测 OMP</button>
      {runtime&&<dl><dt>状态</dt><dd>{runtime.status}</dd><dt>路径</dt><dd>{runtime.executable||'未找到'}</dd><dt>版本</dt><dd>{runtime.version||'—'}</dd><dt>协议</dt><dd>{runtime.protocol||'—'}</dd><dt>能力</dt><dd>{runtime.capabilities? '已协商':'—'}</dd>{runtime.error&&<><dt>诊断</dt><dd>{runtime.error.message}<br/>{runtime.error.suggestion}</dd></>}</dl>}
      <button disabled={busy} onClick={()=>act(async()=>{const info=await host.observer();setObserver(info);return info;})}>启动只读观察</button>{observer&&<p className="observer">{observer.url}<br/>Token: {observer.token}</p>}
    </section>
    <div className="workspace"><aside><h2>任务 <button title="新任务" disabled={busy} onClick={()=>act(()=>host.start(crypto.randomUUID(),executable))}>+</button></h2>{Object.values(tasks).map(item=><button className={item.taskId===selected?'selected':''} key={item.taskId} onClick={()=>setSelected(item.taskId)}>{item.taskId.slice(0,8)} · {item.status}</button>)}</aside>
      <section>{task?<><div className="task-heading"><h2>{task.status}</h2><button disabled={busy||task.status!=='running'} onClick={()=>act(()=>host.abort(task.taskId))}>取消</button><button disabled={busy} onClick={()=>act(()=>host.stop(task.taskId))}>停止</button><button disabled={busy} onClick={()=>act(()=>host.restart(task.taskId))}>重启</button></div>
        {task.error&&<p className="error">{task.error.message}<br/>{task.error.suggestion}</p>}
        {task.pendingUi?.map(request=><UiRequest key={request.id} request={request} complete={(value,confirmed,cancelled)=>respond(request,value,confirmed,cancelled)}/>)}
        {task.truncated&&<p>显示缓冲已截断；完整记录保留在 OMP 会话中。</p>}<pre>{task.text??task.events.map(eventText).filter(Boolean).join('')??'等待输出'}</pre><form onSubmit={event=>{event.preventDefault();if(message.trim())act(async()=>{const result=await host.prompt(task.taskId,message);setMessage('');return result;});}}><textarea value={message} placeholder="输入消息" onChange={event=>setMessage(event.target.value)}/><button disabled={busy||!message.trim()||!['ready','idle','interrupted'].includes(task.status)}>发送</button></form>
      </>:<p>创建或选择任务。</p>}</section></div>
  </main>;
}
