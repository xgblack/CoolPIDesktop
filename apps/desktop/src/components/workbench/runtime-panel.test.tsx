// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {RuntimePanel} from './runtime-panel';
import {RuntimeManagement} from './runtime-management';
import {taskRuntime} from '@/host';
import type {RuntimeTaskInfo,TaskRecord} from '../../../../../packages/host-contract/src';
vi.mock('@/host',()=>({taskRuntime:{action:vi.fn().mockResolvedValue({}),command:vi.fn(),keepAlive:vi.fn().mockResolvedValue(undefined),releaseIdle:vi.fn().mockResolvedValue([])},hostError:(e:unknown)=>e}));
afterEach(()=>{cleanup();vi.clearAllMocks();});
const task={id:'a',title:'Task A',roots:['/tmp'],sessionFile:'/tmp/session',approvalMode:'write'} as TaskRecord;
const info:RuntimeTaskInfo={taskId:'a',runId:'run-a',status:'running',owner:'desktop',pid:1234,startedAt:Date.now(),idleSince:null,keepAlive:false,autoStartSuppressed:false,executable:'/opt/bin/omp',version:'18',error:null};
it('requires confirmation before interrupting work and binds actions to the displayed run',async()=>{
 render(<RuntimePanel task={task} info={info}/>);
 fireEvent.click(screen.getByRole('button',{name:'停止进程'}));
 expect(taskRuntime.action).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'确认停止'}));
 await waitFor(()=>expect(taskRuntime.action).toHaveBeenCalledWith('a','stop','run-a'));
});
it('external owner prevents desktop process actions but preserves details',()=>{
 render(<RuntimePanel task={task} info={{...info,status:'external',owner:'terminal'}}/>);
 expect((screen.getByRole('button',{name:'启动 OMP'}) as HTMLButtonElement).disabled).toBe(true);
 expect(screen.getByText('1234')).toBeTruthy();
 expect((screen.getByRole('checkbox',{name:/保持运行/}) as HTMLInputElement).disabled).toBe(true);
});
it('copying a command never stops a runtime and exposes preview when clipboard fails',async()=>{
 vi.mocked(taskRuntime.command).mockResolvedValue({command:"'/omp' --add-dir '/two words' --approval-mode 'write'",executable:'/omp',arguments:[]});
 Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:vi.fn().mockRejectedValue(new Error('denied'))}});
 render(<RuntimePanel task={task} info={{...info,status:'ready'}}/>);
 fireEvent.click(screen.getByRole('button',{name:'复制终端续接命令'}));
 await screen.findByText('剪贴板不可用，请从下方命令预览手动复制。');
 expect(screen.getByText(/--add-dir/)).toBeTruthy();
 expect(taskRuntime.action).not.toHaveBeenCalled();
});
it('keep alive changes only the selected task',async()=>{
 render(<RuntimePanel task={task} info={{...info,status:'ready'}}/>);
 fireEvent.click(screen.getByRole('checkbox',{name:/保持运行/}));
 await waitFor(()=>expect(taskRuntime.keepAlive).toHaveBeenCalledWith('a',true));
});

it('global release reports failure without affecting task controls and can be retried',async()=>{
 const refresh=vi.fn().mockResolvedValue(undefined);
 vi.mocked(taskRuntime.releaseIdle).mockRejectedValueOnce({code:'release_failed',message:'后台进程释放失败'}).mockResolvedValueOnce(['b']);
 render(<RuntimeManagement all={[info]} tasks={[task]} error={null} onRefresh={refresh}/>);
 fireEvent.click(screen.getByRole('button',{name:'释放后台空闲进程'}));
 await screen.findByText('后台进程释放失败');
 expect(refresh).not.toHaveBeenCalled();
 expect(taskRuntime.action).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'释放后台空闲进程'}));
 await screen.findByText('已释放 1 个后台空闲进程');
 expect(refresh).toHaveBeenCalledOnce();
});
it('unknown global state disables release instead of claiming there are no processes',()=>{
 render(<RuntimeManagement all={[]} tasks={[]} error={{code:'offline',message:'无法连接 Host'}} onRefresh={vi.fn()}/>);
 expect(screen.getByText('本机 OMP · 状态未知')).toBeTruthy();
 expect((screen.getByRole('button',{name:'释放后台空闲进程'}) as HTMLButtonElement).disabled).toBe(true);
 expect(screen.queryByText(/当前没有 OMP 运行进程/)).toBeNull();
});
