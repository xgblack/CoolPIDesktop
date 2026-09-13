import { Children, isValidElement, useLayoutEffect, useRef, useState, memo, useMemo, type ReactNode } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { invoke } from '@tauri-apps/api/core';
import { ArrowDown, Check, Copy } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {ActivityRow,activitySummary} from './activity-row';
import type { Message, ToolActivity } from '../../../../../packages/host-contract/src';

import {RunningClock, TurnFooter} from './turn-footer';
import {durationLabel, turnMetadata} from './turn-metadata';

interface MessageListProps {
  startedAt?: number;
  onFork?: (timestamp: number) => Promise<void>;
  forkDisabled?: boolean;
  taskKey: string;
  tools?: ToolActivity[];
  messages: Message[];
  streamingText: string;
  running: boolean;
  hasMore: boolean;
  loading: boolean;
  onMore: () => void;
  query?: string;
  onError: (error: unknown) => void;
}

function plainText(children: ReactNode): string {
  return Children.toArray(children).map(child => isValidElement<{ children?: ReactNode }>(child)
    ? plainText(child.props.children) : typeof child === 'string' || typeof child === 'number' ? String(child) : '').join('');
}

function CodeBlock({ children, onError }: { children: ReactNode; onError: MessageListProps['onError'] }) {
  const [copied, setCopied] = useState(false);
  const code = plainText(children);
  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
    } catch (error) { onError(error); }
  }
  return <div className="my-3 min-w-0 overflow-hidden rounded-md border border-border bg-muted/50">
    <div className="flex items-center justify-between border-b border-border px-3 py-1 text-xs text-muted-foreground">
      <span>代码</span><Button variant="ghost" size="sm" onClick={() => void copy()} aria-label={copied ? '已复制代码' : '复制代码'}>
        {copied ? <Check size={14} /> : <Copy size={14} />}{copied ? '已复制' : '复制'}
      </Button>
    </div>
    <pre className="overflow-x-auto p-3 font-mono text-xs leading-relaxed [&_code]:!bg-transparent [&_code]:!p-0">{children}</pre>
  </div>;
}

function safeUrl(value: string): string | undefined {
  try {
    const url = new URL(value);
    return (url.protocol === 'https:' || url.protocol === 'http:') && !url.username && !url.password ? url.href : undefined;
  } catch { return undefined; }
}

export const Markdown=memo(function Markdown({ text, onError }: { text: string; onError: MessageListProps['onError'] }) {
  return <div className="min-w-0 break-words text-sm leading-relaxed [overflow-wrap:anywhere] [&_p]:my-2 [&_ul]:my-2 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-2 [&_ol]:list-decimal [&_ol]:pl-5 [&_li]:my-1 [&_h1]:my-4 [&_h1]:text-xl [&_h1]:font-semibold [&_h2]:my-3 [&_h2]:text-lg [&_h2]:font-semibold [&_h3]:my-3 [&_h3]:font-semibold [&_blockquote]:my-3 [&_blockquote]:border-l-2 [&_blockquote]:border-border [&_blockquote]:pl-4 [&_blockquote]:text-muted-foreground [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_code]:font-mono [&_code]:text-xs [&_hr]:my-4 [&_hr]:border-border">
    <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml urlTransform={url => safeUrl(url) ?? ''} components={{
      pre: ({ children }) => <CodeBlock onError={onError}>{children}</CodeBlock>,
      a: ({ href, children }) => {
        const url = href && safeUrl(href);
        return url ? <a className="message-link focus-visible:outline-2 focus-visible:outline-ring" href={url}
          onClick={event => { event.preventDefault(); void invoke('open_external_link', { url }).catch(onError); }}
          onAuxClick={event => { event.preventDefault(); if (event.button === 1) void invoke('open_external_link', { url }).catch(onError); }}
          onContextMenu={event => event.preventDefault()}>{children}</a> : <span>{children}</span>;
      },
      img: ({ alt }) => <span className="text-xs text-muted-foreground">[图片未加载{alt ? `：${alt}` : ''}]</span>,
      table: ({ children }) => <div className="my-3 overflow-x-auto"><table className="w-full border-collapse text-left text-xs [&_th]:border [&_th]:border-border [&_th]:bg-muted [&_th]:p-2 [&_td]:border [&_td]:border-border [&_td]:p-2">{children}</table></div>,
    }}>{text}</ReactMarkdown>
  </div>;
},(a,b)=>a.text===b.text);

