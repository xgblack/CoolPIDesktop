import {FileReferencePicker} from './file-reference-picker';
import { useRef, useLayoutEffect, useState } from 'react';
import { ArrowUp, Square, AtSign, Paperclip, Play, Settings2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';

export interface ComposerProps {
  taskId: string;
  roots?:string[];
  modelLabel?:string; onFiles?:()=>void; onContinue?:()=>void; onSettings?:()=>void;
  draft: string;
  onDraft: (value: string) => void;
  onSend: () => void;
  onAbort: () => void;
  canSend: boolean;
  running: boolean;
  busy: boolean;
  disabledReason?: string;
  attachmentIds?: string[];
  onRemoveAttachment?: (id:string) => void;
}

export function Composer({ taskId, draft, onDraft, onSend, onAbort, canSend, running, busy, disabledReason, attachmentIds=[], onRemoveAttachment, modelLabel, onFiles, onContinue, onSettings, roots=[] }: ComposerProps) {
  const [reference,setReference]=useState(false);
  const input=useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(()=>{if(input.current){input.current.style.height='auto';input.current.style.height=Math.min(280,Math.max(60,input.current.scrollHeight))+'px';}},[draft,taskId]);
  const composing = useRef(false);
  const sendable = canSend && !running && !busy && (draft.trim().length > 0 || attachmentIds.length > 0);
  const send = () => { if (composing.current) return;const command=commands.find(c=>c.id===draft.trim());if(command&&!busy){command.action?.();onDraft('');}else if(sendable)onSend(); };
  const commands=[
    {id:'/files',label:'打开文件与附件',action:onFiles},
    {id:'/settings',label:'打开模型设置',action:onSettings},
    {id:'/stop',label:'取消当前生成',action:running?onAbort:undefined},
    {id:'/continue',label:'继续当前任务',action:!canSend&&!running?onContinue:undefined},
  ].filter(command=>command.action&&command.id.startsWith(draft.trim()));
  const commandMenu=draft.startsWith('/')&&!draft.includes(' ')&&commands.length>0;
  return <><form aria-label="发送消息" className="composer" onSubmit={event => { event.preventDefault(); send(); }}>
    <div className="composer-card">
      {commandMenu&&<div className="composer-commands" aria-label="工作台快捷命令">{commands.map(command=><button key={command.id} type="button" disabled={busy} onClick={()=>{command.action?.();onDraft('');}}><code>{command.id}</code><span>{command.label}</span></button>)}</div>}
      <label htmlFor={`composer-${taskId}`} className="sr-only">消息</label>
      <Textarea ref={input} id={`composer-${taskId}`} value={draft} onChange={event => onDraft(event.target.value)} placeholder="描述你想完成的任务…" aria-describedby={`composer-hint-${taskId}`}
        className="composer-input" onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}
        onKeyDown={event => { if (event.key === 'Enter' && (event.metaKey || event.ctrlKey) && !event.nativeEvent.isComposing && event.nativeEvent.keyCode !== 229 && !composing.current) { event.preventDefault(); send(); } }} />
      {!!attachmentIds.length&&<div className="mt-2 flex flex-wrap gap-1">{attachmentIds.map(id=><button type="button" key={id} className="rounded bg-muted px-2 py-1 text-xs" onClick={()=>onRemoveAttachment?.(id)}>附件 {id.slice(0,8)} ×</button>)}</div>}
      <div className="composer-toolbar">
        <div className="composer-tools">{onFiles&&<Button type="button" variant="ghost" size="icon-sm" aria-label="文件与附件" onClick={onFiles}><Paperclip size={16}/></Button>}{!!roots.length&&<Button type="button" variant="ghost" size="icon-sm" aria-label="引用工作区文件" onClick={()=>setReference(true)}><AtSign size={16}/></Button>}<span className="composer-model">{modelLabel??'未选择模型'}</span></div>
        {running ? <Button type="button" variant="secondary" size="sm" disabled={busy} onClick={onAbort}><Square size={14} />取消生成</Button>
          : <Button type="submit" size="sm" disabled={!sendable}><ArrowUp size={14} />{busy ? '处理中…' : '发送'}</Button>}
      </div>
    </div>
    <div className="composer-hint" id={`composer-hint-${taskId}`}><span>{disabledReason || '⌘ / Ctrl + Enter 发送 · Enter 换行'}</span>{!canSend&&!running&&disabledReason?.includes('继续')&&onContinue&&<Button type="button" size="sm" variant="ghost" disabled={busy} onClick={onContinue}><Play/>继续任务</Button>}{!canSend&&!running&&disabledReason?.includes('模型')&&onSettings&&<Button type="button" size="sm" variant="ghost" onClick={onSettings}><Settings2/>配置模型</Button>}</div>
  </form>{reference&&<FileReferencePicker taskId={taskId} roots={roots} onClose={()=>{setReference(false);input.current?.focus();}} onSelect={value=>{onDraft(draft+(draft&&!draft.endsWith(' ')?' ':'')+value+' ');setReference(false);requestAnimationFrame(()=>input.current?.focus());}}/>}</>;
}
