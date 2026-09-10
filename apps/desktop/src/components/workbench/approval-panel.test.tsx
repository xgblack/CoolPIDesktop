// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { ApprovalPanel } from './approval-panel';

afterEach(cleanup);
describe('ApprovalPanel', () => {
  it('keeps rejection, permission, and cancellation distinct without default approval', () => {
    const onRespond = vi.fn();
    render(<ApprovalPanel requests={[{ id: 'permission', method: 'confirm', message: '是否允许运行此命令？' }]} busy={false} onRespond={onRespond} />);
    expect(onRespond).not.toHaveBeenCalled();
    expect(document.activeElement).not.toBe(screen.getByRole('button', { name: '允许' }));
    fireEvent.click(screen.getByRole('button', { name: '拒绝' }));
    expect(onRespond).toHaveBeenLastCalledWith('permission', null, false, false);
    fireEvent.click(screen.getByRole('button', { name: '允许' }));
    expect(onRespond).toHaveBeenLastCalledWith('permission', null, true, false);
    fireEvent.click(screen.getByRole('button', { name: '取消请求' }));
    expect(onRespond).toHaveBeenLastCalledWith('permission', null, null, true);
  });
  it('does not reuse input when the parent switches task or run, or enable busy responses', () => {
    const onRespond = vi.fn(); const requests = [{ id: 'same-id', method: 'input' as const }];
    const { rerender } = render(<ApprovalPanel key="task-a:run-1" requests={requests} busy={false} onRespond={onRespond} />);
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'A response' } });
    rerender(<ApprovalPanel key="task-b:run-1" requests={requests} busy={true} onRespond={onRespond} />);
    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('');
    expect((screen.getByRole('button', { name: '提交回复' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: '提交回复' }));
    expect(onRespond).not.toHaveBeenCalled();
  });
});
