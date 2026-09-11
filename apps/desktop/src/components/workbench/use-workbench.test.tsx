// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {act,cleanup,renderHook,waitFor} from '@testing-library/react';
import {useWorkbench} from './use-workbench';
import {host,projects,records,sendPrompt} from '../../host';
import type {TaskRecord,TaskSnapshot} from '../../../../../packages/host-contract/src';
vi.mock('../../host',()=>({sendPrompt:vi.fn(),host:{list:vi.fn(),observer:vi.fn(()=>new Promise(()=>{})),snapshot:vi.fn()},projects:{list:vi.fn()},records:{list:vi.fn(),history:vi.fn()},modelConfig:{load:vi.fn().mockResolvedValue({providers:[]})},hostError:(e:unknown)=>e}));
const task=(id:string):TaskRecord=>({id,projectId:'p',title:id,roots:['/tmp'],pinned:false,archived:false,sessionId:id,sessionFile:id,model:null});
const a=task('a'),b=task('b');
const run=(id:string):TaskSnapshot=>({taskId:id,runId:id+'-run',seq:1,status:'ready',events:[]});
function deferred<T>(){let resolve!:(v:T)=>void;const promise=new Promise<T>(r=>{resolve=r;});return{promise,resolve};}
beforeEach(()=>{vi.clearAllMocks();vi.mocked(projects.list).mockResolvedValue([]);vi.mocked(records.list).mockResolvedValue([a,b]);vi.mocked(host.list).mockResolvedValue([run('a'),run('b')]);vi.mocked(records.history).mockResolvedValue({messages:[],totalMessages:0});});
afterEach(cleanup);
it('isolates drafts and a delayed send; duplicate send cannot start another request',async()=>{
 const pending=deferred<TaskSnapshot>();vi.mocked(sendPrompt).mockReturnValue(pending.promise);
 const {result}=renderHook(useWorkbench);await waitFor(()=>expect(result.current.loading).toBe(false));
 act(()=>{result.current.chooseTask(a);result.current.setDraft('a','message A');});
 let send!:Promise<void>;act(()=>{send=result.current.send('a');void result.current.send('a');});
 expect(result.current.messages.some(m=>m.content==='message A')).toBe(true);
 expect(result.current.drafts.a).toBe('');
 act(()=>{result.current.chooseTask(b);result.current.setDraft('b','message B');});
 await act(async()=>{pending.resolve(run('a'));await send;});
 expect(sendPrompt).toHaveBeenCalledTimes(1);expect(sendPrompt).toHaveBeenCalledWith('a','message A',[],[]);
 expect(result.current.taskId).toBe('b');expect(result.current.drafts).toEqual({a:'',b:'message B'});
});

it('keeps a failed message for retry without overwriting the new draft',async()=>{
 vi.mocked(sendPrompt).mockRejectedValueOnce({code:'offline',message:'offline'}).mockResolvedValue(run('a'));
 const {result}=renderHook(useWorkbench);await waitFor(()=>expect(result.current.loading).toBe(false));
 act(()=>{result.current.chooseTask(a);result.current.setDraft('a','first');});
 await act(async()=>{await result.current.send('a');});
 expect(result.current.sendFailed).toBe(true);expect(result.current.messages.at(-1)?.content).toBe('first');
 act(()=>result.current.setDraft('a','new draft'));
 await act(async()=>{await result.current.send('a',true);});
 expect(sendPrompt).toHaveBeenLastCalledWith('a','first',[],[]);
 expect(result.current.drafts.a).toBe('new draft');
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

it('loads persisted history when the task has no active run',async()=>{
 vi.mocked(host.list).mockResolvedValue([]);
 vi.mocked(records.history).mockResolvedValue({messages:[{role:'user',content:'历史用户消息'},{role:'assistant',content:'历史助手消息'}],totalMessages:2});
 const {result}=renderHook(useWorkbench);await waitFor(()=>expect(result.current.loading).toBe(false));
 act(()=>result.current.chooseTask(a));
 await waitFor(()=>expect(result.current.messages).toHaveLength(2));
 expect(records.history).toHaveBeenCalledWith('a',null);
 expect(result.current.messages[0].content).toBe('历史用户消息');
});
it('does not publish an old operation error on another task',async()=>{
 const {result}=renderHook(useWorkbench);await waitFor(()=>expect(result.current.loading).toBe(false));
 act(()=>result.current.chooseTask(a));let reject!:(e:unknown)=>void;const pending=new Promise((_,r)=>{reject=r;});let operation!:Promise<boolean>;
 act(()=>{operation=result.current.act(()=>pending);});act(()=>result.current.chooseTask(b));
 await act(async()=>{reject({code:'old_error',message:'A failed'});await operation;});
 expect(result.current.error).toBeNull();expect(result.current.taskId).toBe('b');
});
