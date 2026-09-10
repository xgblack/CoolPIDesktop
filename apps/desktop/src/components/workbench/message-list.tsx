import { Children, isValidElement, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { invoke } from '@tauri-apps/api/core';
import { ArrowDown, Check, Copy, RefreshCw, Terminal } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { Message } from '../../../../../packages/host-contract/src';

interface MessageListProps {
  taskKey: string;
  messages: Message[];
  streamingText: string;
  running: boolean;
  hasMore: boolean;
  loading: boolean;
  onMore: () => void;
  onRefresh: () => void;
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

function Markdown({ text, onError }: { text: string; onError: MessageListProps['onError'] }) {
  return <div className="min-w-0 break-words text-sm leading-relaxed [overflow-wrap:anywhere] [&_p]:my-2 [&_ul]:my-2 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-2 [&_ol]:list-decimal [&_ol]:pl-5 [&_li]:my-1 [&_h1]:my-4 [&_h1]:text-xl [&_h1]:font-semibold [&_h2]:my-3 [&_h2]:text-lg [&_h2]:font-semibold [&_h3]:my-3 [&_h3]:font-semibold [&_blockquote]:my-3 [&_blockquote]:border-l-2 [&_blockquote]:border-border [&_blockquote]:pl-4 [&_blockquote]:text-muted-foreground [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_code]:font-mono [&_code]:text-xs [&_hr]:my-4 [&_hr]:border-border">
    <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml urlTransform={url => safeUrl(url) ?? ''} components={{
      pre: ({ children }) => <CodeBlock onError={onError}>{children}</CodeBlock>,
      a: ({ href, children }) => {
        const url = href && safeUrl(href);
        return url ? <a className="text-primary underline underline-offset-4 focus-visible:outline-2 focus-visible:outline-ring" href={url}
          onClick={event => { event.preventDefault(); void invoke('open_external_link', { url }).catch(onError); }}
          onAuxClick={event => { event.preventDefault(); if (event.button === 1) void invoke('open_external_link', { url }).catch(onError); }}
          onContextMenu={event => event.preventDefault()}>{children}</a> : <span>{children}</span>;
      },
      img: ({ alt }) => <span className="text-xs text-muted-foreground">[图片未加载{alt ? `：${alt}` : ''}]</span>,
      table: ({ children }) => <div className="my-3 overflow-x-auto"><table className="w-full border-collapse text-left text-xs [&_th]:border [&_th]:border-border [&_th]:bg-muted [&_th]:p-2 [&_td]:border [&_td]:border-border [&_td]:p-2">{children}</table></div>,
    }}>{text}</ReactMarkdown>
  </div>;
}

function MessageContent({ message, onError }: { message: Message; onError: MessageListProps['onError'] }) {
  const blocks = typeof message.content === 'string' ? [{ type: 'text', text: message.content }]
    : Array.isArray(message.content) ? message.content : [{ type: 'unknown', value: message.content }];
  return <>{blocks.map((raw: unknown, index: number) => {
    const block = raw && typeof raw === 'object' ? raw as Record<string, unknown> : { type: 'text', text: String(raw ?? '') };
    if (block.type === 'thinking' || block.type === 'reasoning') {
      const thought = typeof block.thinking === 'string' ? block.thinking : typeof block.text === 'string' ? block.text : '';
      return <details key={index} className="my-2 text-muted-foreground"><summary className="cursor-pointer rounded py-1 text-xs focus-visible:outline-2 focus-visible:outline-ring">思考过程</summary><Markdown text={thought} onError={onError} /></details>;
    }
    if (typeof block.text === 'string') return <Markdown key={index} text={block.text} onError={onError} />;
    if (block.type === 'image') return <p key={index} className="my-2 text-xs text-muted-foreground">[图片未加载]</p>;
    if (block.type === 'toolCall' || block.type === 'tool_use') return <details key={index} className="my-2 border-l-2 border-border pl-3">
      <summary className="cursor-pointer rounded py-1 text-xs focus-visible:outline-2 focus-visible:outline-ring">工具调用：{String(block.name ?? '工具')}</summary>
      <pre className="max-h-80 overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(block.arguments ?? block.input ?? {}, null, 2)}</pre>
    </details>;
    return <pre key={index} className="max-h-80 overflow-auto whitespace-pre-wrap break-all text-xs text-muted-foreground">{JSON.stringify(block.value ?? block, null, 2)}</pre>;
  })}</>;
}

export function MessageList({ taskKey, messages, streamingText, running, hasMore, loading, onMore, onRefresh, onError }: MessageListProps) {
  const viewport = useRef<HTMLDivElement>(null);
  const bottom = useRef(true);
  const previous = useRef({ taskKey, first: messages[0], height: 0 });
  const [atBottom, setAtBottom] = useState(true);
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
  }, [taskKey, messages, streamingText, loading]);
  const jump = () => {
    if (viewport.current) viewport.current.scrollTop = viewport.current.scrollHeight;
    bottom.current = true; setAtBottom(true);
  };
  return <div className="relative min-h-0 flex-1">
    <div ref={viewport} role="region" aria-label="对话消息" tabIndex={0} className="h-full overflow-y-auto overscroll-contain focus-visible:outline-2 focus-visible:outline-ring"
      onScroll={() => { const e = viewport.current; if (!e) return; bottom.current = e.scrollHeight - e.scrollTop - e.clientHeight < 48; setAtBottom(bottom.current); }}>
      <div className="mx-auto max-w-[880px] px-4 py-5 sm:px-8">
        <div className="mb-4 flex items-center justify-center gap-2">
          {hasMore && <Button variant="outline" size="sm" disabled={loading || running} onClick={onMore}>{loading ? '正在加载…' : '加载更多消息'}</Button>}
          {messages.length > 0 && <Button variant="ghost" size="sm" disabled={loading || running} onClick={onRefresh}><RefreshCw size={14} />刷新历史</Button>}
        </div>
        {loading && messages.length === 0 && <div role="status" className="space-y-3 py-6"><span className="text-xs text-muted-foreground">正在加载历史…</span><div className="h-4 w-2/3 rounded bg-muted" /><div className="h-4 w-4/5 rounded bg-muted" /></div>}
        {!loading && messages.length === 0 && !streamingText && <div className="py-16 text-center"><p className="text-sm font-medium">从一个问题开始</p><p className="mt-2 text-xs text-muted-foreground">加载会话后，在下方输入任务或问题。</p></div>}
        {messages.map((message, index) => {
          const tool = message.role === 'toolResult' || message.role === 'tool';
          return <article key={`${index}:${message.timestamp ?? ''}:${message.role}:${message.toolCallId ?? ''}`} className={`min-w-0 py-4 ${tool ? 'border-l-2 border-border pl-3' : ''}`}>
            <div className="mb-2 flex items-center gap-2 text-xs font-medium text-muted-foreground">{tool && <Terminal size={14} />}{message.role === 'user' ? '你' : message.role === 'assistant' ? '助手' : tool ? '工具结果' : message.role}</div>
            <MessageContent message={message} onError={onError} />
          </article>;
        })}
        {streamingText && <article className="min-w-0 py-4"><div className="mb-2 text-xs font-medium text-muted-foreground">助手 · 正在生成</div><Markdown text={streamingText} onError={onError} /></article>}
        {running && !streamingText && <p role="status" className="py-3 text-xs text-muted-foreground">正在处理…</p>}
      </div>
    </div>
    {!atBottom && <Button className="absolute bottom-3 left-1/2 -translate-x-1/2 shadow-sm" size="sm" variant="secondary" onClick={jump}><ArrowDown size={14} />回到底部</Button>}
  </div>;
}
