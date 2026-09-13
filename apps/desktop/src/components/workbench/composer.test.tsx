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

it('uses only the selected model levels and leaves selection controlled on save failure',()=>{
 const onThinking=vi.fn();
 const models:NonNullable<import('./composer').ComposerProps['models']>=[
  {provider:'p',id:'wide',capability:{reasoning:true,thinking:{support:'supported',levels:['low','high']}}},
  {provider:'p',id:'narrow',capability:{reasoning:true,thinking:{support:'supported',levels:['low']}}},
 ];
 const {rerender}=render(<Composer {...base} models={models} modelValue="p/wide" thinking="high" onThinking={onThinking}/>);
 const select=screen.getByRole('combobox',{name:'推理强度'}) as HTMLSelectElement;
 expect(Array.from(select.options,o=>o.value)).toEqual(['','low','high']);
 fireEvent.change(select,{target:{value:'low'}});
 expect(onThinking).toHaveBeenCalledWith('low');
 rerender(<Composer {...base} models={models} modelValue="p/wide" thinking="high" onThinking={onThinking}/>);
 expect(select.value).toBe('high');
 rerender(<Composer {...base} models={models} modelValue="p/narrow" onThinking={onThinking}/>);
 expect(Array.from(select.options,o=>o.value)).toEqual(['','low']);
 rerender(<Composer {...base} models={models} modelValue="p/narrow" onThinking={onThinking} thinkingDisabled/>);
 expect(select.disabled).toBe(true);
});

it.each(['unsupported','unknown'] as const)('disables %s capability without fabricating levels',support=>{
 render(<Composer {...base} models={[{provider:'p',id:'m',capability:{reasoning:true,thinking:{support,levels:[]}}}]} modelValue="p/m" onThinking={vi.fn()}/>);
 const select=screen.getByRole('combobox',{name:'推理强度'}) as HTMLSelectElement;
 expect(select.disabled).toBe(true);
 expect(select.options.length).toBe(1);
 expect(select.textContent).toContain(support==='unknown'?'能力未知':'不可调节');
});

it('hides stale levels while discovery fails or a model is unavailable',()=>{
 const {rerender}=render(<Composer {...base} models={[]} modelValue="p/m" onThinking={vi.fn()} modelsLoading/>);
 expect((screen.getByRole('combobox',{name:'推理强度'}) as HTMLSelectElement).disabled).toBe(true);
 rerender(<Composer {...base} models={[]} modelValue="p/m" onThinking={vi.fn()} modelsError="加载失败"/>);
 expect((screen.getByRole('combobox',{name:'推理强度'}) as HTMLSelectElement).options.length).toBe(1);
});

it('preserves medium when capability metadata disappears and recovers after the runtime snapshot',()=>{
 const onThinking=vi.fn();
 const models:NonNullable<import('./composer').ComposerProps['models']>=[{provider:'p',id:'m',capability:{reasoning:true,thinking:{support:'supported',levels:['minimal','low','medium','high']}}}];
 const {rerender}=render(<Composer {...base} models={models} modelValue="p/m" thinking="medium" onThinking={onThinking}/>);
 const select=screen.getByRole('combobox',{name:'推理强度'}) as HTMLSelectElement;
 expect(select.selectedOptions[0].textContent).toBe('medium');
 rerender(<Composer {...base} models={[{provider:'p',id:'m'}]} modelValue="p/m" thinking="medium" onThinking={onThinking} thinkingDisabled/>);
 expect(select.value).toBe('medium');
 expect(select.selectedOptions[0].textContent).toBe('medium · 推理能力未知');
 expect(select.disabled).toBe(true);
 rerender(<Composer {...base} models={models} modelValue="p/m" thinking="medium" onThinking={onThinking} thinkingDisabled/>);
 expect(select.selectedOptions[0].textContent).toBe('medium');
 expect(onThinking).not.toHaveBeenCalled();
});
