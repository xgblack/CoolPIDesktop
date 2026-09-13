import {useEffect, useState} from 'react';
import {Popover} from 'radix-ui';
import {Check, Clock3, Copy, Database, GitFork, LoaderCircle} from 'lucide-react';
import {compactTokenLabel, completedAtLabel, durationLabel, messageText, turnMetadata} from './turn-metadata';
import type {Message} from '../../../../../packages/host-contract/src';
import './turn-footer.css';

export function RunningClock({startedAt, itemCount}: {startedAt?: number; itemCount?: number}) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {setNow(Date.now());const timer = setInterval(() => setNow(Date.now()), 1000);return () => clearInterval(timer);}, [startedAt]);
  const count = itemCount === undefined ? '' : ` · ${itemCount}项`;
  return <span className="turn-running-clock"><Clock3 size={14}/>{startedAt === undefined ? `正在执行${count || '…'}` : `正在执行 · ${durationLabel(now - startedAt)}${count}`}</span>;
}

export function TurnFooter({messages, answers, user, onFork, forkDisabled, onError}: {
  messages: Message[]; answers: Message[]; user?: Message; onFork?: (timestamp: number) => Promise<void>;
  forkDisabled?: boolean; onError: (error: unknown) => void;
}) {
  const [copied, setCopied] = useState(false), [forking, setForking] = useState(false);
  const {usage, completedAt, durationMs, last, partial} = turnMetadata(messages, user);
  const text = answers.map(messageText).filter(Boolean).join('\n\n');
  useEffect(() => {if (!copied) return;const timer = setTimeout(() => setCopied(false), 2000);return () => clearTimeout(timer);}, [copied]);
  const copy = async () => {try {await navigator.clipboard.writeText(text);setCopied(true);} catch(error) {onError(error);}};
  const fork = async () => {if (!onFork || last?.timestamp === undefined || forking) return;setForking(true);try {await onFork(last.timestamp);} catch(error) {onError(error);} finally {setForking(false);}};
  const number = (value?: number) => value === undefined ? '未知' : value.toLocaleString('zh-CN');
  const fields: [string, string][] = [['输入',number(usage.inputTokens)],['输出',number(usage.outputTokens)],['缓存读取',number(usage.cacheReadTokens)],['缓存写入',number(usage.cacheWriteTokens)],['推理',number(usage.reasoningTokens)],['费用',usage.cost === undefined ? '未知' : `$${usage.cost.toFixed(6)}`]];
  return <footer className="turn-footer" aria-label="本轮对话信息">
    <button type="button" className="turn-action" aria-label={copied?'已复制回答':'复制回答'} title={copied?'已复制':'复制回答'} disabled={!text} onClick={() => void copy()}>{copied?<Check size={14}/>:<Copy size={14}/>}</button>
    {onFork && <button type="button" className="turn-action" aria-label="从此处 Fork" title="从此处创建新会话（沿用工作目录，不回滚文件）" disabled={forkDisabled || forking || !user || last?.timestamp === undefined || !answers.length || last.stopReason === 'error' || last.stopReason === 'aborted'} onClick={() => void fork()}>{forking?<LoaderCircle size={14} className="animate-spin"/>:<GitFork size={14}/>}</button>}
    <Popover.Root><Popover.Trigger asChild><button type="button" className="turn-action turn-stat" aria-label="查看本轮用量"><Database size={13}/><span>{compactTokenLabel(usage.totalTokens)}{usage.totalTokens === undefined ? '' : ' tok'}</span></button></Popover.Trigger><Popover.Portal><Popover.Content className="turn-usage-panel" side="top" align="start" sideOffset={8} collisionPadding={12} aria-label="本轮用量"><strong>本轮用量</strong><dl>{fields.map(([label,value])=><div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl><p>{partial?'本轮历史或用量不完整；缺失项不作估算。':'汇总本轮模型调用，包含工具调用前后的回复。'}</p><Popover.Arrow className="fill-popover"/></Popover.Content></Popover.Portal></Popover.Root>
    <span className="turn-stat" title="从本轮用户消息到最终回复完成的时间"><Clock3 size={13}/>{durationMs === undefined?'用时未知':`用时 ${durationLabel(durationMs)}`}</span>
    <span className="turn-completed">{completedAt === undefined?'完成时间未知':<time dateTime={new Date(completedAt).toISOString()} title={new Date(completedAt).toLocaleString('zh-CN')}>完成于 {completedAtLabel(completedAt)}</time>}</span>
  </footer>;
}
