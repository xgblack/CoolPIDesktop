import {useEffect,useState} from 'react';
import {Copy,Play,RefreshCw,Square} from 'lucide-react';
import type {HostError,RuntimeTaskInfo,TaskRecord,TaskSnapshot} from '../../../../../packages/host-contract/src';
import {taskRuntime,hostError} from '@/host';
import {Button} from '@/components/ui/button';
import {ErrorNotice} from './shared';
import {runtimeLabel} from './use-task-runtime';

type Action='start'|'stop'|'restart'|'cancel';
export interface RuntimePanelProps {task:TaskRecord;run?:TaskSnapshot;info?:RuntimeTaskInfo;all?:RuntimeTaskInfo[];tasks?:TaskRecord[];error?:HostError|null;onRefresh?:()=>Promise<void>;onOpenTask?:(task:TaskRecord)=>void;current?:boolean}
export function RuntimePanel({task,run,info,all=[],tasks=[],error,onRefresh,onOpenTask,current=true}:RuntimePanelProps){
 const [busy,setBusy]=useState(false),[localError,setError]=useState<HostError|null>(null),[confirmation,setConfirmation]=useState<Action|null>(null),[command,setCommand]=useState(''),[copied,setCopied]=useState(false),[notice,setNotice]=useState('');
 const [now,setNow]=useState(Date.now());
 useEffect(()=>{const timer=setInterval(()=>setNow(Date.now()),1000);return()=>clearInterval(timer);},[]);
 const external=info?.owner==='terminal',working=info?.status==='running'||!!run?.pendingUi?.length||!!run?.tools?.some(t=>t.status==='running');
 const connected=!!info?.pid&&info.owner==='desktop';
 const transitional=info?.status==='starting'||info?.status==='unknown';
 const perform=async(fn:()=>Promise<unknown>)=>{setBusy(true);setError(null);setNotice('');try{await fn();await onRefresh?.();}catch(e){setError(hostError(e));}finally{setBusy(false);setConfirmation(null);}};
 const action=(value:Action)=>{if(working&&(value==='stop'||value==='restart')){setConfirmation(value);return;}void perform(()=>taskRuntime.action(task.id,value,info?.runId??null));};
 const seconds=info?.startedAt?Math.max(0,Math.floor((now-(info.startedAt<1e12?info.startedAt*1000:info.startedAt))/1000)):null;
 const elapsed=seconds===null?'—':seconds<60?`${seconds} 秒`:`${Math.floor(seconds/60)} 分 ${seconds%60} 秒`;
 const deadline=info?.idleSince?Math.max(0,Math.ceil((600000-(now-(info.idleSince<1e12?info.idleSince*1000:info.idleSince)))/60000)):null;
 const retention=external?'由外部终端管理':info?.keepAlive?'保持运行，暂停自动释放':current&&connected?'当前任务保持连接':working?'等待工作完成':connected?`后台空闲${deadline===null?'':`，约 ${deadline} 分钟后释放`}`:info?.autoStartSuppressed?'自动启动已暂停；发送消息或手动启动后恢复':'进入任务后自动连接';
 return <div className="runtime-panel">
  <section className="inspector-section" aria-label="OMP 运行详情"><div className="inspector-heading"><h2>OMP 运行</h2><Button variant="ghost" size="icon-sm" aria-label="刷新运行状态" title="刷新运行状态" disabled={busy} onClick={()=>void onRefresh?.()}><RefreshCw size={14}/></Button></div>
   <div className={`runtime-state runtime-${info?.status??'stopped'}`}><span className="runtime-dot"/>{runtimeLabel(info,!!run?.pendingUi?.length)}</div>
   <ErrorNotice error={error}/><ErrorNotice error={localError??info?.error}/>
   <dl className="runtime-facts"><div><dt>PID</dt><dd><code>{info?.pid??'—'}</code></dd></div><div><dt>运行时长</dt><dd>{elapsed}</dd></div><div><dt>所有者</dt><dd>{external?'外部终端':connected?'桌面应用':'无进程'}</dd></div><div><dt>OMP 版本</dt><dd>{info?.version??'—'}</dd></div><div><dt>审批模式</dt><dd>{task.approvalMode==='yolo'?'全部允许':task.approvalMode==='write'?'执行需审批':task.approvalMode==='always-ask'?'写入与执行需审批':'使用 OMP 配置'}</dd></div></dl>
   {info?.reason&&<p className="inspector-note">{info.reason}</p>}<p className="inspector-note">{retention}</p>
   <label className="runtime-keep"><input type="checkbox" checked={info?.keepAlive??false} disabled={busy||external||!info} onChange={e=>{const checked=e.target.checked;void perform(()=>taskRuntime.keepAlive(task.id,checked));}}/>保持运行<span>退出应用时仍会停止</span></label>
   <div className="runtime-actions">{!connected?<Button size="sm" disabled={busy||transitional||external||task.archived} onClick={()=>action('start')}><Play size={13}/>{info?.status==='failed'?'重试启动':'启动 OMP'}</Button>:<>{working&&<Button size="sm" variant="outline" disabled={busy} onClick={()=>action('cancel')}>取消本轮</Button>}<Button size="sm" variant="outline" disabled={busy||transitional} onClick={()=>action('restart')}><RefreshCw size={13}/>重启</Button><Button size="sm" variant="outline" className="runtime-danger" disabled={busy||transitional} onClick={()=>action('stop')}><Square size={13}/>停止进程</Button></>}</div>
   {confirmation&&<div className="runtime-confirm" role="alert"><p>{confirmation==='stop'?'停止':'重启'}进程会中断正在进行的工作，包括工具执行和审批等待。</p><Button size="sm" variant="destructive" disabled={busy} onClick={()=>void perform(()=>taskRuntime.action(task.id,confirmation,info?.runId??null))}>确认{confirmation==='stop'?'停止':'重启'}</Button><Button size="sm" variant="ghost" disabled={busy} onClick={()=>setConfirmation(null)}>返回</Button></div>}
   {external&&<p className="inspector-note">终端正持有此会话。桌面暂停发送和自动连接；终端退出后恢复。</p>}
   <details className="runtime-technical"><summary>技术详情</summary><dl className="runtime-facts"><div><dt>Run ID</dt><dd><code>{info?.runId??'—'}</code></dd></div><div><dt>执行文件</dt><dd><code>{info?.executable??'—'}</code></dd></div><div><dt>会话文件</dt><dd><code>{task.sessionFile??'尚未创建'}</code></dd></div></dl></details>
  </section>
  <section className="inspector-section" aria-label="终端续接"><div className="inspector-heading"><h2>在终端继续</h2></div><p className="inspector-note">执行命令时桌面应用须保持打开，且任务必须空闲。复制不会停止进程；接管后的终端不随桌面退出。</p><Button size="sm" variant="outline" disabled={busy||!task.sessionFile} onClick={()=>void perform(async()=>{const result=await taskRuntime.command(task.id);setCommand(result.command);setCopied(false);try{await navigator.clipboard.writeText(result.command);setCopied(true);}catch{throw {code:'clipboard_failed',message:'剪贴板不可用，请从下方命令预览手动复制。'};}})}><Copy size={13}/>{copied?'已复制续接命令':'复制终端续接命令'}</Button>{!task.sessionFile&&<p className="inspector-note">会话创建后可生成续接命令。</p>}{command&&<details className="runtime-technical" open><summary>完整命令</summary><pre className="runtime-command" tabIndex={0}>{command}</pre></details>}</section>
  <section className="inspector-section" aria-label="本机运行管理"><div className="inspector-heading"><div><h2>本机 OMP · {all.filter(t=>t.pid).length} 个进程</h2><p>只释放可安全回收的后台空闲进程</p></div></div><div className="runtime-task-list">{all.filter(t=>t.pid||t.status==='starting'||t.status==='failed').map(item=>{const record=tasks.find(t=>t.id===item.taskId);return <button key={item.taskId} disabled={!record||!onOpenTask} onClick={()=>record&&onOpenTask?.(record)}><span>{record?.title??item.taskId}</span><small>{runtimeLabel(item)}{item.pid?` · ${item.pid}`:''}</small></button>;})}</div><Button size="sm" variant="outline" disabled={busy||!all.some(t=>t.pid&&t.owner==='desktop')} onClick={()=>void perform(async()=>{const ids=await taskRuntime.releaseIdle();setNotice(ids.length?`已释放 ${ids.length} 个后台空闲进程`:'没有可安全释放的后台空闲进程');})}>释放后台空闲进程</Button>{notice&&<p role="status" className="inspector-note">{notice}</p>}</section>
 </div>;
}
