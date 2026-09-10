import { describe, expect, it } from 'vitest';
import { applySnapshot, eventText } from './task-state';

describe('task snapshot state', () => {
  it('keeps only matching run events in sequence order', () => {
    const snapshot=applySnapshot({}, {taskId:'alpha',runId:'run-1',seq:3,status:'idle',events:[
      {taskId:'other',runId:'run-1',seq:1,eventType:'x',payload:{}},
      {taskId:'alpha',runId:'old',seq:2,eventType:'x',payload:{}},
      {taskId:'alpha',runId:'run-1',seq:3,eventType:'x',payload:{}},
      {taskId:'alpha',runId:'run-1',seq:1,eventType:'x',payload:{}},
    ]});
    expect(snapshot.alpha.events.map(event=>event.seq)).toEqual([1,3]);
  });
  it('extracts streamed assistant deltas without exposing event internals', () => {
    expect(eventText({taskId:'a',runId:'r',seq:1,eventType:'message_update',payload:{assistantMessageEvent:{type:'text_delta',delta:'OK'}}})).toBe('OK');
  });
});

import type { TaskSnapshot } from '../../../packages/host-contract/src';
const current:TaskSnapshot={taskId:'alpha',runId:'run-1',seq:10,status:'idle',events:[],text:'complete retained text',truncated:true};
it('ignores older snapshots within a run but resets on restart',()=>{
  const tasks={alpha:current};
  expect(applySnapshot(tasks,{...current,seq:9})).toBe(tasks);
  const fresh=applySnapshot(tasks,{...current,runId:'run-2',seq:0,text:'',events:[]});
  expect(fresh.alpha.runId).toBe('run-2');
  expect(fresh.alpha.text).toBe('');
});
it('accepts stopped snapshots without losing retained text',()=>{
 const stopped=applySnapshot({alpha:current},{...current,seq:11,status:'stopped'});
 expect(stopped.alpha.status).toBe('stopped');
 expect(stopped.alpha.text).toBe('complete retained text');
 expect(stopped.alpha.truncated).toBe(true);
});
