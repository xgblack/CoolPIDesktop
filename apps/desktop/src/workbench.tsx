import {useEffect,useRef,useState} from 'react';
import {host,hostError,projects,records,recoverSession} from './host';
import type {Project,TaskRecord,TaskSnapshot,RuntimeInfo,Message,PendingUiRequest} from '../../../packages/host-contract/src';
import './style.css';
import {HistoryFeed} from './history';
function messageText(content:unknown):string {
 if(typeof content==='string')return content;
 if(Array.isArray(content))return content.map(v=>typeof v?.text==='string'?v.text:typeof v?.thinking==='string'?'思考：'+v.thinking:v?.type==='image'?'[图片]':v?.type==='toolCall'?'工具调用：'+v.name+'\n'+JSON.stringify(v.arguments,null,2):JSON.stringify(v)).join('\n');
 return JSON.stringify(content)??'';
}
function Approval({request,respond}:{request:PendingUiRequest;respond:(value:string|null,confirmed:boolean|null,cancelled:boolean)=>void}){
 const [value,setValue]=useState('');
 return <fieldset><legend>{request.title||request.method}</legend><p>{request.message}</p>{request.method==='confirm'?<><button onClick={()=>respond(null,true,false)}>允许</button><button onClick={()=>respond(null,false,false)}>拒绝</button></>:<>{request.method==='select'?<select value={value} onChange={e=>setValue(e.target.value)}><option value="">选择</option>{request.options?.map(v=><option key={v}>{v}</option>)}</select>:<textarea value={value} onChange={e=>setValue(e.target.value)}/>}<button onClick={()=>respond(value,null,false)}>提交</button></>}<button onClick={()=>respond(null,null,true)}>取消请求</button></fieldset>;
}
export function App(){
 const [ps,setPs]=useState<Project[]>([]),[ts,setTs]=useState<TaskRecord[]>([]),[runs,setRuns]=useState<TaskSnapshot[]>([]);
 const [project,setProject]=useState(''),[selected,setSelected]=useState(''),[showArchived,setArchived]=useState(false);
 const [runtime,setRuntime]=useState<RuntimeInfo>(),[path,setPath]=useState(''),[name,setName]=useState(''),[title,setTitle]=useState(''),[trusted,setTrusted]=useState(false);
 const [busy,setBusy]=useState(false),[error,setError]=useState(''),[draft,setDraft]=useState(''),[messages,setMessages]=useState<Message[]>([]),[cursor,setCursor]=useState<string|null>(null),[model,setModel]=useState('');
 const generation=useRef(0),active=useRef(true),lastStatus=useRef(''),feed=useRef(new HistoryFeed());
 const [candidate,setCandidate]=useState(''),[relocateTrust,setRelocateTrust]=useState(false);
 const task=ts.find(t=>t.id===selected),run=runs.find(t=>t.taskId===selected),p=ps.find(p=>p.id===project);
 const refresh=async()=>{const epoch=generation.current;const [a,b,c]=await Promise.all([projects.list(),records.list(),host.list()]);if(active.current&&epoch===generation.current){setPs(a);setTs(b);setRuns(c);}};
 const fail=(e:unknown)=>{const v=hostError(e);setError([v.code,v.message,v.suggestion].filter(Boolean).join(' '));};
 const act=async(f:()=>Promise<unknown>)=>{generation.current++;setBusy(true);setError('');try{await f();}catch(e){fail(e);}finally{generation.current++;setBusy(false);await refresh().catch(fail);}};
 useEffect(()=>{active.current=true;let inFlight=false;const poll=async()=>{if(inFlight)return;inFlight=true;try{await refresh();}catch(e){if(active.current)fail(e);}finally{inFlight=false;}};void poll();const id=setInterval(poll,700);return()=>{active.current=false;clearInterval(id);};},[]);
 const history=(more=false)=>feed.current.load(cursor=>records.history(selected,cursor),more,()=>{if(active.current){setMessages(feed.current.messages);setCursor(feed.current.cursor);}});
 useEffect(()=>{feed.current.reset();setMessages([]);setCursor(null);lastStatus.current='';setModel('');setCandidate('');setRelocateTrust(false);},[selected,run?.runId]);
 const turn=run?.events.filter(e=>e.eventType==='user_message').at(-1)?.seq??0;
 useEffect(()=>{if(!run)return;const key=run.runId+':'+run.status+':'+turn;if(run.status==='running'){feed.current.invalidate();lastStatus.current=key;return;}if(key!==lastStatus.current&&['ready','idle','interrupted'].includes(run.status)){lastStatus.current=key;void history().catch(fail);} },[run?.status,run?.runId,turn,selected]);
 const choose=(t:TaskRecord)=>{generation.current++;feed.current.reset();setSelected(t.id);setProject(t.projectId);setTitle(t.title);setMessages([]);setCursor(null);};
 const capabilities=(run?.runtime?.capabilities??runtime?.capabilities) as {models?:{id:string;provider:string;name?:string}[];state?:{model?:{id:string;provider:string}}}|undefined;
 return <main><header><h1>OMP Desktop</h1><span>项目工作台</span></header>{error&&<p role="alert" className="error">{error}</p>}
 <details><summary>系统运行时</summary><label>OMP executable<input value={path} onChange={e=>setPath(e.target.value)} placeholder="已保存路径或 PATH"/></label><button disabled={busy} onClick={()=>act(async()=>setRuntime(await host.runtime(path)))}>检测 OMP</button>{runtime&&<p>{runtime.status} · {runtime.version} · RPC {runtime.protocol}<br/>{runtime.executable}<br/>{runtime.error?.message} {runtime.error?.suggestion}</p>}</details>
 <div className="workspace"><aside><h2>项目</h2><input aria-label="项目名称" value={name} onChange={e=>setName(e.target.value)} placeholder="项目名称"/><label><input type="checkbox" checked={trusted} onChange={e=>setTrusted(e.target.checked)}/>信任所选目录中的配置与扩展</label><button disabled={busy||!name.trim()||!trusted} onClick={()=>act(async()=>{const p=await projects.register(name,trusted);if(p){setProject(p.id);setName('');}})}>选择目录并添加</button>
 {ps.filter(p=>showArchived||!p.archived).map(p=><button key={p.id} className={project===p.id?'selected':''} onClick={()=>setProject(p.id)}>{p.name}{p.archived?'（归档）':''}</button>)}
 <label><input type="checkbox" checked={showArchived} onChange={e=>setArchived(e.target.checked)}/>显示归档</label>
 {p&&<><p className="paths">{p.roots.join('\n')}</p><button disabled={busy||!name.trim()} onClick={()=>act(()=>projects.update(p.id,name,p.archived))}>使用上方名称重命名</button><button disabled={busy} onClick={()=>act(()=>projects.update(p.id,p.name,!p.archived))}>{p.archived?'恢复项目':'归档项目'}</button></>}
 <h2>任务</h2><input aria-label="任务名称" value={title} onChange={e=>setTitle(e.target.value)} placeholder="任务名称"/><button disabled={busy||!p||p.archived||!title.trim()} onClick={()=>act(async()=>choose(await records.create(project,title)))}>创建任务</button>
 {ts.filter(t=>t.projectId===project&&(showArchived||!t.archived)).sort((a,b)=>Number(b.pinned)-Number(a.pinned)).map(t=><button key={t.id} className={selected===t.id?'selected':''} onClick={()=>choose(t)}>{t.pinned?'* ':''}{t.title} · {runs.find(r=>r.taskId===t.id)?.status??t.lastRun?.state??'未运行'}</button>)}</aside>
 <section>{task?<><h2>{task.title}</h2><p>{run?.status??task.lastRun?.state??'未运行'} · {task.sessionId??'新会话'}</p><div className="controls"><button disabled={busy||task.archived} onClick={()=>act(()=>records.resume(task.id))}>加载会话 / 继续</button><button disabled={busy||!run||['stopped','failed'].includes(run.status)} onClick={()=>act(()=>host.stop(task.id))}>停止</button><button disabled={busy||!run||task.archived} onClick={()=>act(()=>host.restart(task.id))}>重启进程</button><button disabled={busy||run?.status!=='running'} onClick={()=>act(()=>host.abort(task.id))}>取消生成</button><button disabled={busy||!title.trim()} onClick={()=>act(()=>records.update({...task,title}))}>重命名</button><button disabled={busy} onClick={()=>act(()=>records.update({...task,pinned:!task.pinned}))}>{task.pinned?'取消置顶':'置顶'}</button><button disabled={busy} onClick={()=>act(()=>records.update({...task,archived:!task.archived}))}>{task.archived?'恢复任务':'归档任务'}</button></div>
 <details><summary>目录与会话恢复</summary><p>主目录：{task.roots[0]}</p>{task.roots.slice(1).map(root=><p key={root}>附加目录：{root}</p>)}<label><input type="checkbox" checked={relocateTrust} onChange={e=>setRelocateTrust(e.target.checked)}/>信任重新选择的目录配置与扩展</label><button disabled={busy||!relocateTrust||!!run&&!['stopped','failed'].includes(run.status)} onClick={()=>act(async()=>{await records.relocate(task.id,relocateTrust);setRelocateTrust(false);})}>重新定位目录</button>{!task.sessionId&&<><button disabled={busy} onClick={()=>act(async()=>setCandidate(await recoverSession(task.id,false)))}>查找未绑定会话</button>{candidate&&<p>发现会话 {candidate}<button disabled={busy} onClick={()=>act(async()=>{await recoverSession(task.id,true);setCandidate('');})}>确认恢复此会话</button></p>}</>}<p>会话缺失或损坏时保留原任务。需要独立对话请创建新任务。</p></details>
 {!run&&task.lastRun?.errorCode&&<p className="error">上次运行：{task.lastRun.errorCode}；不会重发未完成消息或旧审批。</p>}
 {!run&&<p>加载会话将启动 OMP；OMP 可能迁移会话格式。</p>}{run?.error&&<p className="error">{run.error.message} {run.error.suggestion}</p>}
 <label>模型<select value={model} onChange={e=>setModel(e.target.value)}><option value="">{capabilities?.state?.model?.id??'选择已配置模型'}</option>{capabilities?.models?.map(m=><option key={m.provider+'/'+m.id} value={JSON.stringify([m.provider,m.id])}>{m.provider}/{m.id}</option>)}</select></label><button disabled={busy||!model||!run||run.status==='running'} onClick={()=>act(async()=>{const [provider,id]=JSON.parse(model);await records.model(task.id,provider,id);setModel('');})}>切换模型</button>
 {run&&!(capabilities?.models?.length)&&<p>没有可用模型；请配置 OMP 后重新检测。</p>}
 {run?.pendingUi?.map(r=><Approval key={run.runId+r.id} request={r} respond={(value,confirmed,cancelled)=>{void act(()=>host.respond(task.id,r.id,value,confirmed,cancelled));}}/>)}
 <button disabled={busy||!run||!['ready','idle','interrupted'].includes(run.status)} onClick={()=>act(()=>history())}>刷新历史</button><div className="messages">{messages.map((m,i)=><article key={task.sessionId+':'+run?.runId+':'+i}><strong>{m.role}</strong><pre>{messageText(m.content)}</pre></article>)}</div>{cursor&&<button disabled={busy||run?.status==='running'} onClick={()=>act(()=>history(true))}>加载更多历史</button>}
 {run?.status==='running'&&<pre>{run.text}</pre>}
 <form onSubmit={e=>{e.preventDefault();const message=draft;void act(async()=>{await host.prompt(task.id,message);setDraft('');});}}><textarea aria-label="消息" value={draft} onChange={e=>setDraft(e.target.value)}/><button disabled={busy||!draft.trim()||!run||!['ready','idle','interrupted'].includes(run.status)||!capabilities?.state?.model}>发送</button></form>
 </>:<p>选择项目和任务。</p>}</section></div></main>;
}
