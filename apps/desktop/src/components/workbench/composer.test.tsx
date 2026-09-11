// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { Composer } from './composer';

afterEach(cleanup);
const base = { taskId: 'task-a', draft: '任务 A 草稿', onDraft: vi.fn(), onSend: vi.fn(), onAbort: vi.fn(), canSend: true, running: false, busy: false };

describe('Composer', () => {
  it('sends with Cmd/Ctrl+Enter, never plain Enter or IME confirmation', () => {
    const onSend = vi.fn();
    render(<Composer {...base} onSend={onSend} />);
    const input = screen.getByRole('textbox', { name: '消息' });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onSend).not.toHaveBeenCalled();
    fireEvent.compositionStart(input);
    fireEvent.keyDown(input, { key: 'Enter', metaKey: true });
    expect(onSend).not.toHaveBeenCalled();
    fireEvent.compositionEnd(input);
    fireEvent.keyDown(input, { key: 'Enter', ctrlKey: true, isComposing: true });
    expect(onSend).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: 'Enter', metaKey: true });
    fireEvent.keyDown(input, { key: 'Enter', ctrlKey: true });
    expect(onSend).toHaveBeenCalledTimes(2);
  });

  it('retains controlled drafts, switches task content, and prevents busy or unavailable sends', () => {
    const onSend = vi.fn();
    const { rerender } = render(<Composer {...base} onSend={onSend} />);
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('任务 A 草稿');
    rerender(<Composer {...base} taskId="task-b" draft="任务 B 草稿" busy onSend={onSend} />);
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('任务 B 草稿');
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter', ctrlKey: true });
    expect(onSend).not.toHaveBeenCalled();
    rerender(<Composer {...base} canSend={false} disabledReason="请先加载会话" onSend={onSend} />);
    expect(screen.getByText('请先加载会话')).toBeTruthy();
    expect((screen.getByRole('button', { name: '发送' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('keeps drafting available while generating and offers cancellation', () => {
    const onAbort = vi.fn(); const onDraft = vi.fn();
    render(<Composer {...base} running onAbort={onAbort} onDraft={onDraft} />);
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'next question' } });
    expect(onDraft).toHaveBeenCalledWith('next question');
    fireEvent.click(screen.getByRole('button', { name: '取消生成' }));
    expect(onAbort).toHaveBeenCalledTimes(1);
  });
});

it('allows attachment-only sends and routes exact local commands without sending them to OMP',()=>{
 const onSend=vi.fn(),onFiles=vi.fn(),onDraft=vi.fn();
 const {rerender}=render(<Composer {...base} draft="" attachmentIds={['file-1']} onSend={onSend}/>);
 fireEvent.click(screen.getByRole('button',{name:'发送'}));
 expect(onSend).toHaveBeenCalledTimes(1);
 rerender(<Composer {...base} draft="/files" onFiles={onFiles} onSend={onSend} onDraft={onDraft}/>);
 fireEvent.keyDown(screen.getByRole('textbox',{name:'消息'}),{key:'Enter',metaKey:true});
 expect(onFiles).toHaveBeenCalledTimes(1);
 expect(onSend).toHaveBeenCalledTimes(1);
 expect(onDraft).toHaveBeenCalledWith('');
});
