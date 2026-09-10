// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MessageList } from './message-list';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });
const base = { taskKey: 'task-a:run-1', messages: [], streamingText: '', running: false, hasMore: false, loading: false, onMore: vi.fn(), onRefresh: vi.fn(), onError: vi.fn() };

describe('message rendering', () => {
  it('does not render raw HTML, dangerous links, or remote image elements', async () => {
    const { container } = render(<MessageList {...base} messages={[{ role: 'assistant', content: '<script>alert(1)</script>\n\n[bad](javascript:alert%281%29) ![private image](https://example.com/tracking.png) [docs](https://example.com/docs)' }]} />);
    expect(container.querySelector('script')).toBeNull();
    expect(container.querySelector('img')).toBeNull();
    expect(screen.getByText('bad').closest('a')).toBeNull();
    fireEvent.click(screen.getByRole('link', { name: 'docs' }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('open_external_link', { url: 'https://example.com/docs' }));
  });

  it('copies exact fenced code and folds thinking separately from the answer', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const { container } = render(<MessageList {...base} messages={[{ role: 'assistant', content: [{ type: 'thinking', thinking: 'private reasoning text' }, { type: 'text', text: '```ts\nconst x = 1;\n```' }] }]} />);
    expect(container.querySelector('details')?.open).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: '复制代码' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('const x = 1;\n'));
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

  it('reports clipboard rejection and disables pagination while generation is active', async () => {
    const failure = new Error('Clipboard permission denied');
    const onError = vi.fn();
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: vi.fn().mockRejectedValue(failure) }, configurable: true });
    render(<MessageList {...base} onError={onError} hasMore running messages={[{ role: 'assistant', content: '```\nx\n```' }]} />);
    expect((screen.getByRole('button', { name: '加载更多消息' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: '复制代码' }));
    await waitFor(() => expect(onError).toHaveBeenCalledWith(failure));
  });
});
