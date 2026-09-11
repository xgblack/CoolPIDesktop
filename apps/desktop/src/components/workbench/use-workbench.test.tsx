// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {act,cleanup,renderHook,waitFor} from '@testing-library/react';
import {useWorkbench} from './use-workbench';
import {host,projects,records} from '../../host';
import type {TaskRecord,TaskSnapshot} from '../../../../../packages/host-contract/src';
vi.mock('../../host',()=>({host:{list:vi.fn(),prompt:vi.fn()},projects:{list:vi.fn()},records:{list:vi.fn(),history:vi.fn()},hostError:(e:unknown)=>e}));
const task=(id:string):TaskRecord=>({id,projectId:'p',title:id,roots:['/tmp'],pinned:false,archived:false,sessionId:id,sessionFile:id,model:null});
const a=task('a'),b=task('b');
const run=(id:string):TaskSnapshot=>({taskId:id,runId:id+'-run',seq:1,status:'ready',events:[]});
function deferred<T>(){let resolve!:(v:T)=>void;const promise=new Promise<T>(r=>{resolve=r;});return{promise,resolve};}
beforeEach(()=>{vi.clearAllMocks();vi.mocked(projects.list).mockResolvedValue([]);vi.mocked(records.list).mockResolvedValue([a,b]);vi.mocked(host.list).mockResolvedValue([run('a'),run('b')]);vi.mocked(records.history).mockResolvedValue({messages:[],totalMessages:0});});
afterEach(cleanup);
it('isolates drafts and a delayed send; duplicate send cannot start another request',async()=>{
 const pending=deferred<TaskSnapshot>();vi.mocked(host.prompt).mockReturnValue(pending.promise);
 const {result}=renderHook(useWorkbench);await waitFor(()=>expect(result.current.loading).toBe(false));
 act(()=>{result.current.chooseTask(a);result.current.setDraft('a','message A');});
 let send!:Promise<void>;act(()=>{send=result.current.send('a');void result.current.send('a');});
 act(()=>{result.current.chooseTask(b);result.current.setDraft('b','message B');});
 await act(async()=>{pending.resolve(run('a'));await send;});
 expect(host.prompt).toHaveBeenCalledTimes(1);expect(host.prompt).toHaveBeenCalledWith('a','message A',[]);
 expect(result.current.taskId).toBe('b');expect(result.current.drafts).toEqual({a:'',b:'message B'});
});
it('discards delayed history from the previously selected task',async()=>{
 const pending=deferred<{messages:{role:string;content:string}[];totalMessages:number}>();
 vi.mocked(records.history).mockImplementation(id=>id==='a'?pending.promise:Promise.resolve({messages:[{role:'assistant',content:'B only'}],totalMessages:1}));
 const {result}=renderHook(useWorkbench);await waitFor(()=>expect(result.current.loading).toBe(false));
 act(()=>result.current.chooseTask(a));await waitFor(()=>expect(records.history).toHaveBeenCalledWith('a',null));
 act(()=>result.current.chooseTask(b));await waitFor(()=>expect(result.current.messages[0]?.content).toBe('B only'));
 await act(async()=>pending.resolve({messages:[{role:'assistant',content:'stale A'}],totalMessages:1}));
 expect(result.current.messages[0]?.content).toBe('B only');expect(result.current.historyBusy).toBe(false);
});
it('does not publish an old operation error on another task',async()=>{
 const {result}=renderHook(useWorkbench);await waitFor(()=>expect(result.current.loading).toBe(false));
 act(()=>result.current.chooseTask(a));let reject!:(e:unknown)=>void;const pending=new Promise((_,r)=>{reject=r;});let operation!:Promise<boolean>;
 act(()=>{operation=result.current.act(()=>pending);});act(()=>result.current.chooseTask(b));
 await act(async()=>{reject({code:'old_error',message:'A failed'});await operation;});
 expect(result.current.error).toBeNull();expect(result.current.taskId).toBe('b');
});
