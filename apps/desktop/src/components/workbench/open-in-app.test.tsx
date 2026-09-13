// @vitest-environment jsdom
import {afterEach, beforeEach, expect, it, vi} from 'vitest';
import {cleanup, fireEvent, render, screen, waitFor} from '@testing-library/react';
import {TooltipProvider} from '@/components/ui/tooltip';
const api = vi.hoisted(() => ({list:vi.fn(), icon:vi.fn(), open:vi.fn()}));
vi.mock('@/host', () => ({openInApp:api, hostError:(e:unknown) => e}));
beforeEach(() => {
 vi.resetModules(); localStorage.clear();
 api.list.mockResolvedValue([{id:'finder', name:'访达'}, {id:'warp', name:'Warp'}]);
 api.icon.mockRejectedValue({message:'应用没有图标'}); api.open.mockResolvedValue(undefined);
});
afterEach(() => {cleanup(); vi.clearAllMocks();});
async function mount(taskId = 'a') {
 const {OpenInApp} = await import('./open-in-app');
 return render(<TooltipProvider><OpenInApp taskId={taskId}/></TooltipProvider>);
}
it('restores the installed choice and opens the current task without passing a path', async () => {
 localStorage.setItem('cool-pi-desktop.open-in-app.choice', 'warp');
 await mount('worktree-task');
 fireEvent.click(await screen.findByRole('button', {name:'在Warp中打开工作目录'}));
 expect(api.open).toHaveBeenCalledWith('worktree-task', 'warp');
});
it('falls back from an uninstalled choice and guards repeat clicks while launch is pending', async () => {
 localStorage.setItem('cool-pi-desktop.open-in-app.choice', 'removed');
 let finish!:()=>void;
 api.open.mockImplementationOnce(() => new Promise<void>(resolve => {finish = resolve;}));
 await mount();
 const button = await screen.findByRole('button', {name:'在访达中打开工作目录'});
 fireEvent.click(button); fireEvent.click(button);
 expect(api.open).toHaveBeenCalledTimes(1);
 finish();
 await waitFor(() => expect(localStorage.getItem('cool-pi-desktop.open-in-app.choice')).toBe('finder'));
});
it('reports launch failures rather than implying success', async () => {
 api.open.mockRejectedValueOnce({message:'应用未安装或已移除，请刷新打开方式'});
 await mount();
 fireEvent.click(await screen.findByRole('button', {name:'在访达中打开工作目录'}));
 await waitFor(() => expect(screen.getByRole('status').textContent).toContain('应用未安装或已移除'));
});
it('hides the button when the host has no supported installed applications', async () => {
 api.list.mockResolvedValueOnce([]);
 await mount();
 await waitFor(() => expect(screen.queryByRole('button', {name:'选择打开方式'})).toBeNull());
});
