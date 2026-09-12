import type {HostEvent,TaskSnapshot} from '../../../packages/host-contract/src';

export function mergeSnapshot(old:TaskSnapshot[], incoming:TaskSnapshot):TaskSnapshot[] {
 const current=old.find(s=>s.taskId===incoming.taskId);
 if(current?.runId===incoming.runId&&current.seq>incoming.seq)return old;
 return [...old.filter(s=>s.taskId!==incoming.taskId),incoming];
}

/** A gap requires an authoritative snapshot; bounded diagnostic events are not a replay log. */
export function applyEvent(snapshot:TaskSnapshot,event:HostEvent):TaskSnapshot {
 if(snapshot.runId!==event.runId||event.seq<=snapshot.seq)return snapshot;
 if(event.seq!==snapshot.seq+1)return snapshot;
 const updates=new Map(event.trajectory?.map(r=>[r.id,r])??[]);
 const trajectory=[...(snapshot.trajectory??[]).map(r=>{const next=updates.get(r.id);updates.delete(r.id);return next??r;}),...updates.values()].slice(-512);
 const p=event.payload as Record<string,any>;
 const next={...snapshot,trajectory,seq:event.seq,events:[...snapshot.events,{...event,trajectory:undefined}].slice(-256)};
 if(event.eventType==='status')next.status=p.status;
 if(event.eventType==='user_message')next.text='';
 if(event.eventType==='message_update'&&p.assistantMessageEvent?.type==='text_delta')next.text=(next.text??'')+p.assistantMessageEvent.delta;
 if(event.eventType==='error'){next.error=p as any;next.status='failed';}
 return next;
}