const MessageContent=memo(function MessageContent({ message, results, onError }: { message: Message; results?: Map<string,Message>; onError: MessageListProps['onError'] }) {
  const blocks = typeof message.content === 'string' ? [{ type: 'text', text: message.content }]
    : Array.isArray(message.content) ? message.content : [{ type: 'unknown', value: message.content }];
  return <>{blocks.map((raw: unknown, index: number) => {
    const block = raw && typeof raw === 'object' ? raw as Record<string, unknown> : { type: 'text', text: String(raw ?? '') };
    if (block.type === 'thinking' || block.type === 'reasoning') {
      const thought = typeof block.thinking === 'string' ? block.thinking : typeof block.text === 'string' ? block.text : '';
      return <ActivityRow key={index} kind="thinking" preview={activitySummary(thought)}><Markdown text={thought} onError={onError}/></ActivityRow>;
    }
    if (typeof block.text === 'string') return <Markdown key={index} text={block.text} onError={onError} />;
    if (block.type === 'image') return <p key={index} className="my-2 text-xs text-muted-foreground">[图片未加载]</p>;
    if (block.type === 'toolCall' || block.type === 'tool_use') {const result=results?.get(String(block.id??''));return <ActivityRow key={index} name={String(block.name??'工具')} state={result?((result as Message & {isError?:boolean}).isError?'failed':'succeeded'):undefined} preview={activitySummary(block.arguments??block.input)}><pre>{JSON.stringify(block.arguments??block.input??{},null,2)}</pre>{result&&<MessageContent message={result} onError={onError}/>}</ActivityRow>;}
    return <pre key={index} className="max-h-80 overflow-auto whitespace-pre-wrap break-all text-xs text-muted-foreground">{JSON.stringify(block.value ?? block, null, 2)}</pre>;
  })}</>;
},(a,b)=>a.message===b.message&&a.results===b.results);

const StreamingAnswer=memo(function StreamingAnswer({text,complete,onError}:{text:string;complete:boolean;onError:MessageListProps['onError']}){
 return complete?<Markdown text={text} onError={onError}/>:<div className="streaming-text">{text}</div>;
},(a,b)=>a.text===b.text&&a.complete===b.complete);

