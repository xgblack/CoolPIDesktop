import { useRef } from 'react';
import { ArrowUp, Square } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';

export interface ComposerProps {
  taskId: string;
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

export function Composer({ taskId, draft, onDraft, onSend, onAbort, canSend, running, busy, disabledReason, attachmentIds=[], onRemoveAttachment }: ComposerProps) {
  const composing = useRef(false);
  const sendable = canSend && !running && !busy && draft.trim().length > 0;
  const send = () => { if (sendable && !composing.current) onSend(); };
  return <form aria-label="发送消息" className="shrink-0 border-t border-border bg-background px-4 py-3 sm:px-8" onSubmit={event => { event.preventDefault(); send(); }}>
    <div className="mx-auto max-w-[880px]">
      <label htmlFor={`composer-${taskId}`} className="sr-only">消息</label>
      <Textarea id={`composer-${taskId}`} value={draft} onChange={event => onDraft(event.target.value)} placeholder="描述你想完成的任务…" aria-describedby={`composer-hint-${taskId}`}
        className="min-h-24 max-h-56 resize-y border-0 bg-transparent px-2 shadow-none focus-visible:ring-1" onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}
        onKeyDown={event => { if (event.key === 'Enter' && (event.metaKey || event.ctrlKey) && !event.nativeEvent.isComposing && event.nativeEvent.keyCode !== 229 && !composing.current) { event.preventDefault(); send(); } }} />
      {!!attachmentIds.length&&<div className="mt-2 flex flex-wrap gap-1">{attachmentIds.map(id=><button type="button" key={id} className="rounded bg-muted px-2 py-1 text-xs" onClick={()=>onRemoveAttachment?.(id)}>附件 {id.slice(0,8)} ×</button>)}</div>}
      <div className="mt-2 flex items-center justify-between gap-3">
        <p id={`composer-hint-${taskId}`} className="text-xs text-muted-foreground">{disabledReason || '⌘ / Ctrl + Enter 发送 · Enter 换行'}</p>
        {running ? <Button type="button" variant="secondary" size="sm" disabled={busy} onClick={onAbort}><Square size={14} />取消生成</Button>
          : <Button type="submit" size="sm" disabled={!sendable}><ArrowUp size={14} />{busy ? '处理中…' : '发送'}</Button>}
      </div>
    </div>
  </form>;
}
