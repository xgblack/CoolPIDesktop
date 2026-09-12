// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { Composer } from './composer';
import {searchFiles} from '@/host';
vi.mock('@/host',()=>({searchFiles:vi.fn(),hostError:(e:unknown)=>e}));

afterEach(cleanup);
const base = { taskId: 'task-a', draft: '任务 A 草稿', onDraft: vi.fn(), onSend: vi.fn(), onAbort: vi.fn(), canSend: true, running: false, busy: false };

describe('Composer', () => {
  it('keeps approval switching in the existing toolbar and preserves drafts while switching', () => {
    const onApprovalMode = vi.fn();
    const {rerender} = render(<Composer {...base} onApprovalMode={onApprovalMode} approvalMode="write"/>);
    const mode = screen.getByRole('button', {name:'审批模式'}) as HTMLButtonElement;
    expect(mode.closest('.composer-tools')).toBe(screen.getByRole('button', {name:'添加内容'}).closest('.composer-tools'));
    expect(mode.textContent).toContain('执行审批');
    rerender(<Composer {...base} onApprovalMode={onApprovalMode} approvalMode="write" approvalSwitching busy/>);
    expect((screen.getByRole('button',{name:'审批模式'}) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button',{name:'发送'}) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('textbox',{name:'消息'}) as HTMLTextAreaElement).value).toBe(base.draft);
    rerender(<Composer {...base} onApprovalMode={onApprovalMode} running/>);
    expect((screen.getByRole('button',{name:'审批模式'}) as HTMLButtonElement).disabled).toBe(true);
    rerender(<Composer {...base} onApprovalMode={onApprovalMode} approvalDisabled/>);
    expect((screen.getByRole('button',{name:'审批模式'}) as HTMLButtonElement).disabled).toBe(true);
  });
  it('sends with Enter, never modified Enter or IME confirmation', () => {
    const onSend = vi.fn();
    render(<Composer {...base} onSend={onSend} />);
    const input = screen.getByRole('textbox', { name: '消息' });
    fireEvent.keyDown(input, { key: 'Enter', shiftKey: true });
    expect(onSend).not.toHaveBeenCalled();
    fireEvent.compositionStart(input);
    fireEvent.keyDown(input, { key: 'Enter', metaKey: true });
    expect(onSend).not.toHaveBeenCalled();
    fireEvent.compositionEnd(input);
    fireEvent.keyDown(input, { key: 'Enter', ctrlKey: true, isComposing: true });
    expect(onSend).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: 'Enter', metaKey: true });
    fireEvent.keyDown(input, { key: 'Enter', ctrlKey: true });
    expect(onSend).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onSend).toHaveBeenCalledTimes(1);
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
 fireEvent.keyDown(screen.getByRole('textbox',{name:'消息'}),{key:'Enter'});
 expect(onFiles).toHaveBeenCalledTimes(1);
 expect(onSend).toHaveBeenCalledTimes(1);
 expect(onDraft).toHaveBeenCalledWith('');
});

it('filters @ filenames and Enter selects the candidate without sending',async()=>{
 vi.mocked(searchFiles).mockResolvedValue({entries:[{name:'composer.tsx',path:'src/composer.tsx',kind:'file',size:1}],truncated:false});
 const onSend=vi.fn(),onReferences=vi.fn(),onDraft=vi.fn();
 render(<Composer {...base} draft="@comp" roots={['/project']} onSend={onSend} onReferences={onReferences} onDraft={onDraft}/>);
 await screen.findByRole('option');
 expect(searchFiles).toHaveBeenCalledWith('task-a',0,'comp');
 fireEvent.keyDown(screen.getByRole('textbox',{name:'消息'}),{key:'Enter'});
 expect(onSend).not.toHaveBeenCalled();
 expect(onDraft).toHaveBeenCalledWith('@composer.tsx ');
 expect(onReferences).toHaveBeenCalledWith([{rootIndex:0,path:'src/composer.tsx',name:'composer.tsx'}]);
});
