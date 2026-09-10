import type {HostEvent,TaskSnapshot} from '../../../packages/host-contract/src';
export type TaskMap=Record<string,TaskSnapshot>;
export function applySnapshot(tasks:TaskMap,snapshot:TaskSnapshot){const current=tasks[snapshot.taskId];if(current&&current.runId===snapshot.runId&&snapshot.seq<current.seq)return tasks;return {...tasks,[snapshot.taskId]:{...snapshot,events:snapshot.events.filter(event=>event.taskId===snapshot.taskId&&event.runId===snapshot.runId&&event.seq<=snapshot.seq).sort((a,b)=>a.seq-b.seq).slice(-512)}}}
function rec(v:unknown):Record<string,any>{return typeof v==='object'&&v&&!Array.isArray(v)?v as any:{}}
export function eventText(event:HostEvent){const payload=rec(event.payload),assistant=rec(payload.assistantMessageEvent);return assistant.type==='text_delta'?assistant.delta:(event.eventType==='text_delta'||payload.type==='text_delta')?payload.delta:null}
