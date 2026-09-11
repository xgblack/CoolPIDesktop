import {useEffect,useRef,useState} from 'react';
import {RefreshCw,SquareTerminal,X} from 'lucide-react';
import {terminal,hostError} from '@/host';
import {ErrorNotice,IconButton} from './shared';
import type {HostError,TaskRecord,TerminalSnapshot} from '../../../../../packages/host-contract/src';

export function TerminalPanel({task}:{task:TaskRecord}) {
 const [snap,setSnap]=useState<TerminalSnapshot>(),[input,setInput]=useState(''),[error,setError]=useState<HostError>(),[busy,setBusy]=useState(false),id=useRef(''),alive=useRef(true),outputRef=useRef<HTMLPreElement>(null);
 useEffect(()=>()=>{alive.current=false;if(id.current)void terminal.close(task.id,id.current).catch(()=>{});},[task.id]);
 const refresh=async()=>{if(!id.current)return;try{const v=await terminal.snapshot(task.id,id.current);if(alive.current)setSnap(v)}catch(e){if(alive.current)setError(hostError(e))}};
 const start=async()=>{setBusy(true);setError(undefined);try{const v=await terminal.create(task.id,0);id.current=v.id;setSnap(v)}catch(e){setError(hostError(e))}finally{setBusy(false)}};
 useEffect(()=>{void start();const timer=setInterval(()=>void refresh(),300);return()=>clearInterval(timer)},[task.id]);
 useEffect(()=>{if(!id.current||!outputRef.current)return;const observer=new ResizeObserver(entries=>{const width=entries[0]?.contentRect.width??640;void terminal.resize(task.id,id.current,Math.max(20,Math.min(500,Math.floor(width/8))),24).catch(()=>{})});observer.observe(outputRef.current);return()=>observer.disconnect()},[task.id,snap?.id]);
 const send=async()=>{if(!id.current||!input)return;const value=input;setInput('');try{await terminal.write(task.id,id.current,value)}catch(e){setError(hostError(e))}};
 return <section className="inspector-section terminal-section" aria-label="集成终端"><div className="inspector-heading"><div><h2><SquareTerminal size={15}/>集成终端</h2><p>{snap?.exited?'终端已退出':'绑定当前任务主目录'}</p></div><div className="flex gap-1"><IconButton label="刷新终端" disabled={!id.current} onClick={()=>void refresh()}><RefreshCw size={14}/></IconButton><IconButton label="关闭终端" disabled={!id.current} onClick={()=>{if(id.current)void terminal.close(task.id,id.current);id.current='';setSnap(undefined)}}><X size={14}/></IconButton></div></div><ErrorNotice error={error}/>{busy?<p role="status">正在启动终端…</p>:snap&&<><pre ref={outputRef} className="terminal-output" aria-label="终端输出">{snap.output||' '}</pre><div className="terminal-input"><input value={input} disabled={snap.exited} onChange={e=>setInput(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'){e.preventDefault();void send()}}} placeholder="输入命令" aria-label="终端输入"/><button type="button" disabled={snap.exited||!input} onClick={()=>void send()}>发送</button></div></>}</section>;
}
