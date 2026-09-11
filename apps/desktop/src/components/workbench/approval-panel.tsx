import { useState } from 'react';
import { CircleHelp } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import type { PendingUiRequest } from '../../../../../packages/host-contract/src';

export interface ApprovalPanelProps {
  requests: PendingUiRequest[];
  busy: boolean;
  onRespond: (id: string, value: string | null, confirmed: boolean | null, cancelled: boolean) => void;
}

function Approval({ request, busy, onRespond }: { request: PendingUiRequest } & Omit<ApprovalPanelProps, 'requests'>) {
  const [value, setValue] = useState('');
  const confirmation = request.method === 'confirm';
  const selection = request.method === 'select';
  return <fieldset disabled={busy} className="min-w-0 border-l-2 border-primary px-4 py-2">
    <legend className="flex items-center gap-2 text-sm font-medium"><CircleHelp size={16} />{request.title || (confirmation ? 'OMP 扩展请求确认' : 'OMP 扩展请求')}</legend>
    <p className="mt-1 text-xs text-muted-foreground">此请求由 OMP 扩展明确发起，不代表客户端对命令或文件的安全审批。</p>
    {request.message && <p className="my-2 whitespace-pre-wrap break-words text-sm leading-relaxed [overflow-wrap:anywhere]">{request.message}</p>}
    {!confirmation && <div className="my-3">
      <label className="mb-1.5 block text-xs text-muted-foreground" htmlFor={`approval-${request.id}`}>{selection ? '选择选项' : '回复内容'}</label>
      {selection ? <Select value={value} onValueChange={setValue} disabled={busy}><SelectTrigger id={`approval-${request.id}`} className="w-full"><SelectValue placeholder="请选择，不会自动确认" /></SelectTrigger><SelectContent>{request.options?.map((option, index) => <SelectItem key={index} value={String(index)}>{option}</SelectItem>)}</SelectContent></Select>
        : <Textarea id={`approval-${request.id}`} value={value} onChange={event => setValue(event.target.value)} className="max-h-56 min-h-20" />}
    </div>}
    <div className="mt-3 flex flex-wrap items-center gap-2">
      {confirmation ? <><Button variant="outline" size="sm" disabled={busy} onClick={() => onRespond(request.id, null, false, false)}>拒绝</Button><Button size="sm" disabled={busy} onClick={() => onRespond(request.id, null, true, false)}>允许</Button></>
        : <Button size="sm" disabled={busy || (selection && value === '')} onClick={() => onRespond(request.id, selection ? request.options?.[Number(value)] ?? null : value, null, false)}>提交回复</Button>}
      <Button variant="ghost" size="sm" disabled={busy} onClick={() => onRespond(request.id, null, null, true)}>取消请求</Button>
    </div>
  </fieldset>;
}

export function ApprovalPanel({ requests, busy, onRespond }: ApprovalPanelProps) {
  if (requests.length === 0) return null;
  return <section aria-label="待处理请求" className="max-h-[40vh] shrink-0 space-y-4 overflow-y-auto border-t border-border bg-muted/30 px-4 py-3 sm:px-8">
    <div className="mx-auto max-w-[880px] space-y-4">{requests.map(request => <Approval key={request.id} request={request} busy={busy} onRespond={onRespond} />)}</div>
  </section>;
}
