// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MessageList } from './message-list';
import { invoke } from '@tauri-apps/api/core';

globalThis.ResizeObserver = class {observe(){} unobserve(){} disconnect(){}};

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });
const base = { taskKey: 'task-a:run-1', messages: [], streamingText: '', running: false, hasMore: false, loading: false, onMore: vi.fn(), onError: vi.fn() };

describe('message rendering', () => {
  it('does not render raw HTML, dangerous links, or remote image elements', async () => {
    const { container } = render(<MessageList {...base} messages={[{ role: 'assistant', content: '<script>alert(1)</script>\n\n[bad](javascript:alert%281%29) ![private image](https://example.com/tracking.png) [docs](https://example.com/docs)' }]} />);
    expect(container.querySelector('script')).toBeNull();
    expect(container.querySelector('img')).toBeNull();
    expect(screen.getByText('bad').closest('a')).toBeNull();
    fireEvent.click(screen.getByRole('link', { name: 'docs' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('open_external_link', { url: 'https://example.com/docs' }));
  });

  it('renders owned base64 images while keeping remote images blocked', () => {
    const pixel = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jp1sAAAAASUVORK5CYII=';
    const { container } = render(<MessageList {...base} messages={[{ role: 'assistant', content: `![owned](${pixel})\n\n![remote](https://example.com/a.png)` }]} />);
    expect(container.querySelectorAll('.message-image img')).toHaveLength(1);
    expect(screen.getByText(/图片不可用/)).toBeTruthy();
  });

  it('copies exact fenced code and folds thinking separately from the answer', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const { container } = render(<MessageList {...base} messages={[{ role: 'assistant', content: [{ type: 'thinking', thinking: 'private reasoning text' }, { type: 'text', text: '```ts\nconst x = 1;\n```' }] }]} />);
    expect(container.querySelector('details')?.open).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: '复制代码' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('const x = 1;\n'));
  });

  it('applies the explicit default Thinking expansion preference',()=>{
    const message={role:'assistant',content:[{type:'thinking',thinking:'inspect privately'},{type:'text',text:'answer'}]};
    const {container,rerender}=render(<MessageList {...base} messages={[message]}/>);
    expect(container.querySelector('.activity-row')?.hasAttribute('open')).toBe(false);
    rerender(<MessageList {...base} messages={[message]} thinkingExpanded/>);
    expect(container.querySelector('.activity-row')?.hasAttribute('open')).toBe(true);
  });

  it('keeps user scroll position and keyboard focus during streaming, and resets on task switch', () => {
    const { rerender } = render(<MessageList {...base} streamingText="first" running />);
    const region = screen.getByRole('region', { name: '对话消息' });
    Object.defineProperties(region, { scrollHeight: { value: 1200, configurable: true }, clientHeight: { value: 400, configurable: true } });
    region.scrollTop = 100;
    fireEvent.scroll(region);
    region.focus();
    rerender(<MessageList {...base} streamingText="first and second" running />);
    expect(region.scrollTop).toBe(100);
    expect(document.activeElement).toBe(region);
    expect(screen.getByRole('button', { name: '回到底部' })).toBeTruthy();
    rerender(<MessageList {...base} taskKey="task-b:run-2" streamingText="other task" running />);
    expect(region.scrollTop).toBe(1200);
  });

  it('renders markdown during streaming and keeps completed blocks mounted', () => {
    const {container,rerender}=render(<MessageList {...base} streamingText="## 实时" running/>);
    expect(screen.getByRole('heading',{name:'实时'})).toBeTruthy();
    rerender(<MessageList {...base} streamingText={'## 实时\n\n下一段'} running/>);
    expect(screen.getByText('下一段')).toBeTruthy();
    const heading=screen.getByRole('heading',{name:'实时'});
    rerender(<MessageList {...base} streamingText={'## 实时\n\n下一段\n\n第三段'} running/>);
    expect(screen.getByRole('heading',{name:'实时'})).toBe(heading);
    rerender(<MessageList {...base} streamingText={'## 实时\n\n下一段'} running={false}/>);
    expect(screen.getByRole('heading',{name:'实时'})).toBeTruthy();
  });

  it('renders an unfinished fenced block as code while chunks arrive', () => {
    const { container, rerender } = render(<MessageList {...base} streamingText={'```ts\nconst answer ='} running />);
    expect(screen.getByText('const answer =')).toBeTruthy();
    expect(container.querySelector('.assistant-markdown-renderer pre')).toBeTruthy();
    rerender(<MessageList {...base} streamingText={'```ts\nconst answer = 42\n```'} running />);
    expect(screen.getByText('const answer = 42')).toBeTruthy();
  });

  it('keeps the completed stream visible until persisted history takes over', () => {
    const text='不会在历史同步时闪烁';
    const {rerender}=render(<MessageList {...base} streamingText={text} running={false} startedAt={1000}/>);
    expect(screen.getByText(text)).toBeTruthy();
    rerender(<MessageList {...base} streamingText={text} running={false} startedAt={1000} messages={[
      {role:'user',content:'问题',timestamp:1000},{role:'assistant',content:text,timestamp:1200},
    ]}/>);
    expect(screen.getAllByText(text)).toHaveLength(1);
    expect(screen.queryByLabelText('当前执行轮次')).toBeNull();
  });

  it('hands a tool-split stream to persisted assistant messages without duplicating it', () => {
    const {rerender}=render(<MessageList {...base} streamingText="准备检查最终结论" running/>);
    rerender(<MessageList {...base} streamingText="准备检查最终结论" running={false} messages={[
      {role:'user',content:'检查代码'},{role:'assistant',content:'准备检查'},
      {role:'toolResult',content:'结果'},{role:'assistant',content:'最终结论'},
    ]}/>);
    expect(screen.queryByLabelText('当前执行轮次')).toBeNull();
    expect(screen.getAllByText(/准备检查|最终结论/)).toHaveLength(2);
  });

  it('reports clipboard rejection and disables pagination while generation is active', async () => {
    const failure = new Error('Clipboard permission denied');
    const onError = vi.fn();
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: vi.fn().mockRejectedValue(failure) }, configurable: true });
    render(<MessageList {...base} onError={onError} hasMore running messages={[{ role: 'assistant', content: '```\nx\n```' }]} />);
    expect((screen.getByRole('button', { name: '加载更多消息' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: '复制代码' }));
    await waitFor(() => expect(onError).toHaveBeenCalledWith(failure));
  });

  it('shows a stable minimap and quotes only text selected inside the transcript', async()=>{
    const onQuote=vi.fn();
    render(<MessageList {...base} onQuote={onQuote} messages={[
      {role:'user',content:'first question'},
      {role:'assistant',content:'first answer'},
      {role:'user',content:'second question'},
      {role:'assistant',content:'second answer'},
    ]}/>);
    expect(screen.getByRole('navigation',{name:'对话导航'})).toBeTruthy();
    expect(screen.getAllByRole('button',{name:/跳转到第/})).toHaveLength(4);
    const text=screen.getByText('first answer').firstChild!;
    const range=document.createRange();range.selectNodeContents(text);
    const selection=window.getSelection()!;selection.removeAllRanges();selection.addRange(range);
    fireEvent.mouseUp(screen.getByRole('region',{name:'对话消息'}));
    fireEvent.click(await screen.findByRole('button',{name:'引用到输入框'}));
    expect(onQuote).toHaveBeenCalledWith('first answer');
  });
});

