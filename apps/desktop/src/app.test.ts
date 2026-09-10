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
