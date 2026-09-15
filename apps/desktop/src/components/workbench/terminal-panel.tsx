import {useEffect,useRef,useState} from 'react';
import {Terminal} from '@xterm/xterm';
import {FitAddon} from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import {Play,Plus,SquareTerminal,X} from 'lucide-react';
import {terminal,hostError} from '@/host';
import {ErrorNotice,IconButton} from './shared';
import type {HostError,TaskRecord,TerminalSnapshot} from '../../../../../packages/host-contract/src';

export function TerminalPanel({task}:{task:TaskRecord}) {
 return <TaskTerminal key={task.id+JSON.stringify(task.roots)} task={task}/>;
}
type TerminalTab={key:number;root:number;title:string;restored?:boolean};
const storageKey=(taskId:string)=>`cool-pi-desktop.terminal-tabs:${taskId}`;
function restoreTabs(task:TaskRecord):TerminalTab[]{
 try{
  const value=JSON.parse(localStorage.getItem(storageKey(task.id))??'[]');
  if(!Array.isArray(value))return[];
  return value.slice(0,4).flatMap((item,index)=>Number.isInteger(item?.root)&&item.root>=0&&item.root<task.roots.length?[{key:index+1,root:item.root,title:typeof item.title==='string'&&item.title.length<=80?item.title:`终端 ${index+1}`,restored:true}]:[]);
 }catch{return[]}
}
function TaskTerminal({task}:{task:TaskRecord}) {
 const initial=useRef<TerminalTab[]>(restoreTabs(task));
 const [root,setRoot]=useState(0),[generation,setGeneration]=useState(initial.current.length),[sessions,setSessions]=useState<TerminalTab[]>(initial.current),[active,setActive]=useState<number|undefined>(initial.current.at(-1)?.key);
 useEffect(()=>{try{localStorage.setItem(storageKey(task.id),JSON.stringify(sessions.map(({root,title})=>({root,title}))))}catch{}},[sessions,task.id]);
 const start=()=>{if(sessions.length>=4)return;const key=generation+1;setGeneration(key);setSessions(old=>[...old,{key,root,title:`终端 ${old.length+1}`}]);setActive(key)};
 const close=(key:number)=>{setSessions(old=>{const next=old.filter(item=>item.key!==key);setActive(current=>current===key?next.at(-1)?.key:current);return next;});};
 const restart=(key:number)=>setSessions(old=>old.map(item=>item.key===key?{...item,restored:false}:item));
 return <section className="inspector-section" aria-label="集成终端">
  <div className="inspector-heading"><h2><SquareTerminal size={15}/>终端</h2>
   <IconButton label="启动终端" disabled={sessions.length>=4||task.archived} onClick={start}>{sessions.length?<Plus size={14}/>:<Play size={14}/>}</IconButton>
  </div>
  <select aria-label="终端执行目录" className="w-full min-w-0 border p-1 text-xs" value={root} onChange={e=>setRoot(Number(e.target.value))}>
   {task.roots.map((path,i)=><option key={i} value={i}>{i===0?'主目录':'附加目录 '+i} · {path}</option>)}
  </select>
  {!!sessions.length&&<div className="terminal-tabs" role="tablist" aria-label="终端标签">{sessions.map(item=><button role="tab" aria-selected={active===item.key} key={item.key} onClick={()=>setActive(item.key)}>{item.title}<small>{item.root===0?'主目录':`目录 ${item.root+1}`}</small></button>)}</div>}
  {sessions.map(item=><div key={item.key} hidden={active!==item.key} className="terminal-tab" data-state={active===item.key?'active':'inactive'}>{item.restored?<div className="terminal-restored"><p role="status">上次终端进程已退出。重新启动会在同一受信任目录创建新的 PTY。</p><div><button type="button" onClick={()=>restart(item.key)}><Play/>重新启动终端</button><IconButton label="关闭终端标签" onClick={()=>close(item.key)}><X/></IconButton></div></div>:<TerminalSession taskId={task.id} root={item.root} onClosed={()=>close(item.key)}/>}</div>)}
 </section>;
}
export function TerminalSession({taskId,root,onClosed}:{taskId:string;root:number;onClosed:()=>void}) {
 const container=useRef<HTMLDivElement>(null),closeRef=useRef<()=>Promise<void>>(async()=>{});
 const [error,setError]=useState<HostError>(),[status,setStatus]=useState('正在启动'),[closing,setClosing]=useState(false);
 useEffect(()=>{
  let active=true,id='',cursor=0,timer:ReturnType<typeof setTimeout>|undefined;
  let inputQueue=Promise.resolve(),resizeQueue=Promise.resolve();
  const term=new Terminal({fontSize:12,scrollback:3000,convertEol:false,disableStdin:true,theme:{background:'#17191c',foreground:'#e4e7eb'},allowProposedApi:false});
  const fit=new FitAddon();term.loadAddon(fit);term.open(container.current!);
  const report=(e:unknown)=>{if(active)setError(hostError(e));else console.error('Terminal cleanup failed',e)};
  const resize=()=>{if(!active||!id||!container.current?.clientWidth)return;fit.fit();const cols=Math.min(500,Math.max(2,term.cols)),rows=Math.min(200,Math.max(1,term.rows));resizeQueue=resizeQueue.then(()=>terminal.resize(taskId,id,cols,rows)).catch(report)};
  const observer=new ResizeObserver(resize);observer.observe(container.current!);
  const receive=async(s:TerminalSnapshot)=>{
   if(!active||s.id!==id||s.taskId!==taskId)return;
   if(s.start!==cursor){term.reset();term.writeln('[输出缓冲已截断]')}
   const bytes=Uint8Array.from(atob(s.output),c=>c.charCodeAt(0));
   await new Promise<void>(resolve=>term.write(bytes,resolve));
   cursor=s.end;
   if(s.error)report(s.error);
   if(s.exited){term.options.disableStdin=true;setStatus('已退出 · '+(s.exitCode??'未知'));return}
   timer=setTimeout(()=>void poll(),100);
  };
  const poll=async()=>{try{const s=await terminal.snapshot(taskId,id,cursor);await receive(s)}catch(e){if(active){term.options.disableStdin=true;setStatus('已断开');report(e)}}};
  const data=term.onData(value=>{if(!id||!active)return;inputQueue=inputQueue.then(()=>terminal.write(taskId,id,value)).catch(e=>{term.options.disableStdin=true;report(e)})});
  const close=async()=>{if(!id)return;await terminal.close(taskId,id);id=''};
  closeRef.current=async()=>{setClosing(true);try{await close();if(active)onClosed()}catch(e){report(e);if(active)setClosing(false)}};
  void terminal.create(taskId,root).then(async s=>{
   id=s.id;
   if(!active){await close();return}
   setStatus('运行中');term.options.disableStdin=false;resize();term.focus();await receive(s);
  }).catch(e=>{report(e);if(active)setStatus('启动失败')});
  return()=>{active=false;clearTimeout(timer);observer.disconnect();data.dispose();term.dispose();void close().catch(report)};
 },[taskId,root]);
 return <div className="mt-2 min-w-0"><div className="flex items-center justify-between"><span role="status" className="text-xs">{status}</span><IconButton label="关闭终端" disabled={closing} onClick={()=>void closeRef.current()}><X size={14}/></IconButton></div><div ref={container} className="terminal-canvas" aria-label="终端屏幕"/><ErrorNotice error={error}/></div>;
}