export function MessageList({ taskKey, tools=[], messages, streamingText, running, hasMore, loading, onMore, query='', onError, startedAt, onFork, forkDisabled }: MessageListProps) {
  const viewport = useRef<HTMLDivElement>(null);
  const bottom = useRef(true);
  const previous = useRef({ taskKey, first: messages[0], height: 0 });
  const [atBottom, setAtBottom] = useState(true);
  const normalized = query.trim().toLowerCase();
  const visibleMessages = normalized ? messages.filter(message => JSON.stringify(message.content).toLowerCase().includes(normalized)) : messages;
  const lastUserIndex=messages.reduce((found,message,index)=>message.role==='user'?index:found,-1);
  const latestAssistant=[...messages.slice(lastUserIndex+1)].reverse().find(message=>message.role==='assistant');
  const persistedCurrentAnswer=!running&&!!latestAssistant;
  const liveText=persistedCurrentAnswer?'':streamingText;
  const results=useMemo(()=>new Map(messages.filter(m=>m.toolCallId).map(m=>[m.toolCallId!,m])),[messages]);
  const calledIds=useMemo(()=>new Set(messages.flatMap(m=>Array.isArray(m.content)?m.content.filter(b=>b&&['toolCall','tool_use'].includes(b.type)).map(b=>b.id):[])),[messages]);
  const transcript=useMemo(()=>{
    const rows:{user?:Message;prompt?:Message;activity:Message[];answer:Message[];original:Message[]}[]=[];
    let prompt:Message|undefined;
    for(const message of messages){
      if(message.role==='user'){prompt=message;rows.push({user:message,activity:[],answer:[],original:[]});continue;}
      let row=rows.at(-1);if(!row||row.user){row={prompt,activity:[],answer:[],original:[]};rows.push(row);}
      row.original.push(message);
      if(message.role!=='assistant'){if(!message.toolCallId||!calledIds.has(message.toolCallId))row.activity.push(message);continue;}
      const blocks=Array.isArray(message.content)?message.content:[{type:'text',text:message.content}];
      if(row.answer.length){row.activity.push(...row.answer);row.answer=[];}
      const process=blocks.filter(b=>b&&['thinking','reasoning','toolCall','tool_use'].includes(b.type));
      const body=blocks.filter(b=>!b||!['thinking','reasoning','toolCall','tool_use'].includes(b.type));
      if(process.length)row.activity.push({...message,content:process});
      if(body.length){
        // Text preceding another tool step belongs in the expandable process.
        row.answer.push({...message,content:body});
      }
      if(process.some(b=>['toolCall','tool_use'].includes(b.type))){row.activity.push(...row.answer);row.answer=[];}
    }
    return normalized?rows.filter(row=>JSON.stringify(row.user?.content??row.original.map(m=>m.content)).toLowerCase().includes(normalized)):rows;
  },[messages,normalized]);
  useLayoutEffect(() => {
    const element = viewport.current;
    if (!element) return;
    const switched = previous.current.taskKey !== taskKey;
    if (switched || bottom.current) {
      element.scrollTop = element.scrollHeight;
      bottom.current = true;
      setAtBottom(true);
    } else if (messages[0] !== previous.current.first && messages.length > 0) {
      // Preserve the visible location when earlier history is prepended.
      element.scrollTop += Math.max(0, element.scrollHeight - previous.current.height);
    }
    previous.current = { taskKey, first: messages[0], height: element.scrollHeight };
  }, [taskKey, messages, liveText, loading]);
  const jump = () => {
    if (viewport.current) viewport.current.scrollTop = viewport.current.scrollHeight;
    bottom.current = true; setAtBottom(true);
  };
  return <div className="relative min-h-0 flex-1">
    <div ref={viewport} role="region" aria-label="对话消息" tabIndex={0} className="h-full overflow-y-auto overscroll-contain focus-visible:outline-2 focus-visible:outline-ring"
      onScroll={() => { const e = viewport.current; if (!e) return; bottom.current = e.scrollHeight - e.scrollTop - e.clientHeight < 48; setAtBottom(bottom.current); }}>
      <div className="message-column">
        {hasMore && <div className="conversation-search"><Button variant="outline" size="sm" disabled={loading || running} onClick={onMore}>{loading ? '正在加载…' : '加载更多消息'}</Button></div>}
        {loading && messages.length === 0 && <div role="status" className="space-y-3 py-6"><span className="text-xs text-muted-foreground">正在加载历史…</span><div className="h-4 w-2/3 rounded bg-muted" /><div className="h-4 w-4/5 rounded bg-muted" /></div>}
        {!loading && messages.length === 0 && !liveText && <div className="py-16 text-center"><p className="text-sm font-medium">从一个问题开始</p><p className="mt-2 text-xs text-muted-foreground">加载会话后，在下方输入任务或问题。</p></div>}
        {transcript.map((item,index)=>item.user?<article key={index} className="message-row message-user"><div className="user-bubble"><MessageContent message={item.user} onError={onError}/></div></article>:<section key={`${taskKey}:${item.prompt?.timestamp??index}`} className="assistant-turn">
          {!!item.activity.length&&<details className="activity-group" open={normalized?true:undefined}><summary><span>{turnMetadata(item.original,item.prompt).durationMs === undefined?`执行过程 · ${item.activity.length}项`:`已执行 · ${item.activity.length}项 · ${durationLabel(turnMetadata(item.original,item.prompt).durationMs!)}`}</span><ArrowDown size={12}/></summary><div>{item.activity.map((message,i)=><div key={i}>{message.role==='toolResult'||message.role==='tool'?<ActivityRow name={String((message as Message & {toolName?:string}).toolName??'工具输出')} state={(message as Message & {isError?:boolean}).isError?'failed':undefined}><MessageContent message={message} onError={onError}/></ActivityRow>:message.role==='system'||message.role==='custom'?<ActivityRow kind="context" preview={activitySummary(message.content)}><MessageContent message={message} onError={onError}/></ActivityRow>:<MessageContent message={message} results={results} onError={onError}/>}</div>)}</div></details>}
          {item.answer.map((message,i)=><div key={i} className="assistant-body"><MessageContent message={message} onError={onError}/></div>)}
          <TurnFooter messages={item.original} answers={item.answer} user={item.prompt} onFork={onFork} forkDisabled={forkDisabled||running||loading} onError={onError}/>
        </section>)}
        {normalized && !visibleMessages.length && !loading && <p className="py-8 text-center text-xs text-muted-foreground">没有匹配的消息</p>}
        {(running || liveText) && <section className="live-turn" aria-label="当前执行轮次" aria-busy={running}>
          {running ? <RunningClock startedAt={startedAt} itemCount={tools.length}/> : loading ? <span className="text-xs text-muted-foreground">正在同步…</span> : null}
          {running&&tools.length>0&&<details className="activity-group"><summary><span>执行详情</span><ArrowDown size={12}/></summary><div className="live-activities" aria-label="当前工具执行">{tools.map(tool=><ActivityRow key={tool.id} name={tool.name} preview={activitySummary(tool.args)} state={tool.status}><pre>{JSON.stringify({参数:tool.args,结果:tool.result},null,2)}</pre></ActivityRow>)}</div></details>}
          {liveText && <div className="assistant-body"><StreamingAnswer text={liveText} complete={!running} onError={onError}/></div>}
        </section>}
      </div>
    </div>
    {!atBottom && <Button className="absolute bottom-3 left-1/2 -translate-x-1/2 shadow-sm" size="sm" variant="secondary" onClick={jump}><ArrowDown size={14} />回到底部</Button>}
  </div>;
}
