import {expect,it} from 'vitest';
import {applyEvent,mergeSnapshot} from './live-events';
import type {HostEvent,TaskSnapshot} from '../../../packages/host-contract/src';
const snapshot:TaskSnapshot={taskId:'a',runId:'run1',seq:1,status:'running',events:[],text:'Hello'};
const event:HostEvent={taskId:'a',runId:'run1',seq:2,eventType:'message_update',payload:{assistantMessageEvent:{type:'text_delta',delta:' world'}}};
it('applies contiguous deltas once and rejects gaps and old runs',()=>{
 const next=applyEvent(snapshot,event);expect(next.text).toBe('Hello world');
 expect(applyEvent(next,event)).toBe(next);
 expect(applyEvent(snapshot,{...event,seq:3})).toBe(snapshot);
 expect(applyEvent(snapshot,{...event,runId:'old'})).toBe(snapshot);
 expect(mergeSnapshot([next],snapshot)).toEqual([next]);
});
it('recovery uses snapshot text, not retained diagnostic deltas',()=>{
 const recovered={...snapshot,seq:90,text:'Complete answer',events:[event]};
 expect(mergeSnapshot([snapshot],recovered)[0].text).toBe('Complete answer');
 expect(applyEvent(recovered,{...event,seq:91}).text).toBe('Complete answer world');
});
it('carries authoritative turn timing and clears old tools on the next prompt',()=>{
 const started=applyEvent({...snapshot,turnStartedAt:1,turnCompletedAt:2,tools:[{id:'old',name:'read',status:'succeeded',args:{},seq:1}]},{...event,eventType:'user_message',payload:{text:'next',timestamp:1000}});
 expect(started.turnStartedAt).toBe(1000);expect(started.turnCompletedAt).toBeNull();expect(started.tools).toEqual([]);
 const ended=applyEvent(started,{...event,seq:3,eventType:'status',payload:{status:'interrupted',completedAt:2500}});
 expect(ended.turnStartedAt).toBe(1000);expect(ended.turnCompletedAt).toBe(2500);
});