it('summarizes the whole turn once, copies only its final answer and forks its exact boundary', async () => {
  const writeText = vi.fn().mockResolvedValue(undefined), onFork = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', {value:{writeText},configurable:true});
  const messages = [
    {role:'user',content:'Inspect the file',timestamp:1000},
    {role:'assistant',timestamp:1100,completedAt:1500,usage:{input:100,output:10,totalTokens:120,cacheRead:10,cacheWrite:0,cost:{total:0.01}},content:[{type:'thinking',thinking:'Read first'},{type:'text',text:'I will inspect it'},{type:'toolCall',id:'c1',name:'read'}]},
    {role:'toolResult',toolCallId:'c1',content:'File contents',timestamp:2000},
    {role:'assistant',timestamp:2100,completedAt:4000,usage:{input:200,output:20,totalTokens:240,cacheRead:20,cacheWrite:0,cost:{total:0.02}},content:[{type:'thinking',thinking:'Review done'},{type:'text',text:'**Final** answer'}]},
  ];
  render(<MessageList {...base} messages={messages} onFork={onFork} query="final"/>);
  expect(screen.getAllByLabelText('本轮对话信息')).toHaveLength(1);
  expect(screen.getByText('360 tok')).toBeTruthy();
  expect(screen.getByText('用时 3秒')).toBeTruthy();
  fireEvent.click(screen.getByLabelText('复制回答'));
  await waitFor(()=>expect(writeText).toHaveBeenCalledWith('**Final** answer'));
  fireEvent.click(screen.getByLabelText('从此处 Fork'));
  await waitFor(()=>expect(onFork).toHaveBeenCalledWith(2100));
  fireEvent.click(screen.getByLabelText('查看本轮用量'));
  expect(await screen.findByText('300')).toBeTruthy();
  expect(screen.getByText('$0.030000')).toBeTruthy();
});

it('does not claim missing usage or timestamps are zero, and reports fork failures', async () => {
  const error = new Error('session changed'), onError = vi.fn();
  render(<MessageList {...base} onError={onError} onFork={vi.fn().mockRejectedValue(error)} messages={[{role:'user',content:'ask'},{role:'assistant',content:'answer',timestamp:1234}]} />);
  expect(screen.getByText('用时未知')).toBeTruthy();
  expect(screen.getByText('完成时间未知')).toBeTruthy();
  fireEvent.click(screen.getByLabelText('从此处 Fork'));
  await waitFor(()=>expect(onError).toHaveBeenCalledWith(error));
  fireEvent.click(screen.getByLabelText('查看本轮用量'));
  expect(await screen.findAllByText('未知')).toHaveLength(6);
});

it('keeps a live wall clock through tools, streaming and switching tasks, then stops it', () => {
  vi.useFakeTimers();
  try {
    vi.setSystemTime(10000);
    const {rerender} = render(<MessageList {...base} running startedAt={1000} tools={[{id:'t',name:'read',status:'running',args:{},seq:1}]}/>);
    expect(screen.getByText('正在执行 · 9秒 · 1项')).toBeTruthy();
    act(()=>vi.advanceTimersByTime(2000));
    expect(screen.getByText('正在执行 · 11秒 · 1项')).toBeTruthy();
    rerender(<MessageList {...base} running startedAt={1000} streamingText="response"/>);
    expect(screen.getByText('正在执行 · 11秒 · 0项')).toBeTruthy();
    rerender(<MessageList {...base} taskKey="b" running startedAt={11000}/>);
    expect(screen.getByText('正在执行 · 1秒 · 0项')).toBeTruthy();
    rerender(<MessageList {...base} running startedAt={1000}/>);
    expect(screen.getByText('正在执行 · 11秒 · 0项')).toBeTruthy();
    rerender(<MessageList {...base}/>);
    expect(screen.queryByText(/正在执行/)).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
  } finally {vi.useRealTimers();}
});
