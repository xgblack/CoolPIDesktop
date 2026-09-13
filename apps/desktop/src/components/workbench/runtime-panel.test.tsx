// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {RuntimePanel} from './runtime-panel';
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
 expect((screen.getByRole('checkbox') as HTMLInputElement).disabled).toBe(true);
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
 fireEvent.click(screen.getByRole('checkbox'));
 await waitFor(()=>expect(taskRuntime.keepAlive).toHaveBeenCalledWith('a',true));
});
