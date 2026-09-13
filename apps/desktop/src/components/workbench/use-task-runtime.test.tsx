// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {act,cleanup,renderHook} from '@testing-library/react';
import {taskRuntime} from '@/host';
import {useTaskRuntime,runtimeLabel} from './use-task-runtime';
vi.mock('@/host',()=>({taskRuntime:{list:vi.fn().mockResolvedValue([]),focus:vi.fn().mockResolvedValue(undefined)},hostError:(e:unknown)=>e}));
afterEach(()=>{cleanup();vi.useRealTimers();vi.clearAllMocks();});
it('debounces fast task switches and releases focused task on unmount',async()=>{
 vi.useFakeTimers();
 const {rerender,unmount}=renderHook(({id})=>useTaskRuntime(id),{initialProps:{id:'a'}});
 await act(()=>vi.advanceTimersByTimeAsync(200));
 rerender({id:'b'});
 await act(()=>vi.advanceTimersByTimeAsync(300));
 expect(taskRuntime.focus).toHaveBeenCalledTimes(1);
 expect(taskRuntime.focus).toHaveBeenLastCalledWith('b');
 unmount();
 expect(taskRuntime.focus).toHaveBeenLastCalledWith(null);
});
it('displays no process separately from ready and approval states',()=>{
 expect(runtimeLabel()).toBe('未启动');
 expect(runtimeLabel({status:'ready'} as any)).toBe('已就绪');
 expect(runtimeLabel({status:'running'} as any,true)).toBe('等待审批');
 expect(runtimeLabel({status:'stopped',autoStartSuppressed:true} as any)).toBe('已手动停止');
});
it('reports unknown instead of stopped when the host poll fails',async()=>{
 vi.useFakeTimers();
 vi.mocked(taskRuntime.list).mockResolvedValue([{taskId:'a',status:'ready',pid:123} as any]);
 const {result}=renderHook(()=>useTaskRuntime(null));
 await act(()=>vi.advanceTimersByTimeAsync(1));
 vi.mocked(taskRuntime.list).mockRejectedValueOnce({code:'offline',message:'Host unavailable'});
 await act(()=>vi.advanceTimersByTimeAsync(3000));
 expect(result.current.tasks[0].status).toBe('unknown');
 expect(result.current.tasks[0].pid).toBe(123);
 expect(result.current.error?.code).toBe('offline');
});
