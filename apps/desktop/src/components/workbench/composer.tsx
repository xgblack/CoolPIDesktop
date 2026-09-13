import {ApprovalModeSelect} from './approval-mode';
import type {ApprovalMode} from '../../../../../packages/host-contract/src';
import {FileCompletion,useFileCompletion} from './file-completion';
import type {FileReference} from '@/host';
import {DropdownMenu,DropdownMenuTrigger,DropdownMenuContent,DropdownMenuItem} from '@/components/ui/dropdown-menu';
import { useRef, useLayoutEffect, useState } from 'react';
import { ArrowUp, Square, Plus, File, X, Play, Settings2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';

export interface ComposerProps {
  taskId: string;
  approvalMode?:ApprovalMode|null; onApprovalMode?:(mode:ApprovalMode|null)=>void;
  approvalDisabled?:boolean; approvalSwitching?:boolean;
  roots?:string[];
  references?:FileReference[];onReferences?:(refs:FileReference[])=>void;
  models?:{id:string;provider:string;name?:string}[];onModel?:(provider:string,id:string)=>void;
  modelValue?:string;modelsLoading?:boolean;modelsError?:string;onRefreshModels?:()=>void;
  modelLabel?:string; onFiles?:()=>void; onContinue?:()=>void; onSettings?:()=>void;
  draft: string;
  onDraft: (value: string) => void;
  onSend: () => void;
  onAbort: () => void;
  canSend: boolean;
  running: boolean;
  busy: boolean;
  disabledReason?: string;
  modelDisabled?:boolean;
  attachmentIds?: string[];
  onRemoveAttachment?: (id:string) => void;
}

export function Composer({ taskId, draft, onDraft, onSend, onAbort, canSend, running, busy, disabledReason, modelDisabled=false, attachmentIds=[], onRemoveAttachment, modelLabel, onFiles, onContinue, onSettings, roots=[],references=[],onReferences,models=[],onModel,modelValue,modelsLoading,modelsError,onRefreshModels, approvalMode, onApprovalMode, approvalDisabled, approvalSwitching }: ComposerProps) {
  const [caret,setCaret]=useState(draft.length),[dismissed,setDismissed]=useState(false),[selected,setSelected]=useState(0);
  const match=draft.slice(0,caret).match(/(?:^|\s)@([^\s@]*)$/);
  const query=match&&!dismissed&&roots.length?match[1]:null;
  const completion=useFileCompletion(taskId,roots.length,query);
  const input=useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(()=>{input.current?.setSelectionRange(draft.length,draft.length);setCaret(draft.length);},[taskId]);
  useLayoutEffect(()=>{if(input.current){input.current.style.height='auto';input.current.style.height=Math.min(280,Math.max(60,input.current.scrollHeight))+'px';}},[draft,taskId]);
  const composing = useRef(false);
  const selectFile=(file:FileReference)=>{if(!match)return;const start=caret-match[1].length-1;const value=`@${file.name} `;onDraft(draft.slice(0,start)+value+draft.slice(caret));onReferences?.([...references.filter(r=>r.rootIndex!==file.rootIndex||r.path!==file.path),file]);setDismissed(true);requestAnimationFrame(()=>{input.current?.focus();input.current?.setSelectionRange(start+value.length,start+value.length);});};
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
      {query!==null&&<FileCompletion {...completion} selected={selected} onSelect={selectFile}/>}
      {commandMenu&&<div className="composer-commands" aria-label="工作台快捷命令">{commands.map(command=><button key={command.id} type="button" disabled={busy} onClick={()=>{command.action?.();onDraft('');}}><code>{command.id}</code><span>{command.label}</span></button>)}</div>}
      <label htmlFor={`composer-${taskId}`} className="sr-only">消息</label>
      <Textarea ref={input} autoFocus id={`composer-${taskId}`} value={draft} onChange={event => {onDraft(event.target.value);onReferences?.(references.filter(r=>event.target.value.includes('@'+r.name)));setCaret(event.target.selectionStart);setDismissed(false);setSelected(0);}} onSelect={e=>setCaret(e.currentTarget.selectionStart)} placeholder="描述你想完成的任务…" aria-describedby={`composer-hint-${taskId}`} aria-controls={query!==null?'file-candidates':undefined} aria-activedescendant={query!==null&&completion.items[selected]?`file-candidate-${selected}`:undefined}
        className="composer-input" onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}
        onKeyDown={event => {
          if(event.nativeEvent.isComposing||event.nativeEvent.keyCode===229||composing.current)return;
          if(query!==null){if(event.key==='Escape'){event.preventDefault();setDismissed(true);return;}if(['ArrowDown','ArrowUp'].includes(event.key)){event.preventDefault();setSelected(i=>Math.max(0,Math.min(completion.items.length-1,i+(event.key==='ArrowDown'?1:-1))));return;}if(event.key==='Enter'&&!event.shiftKey&&!event.ctrlKey&&!event.metaKey){event.preventDefault();if(completion.items[selected])selectFile(completion.items[selected]);return;}}
          if(event.key==='Enter'&&!event.altKey){if(event.shiftKey||event.ctrlKey||event.metaKey){if(event.ctrlKey||event.metaKey){event.preventDefault();const el=event.currentTarget;const pos=el.selectionStart;onDraft(draft.slice(0,pos)+'\n'+draft.slice(el.selectionEnd));requestAnimationFrame(()=>input.current?.setSelectionRange(pos+1,pos+1));}return;}event.preventDefault();send();}
        }} />
      {!!references.length&&<div className="composer-references">{references.map(file=><span key={`${file.rootIndex}:${file.path}`} title={file.path}><File size={14}/>{file.name}<button type="button" aria-label={`移除引用 ${file.name}`} onClick={()=>{onReferences?.(references.filter(r=>r!==file));if(!references.some(r=>r!==file&&r.name===file.name))onDraft(draft.replace('@'+file.name,''));}}><X size={12}/></button></span>)}</div>}
      {!!attachmentIds.length&&<div className="mt-2 flex flex-wrap gap-1">{attachmentIds.map(id=><button type="button" key={id} className="rounded bg-muted px-2 py-1 text-xs" onClick={()=>onRemoveAttachment?.(id)}>附件 {id.slice(0,8)} ×</button>)}</div>}
      <div className="composer-toolbar">
        <div className="composer-tools"><DropdownMenu><DropdownMenuTrigger asChild><Button type="button" variant="ghost" size="icon-sm" aria-label="添加内容"><Plus size={16}/></Button></DropdownMenuTrigger><DropdownMenuContent side="top" align="start"><DropdownMenuItem disabled={!roots.length} onSelect={()=>{onDraft(draft+' @');setCaret(draft.length+2);setDismissed(false);requestAnimationFrame(()=>input.current?.focus());}}>引用工作区文件</DropdownMenuItem>{onFiles&&<DropdownMenuItem onSelect={onFiles}>文件与附件</DropdownMenuItem>}{onSettings&&<DropdownMenuItem onSelect={onSettings}>模型配置</DropdownMenuItem>}</DropdownMenuContent></DropdownMenu>{onApprovalMode&&<ApprovalModeSelect value={approvalMode} onChange={onApprovalMode} disabled={busy||running||approvalDisabled} switching={approvalSwitching}/>}</div>
        <div className="composer-tools"><DropdownMenu><DropdownMenuTrigger asChild><Button type="button" variant="ghost" size="sm" aria-label="选择模型" disabled={busy||running||modelDisabled}>{models.find(m=>m.provider+'/'+m.id===modelValue)?.name??models.find(m=>m.provider+'/'+m.id===modelValue)?.id??modelLabel??(modelsLoading?'加载模型…':modelValue?'模型不可用':'选择模型')}</Button></DropdownMenuTrigger><DropdownMenuContent side="top" align="end">{modelsLoading&&<DropdownMenuItem disabled>正在加载模型…</DropdownMenuItem>}{modelsError&&<DropdownMenuItem disabled>{modelsError}</DropdownMenuItem>}{!modelsLoading&&!modelsError&&!models.length&&<DropdownMenuItem disabled>没有可用模型</DropdownMenuItem>}{onRefreshModels&&<DropdownMenuItem disabled={modelsLoading} onSelect={onRefreshModels}>刷新模型列表</DropdownMenuItem>}{models.map(m=><DropdownMenuItem key={m.provider+'/'+m.id} disabled={modelsLoading} onSelect={()=>onModel?.(m.provider,m.id)}>{modelValue===m.provider+'/'+m.id?'✓ ':''}{m.name??m.id} · {m.provider}</DropdownMenuItem>)}{onSettings&&<DropdownMenuItem onSelect={onSettings}>模型配置</DropdownMenuItem>}</DropdownMenuContent></DropdownMenu>
        {running ? <Button type="button" variant="secondary" size="sm" disabled={busy} onClick={onAbort}><Square size={14} />取消生成</Button>
          : <Button type="submit" size="icon-sm" aria-label="发送" title="发送" disabled={!sendable}><ArrowUp size={16} /></Button>}</div>
      </div>
    </div>
    <div className="composer-hint" id={`composer-hint-${taskId}`}><span>{disabledReason}</span>{!canSend&&!running&&disabledReason?.includes('模型')&&onSettings&&<Button type="button" size="sm" variant="ghost" onClick={onSettings}><Settings2/>配置模型</Button>}</div>
  </form></>;
}
