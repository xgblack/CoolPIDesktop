// @vitest-environment jsdom
import {afterEach, beforeEach, expect, it, vi} from 'vitest';
import {act, cleanup, fireEvent, render, screen} from '@testing-library/react';
import {WorkbenchInspector} from './workbench-inspector';
import {host,records} from '@/host';
import type {TaskRecord, TaskSnapshot} from '../../../../../packages/host-contract/src';
vi.mock('./files-panel',()=>({FilesPanel:()=>null}));
vi.mock('./terminal-panel',()=>({TerminalPanel:()=>null}));
vi.mock('@/host', () => ({host: {roots: vi.fn().mockResolvedValue([])}, records: {gitStatus: vi.fn(), gitDiff: vi.fn()}, hostError: (e: unknown) => e}));
afterEach(() => {cleanup(); vi.resetAllMocks();});
beforeEach(() => { vi.mocked(host.roots).mockResolvedValue([]); });
const task = (id: string) => ({id, roots: ['/tmp/project']} as TaskRecord);
const props = {busy: false, onRefreshUsage: vi.fn()};
it('keeps null usage unknown and reports Git failures without claiming a non-repository', async () => {
  vi.mocked(records.gitStatus).mockRejectedValue({code: 'git_failed', message: 'permission denied'});
  render(<WorkbenchInspector {...props} task={task('a')} run={{usage: {cost: null, contextTokens: null, contextWindow: 100}} as unknown as TaskSnapshot}/>);
  fireEvent.mouseDown(screen.getByRole('tab',{name:'变更'}),{button:0,ctrlKey:false});
  await screen.findByText('permission denied');
  expect(screen.queryByText('所选目录不是 Git 仓库。')).toBeNull();
  expect(screen.queryByText('0%')).toBeNull();
});
it('discards old task responses and lists an untracked file once', async () => {
  let resolve!: (value: any) => void;
  vi.mocked(records.gitStatus).mockImplementationOnce(() => new Promise(r => {resolve = r;}));
  vi.mocked(records.gitStatus).mockResolvedValue({rootIndex: 0, available: true, changes: [{path: 'new.txt', kind: 'untracked', indexStatus: '?', worktreeStatus: '?'}]});
  const {rerender} = render(<WorkbenchInspector {...props} task={task('a')}/>);
  fireEvent.mouseDown(screen.getByRole('tab',{name:'变更'}),{button:0,ctrlKey:false});
  rerender(<WorkbenchInspector {...props} task={task('b')}/>);
  await screen.findByText('new.txt');
  await act(async () => resolve({rootIndex: 0, available: false, changes: []}));
  expect(screen.getAllByText('new.txt')).toHaveLength(1);
  expect(screen.queryByText('所选目录不是 Git 仓库。')).toBeNull();
  vi.mocked(records.gitDiff).mockResolvedValue({rootIndex: 0, path: 'new.txt', staged: false, binary: true, text: 'binary'});
  fireEvent.click(screen.getByRole('button', {name: /未跟踪\s*new.txt/}));
  await screen.findByText('二进制文件');
});
