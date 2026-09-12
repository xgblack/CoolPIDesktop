import type { TrajectoryRecord } from '../../../../../../packages/host-contract/src';
export type { TrajectoryRecord };
export const ROW_HEIGHT = 30;
export const kindLabels: Record<TrajectoryRecord['kind'],string> = {user:'用户',assistant:'助手',tool:'工具',context:'上下文',compaction:'压缩'};
export const statusLabels: Record<TrajectoryRecord['status'],string> = {running:'运行中',succeeded:'完成',failed:'失败',cancelled:'已取消',interrupted:'已中断',unknown:'状态未记录'};
export function stringify(value:unknown):string { return typeof value==='string'?value:value==null?'':JSON.stringify(value,null,2); }
export function previewText(value:unknown):string {
 if(Array.isArray(value)) return value.map(v=>v&&typeof v==='object' ? String(v.text??v.thinking??v.name??'') : String(v??'')).filter(Boolean).join('\n');
 return stringify(value);
}
export function searchRecords(records:TrajectoryRecord[],query:string):Set<string> {
 const terms=query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
 return new Set(records.filter(r=>{const text=[r.name,r.model,kindLabels[r.kind],stringify(r.content),stringify(r.input),stringify(r.output),r.error].join('\n').toLocaleLowerCase();return terms.every(term=>text.includes(term));}).map(r=>r.id));
}
export function findRecord(records:TrajectoryRecord[],id:string|null) { return id?records.find(r=>r.id===id||r.aliases.includes(id)):undefined; }
export interface LedgerRow {key:string;type:'turn'|'step'|'record';record:TrajectoryRecord;label?:string;count?:number;depth:number}
export function ledgerRows(records:TrajectoryRecord[],collapsed:Set<number>,foldTools:boolean,matches:Set<string>|null,range:Set<string>|null,foldedAssistants:Set<string>=new Set()):LedgerRow[] {
 const rows:LedgerRow[]=[];let turn:number|null|undefined;let step:number|null|undefined;
 const ids=new Map(records.map(r=>[r.id,r]));
 const counts=new Map<number|null,number>(); for(const r of records)counts.set(r.turn,(counts.get(r.turn)??0)+1);
 for(const r of records){
  if(matches&&!matches.has(r.id))continue;
  if(r.turn!==turn){turn=r.turn;step=undefined;rows.push({key:`turn:${r.turn}:${r.id}`,type:'turn',record:r,label:r.turn==null?'会话上下文':`轮次 ${r.turn}`,count:counts.get(r.turn),depth:0});}
  if(!matches&&r.turn!=null&&collapsed.has(r.turn))continue;
  if(r.step!==step){step=r.step;if(r.step!=null)rows.push({key:`step:${r.turn}:${r.step}:${r.id}`,type:'step',record:r,label:`生成步骤 ${r.step}`,depth:0});}
  let depth=0;let parent=r.parentId;const visited=new Set([r.id]);let toolAncestor=false;let foldedAncestor=false;
  while(parent&&ids.has(parent)&&!visited.has(parent)){visited.add(parent);const p=ids.get(parent)!;toolAncestor ||= p.kind==='tool';foldedAncestor ||= foldedAssistants.has(p.id);depth++;parent=p.parentId;}
  if((foldTools&&(r.kind==='tool'||toolAncestor)||foldedAncestor)&&!matches)continue;
  rows.push({key:r.id,type:'record',record:r,depth:Math.min(depth,5)});
 }
 return rows;
}
export interface TimelineItem {record:TrajectoryRecord;start:number;end:number;lane:number}
/** DSH duration mode removes idle gaps but retains overlapping operation spans. */
export function timelineItems(records:TrajectoryRecord[],duration:boolean):TimelineItem[]{
 const lane=(r:TrajectoryRecord)=>r.kind==='assistant'||r.kind==='compaction'?1:r.kind==='tool'?2:0;
 if(!duration)return records.map((record,index)=>({record,start:index,end:index+1,lane:lane(record)}));
 const spans=records.filter(r=>r.startedAt!=null&&Number.isFinite(r.startedAt)).map(record=>{
  const start=record.startedAt!;const length=record.status==='running'?0:Math.max(0,record.durationMs??0);
  return {record,start,end:start+length,lane:lane(record)};
 });
 let covered:number|null=null,removed=0;const offsets=new Map<string,number>();
 for(const s of [...spans].sort((a,b)=>a.start-b.start||a.end-b.end)){
  if(covered!=null&&s.start>covered)removed+=s.start-covered;
  offsets.set(s.record.id,removed);covered=Math.max(covered??s.end,s.end);
 }
 const origin=Math.min(...spans.map(s=>s.start));
 return spans.map(s=>({...s,start:s.start-origin-(offsets.get(s.record.id)??0),end:s.end-origin-(offsets.get(s.record.id)??0)}));
}
export function overlapping(items:TimelineItem[],start:number,end:number):Set<string> {const lo=Math.min(start,end),hi=Math.max(start,end);return new Set(items.filter(i=>i.start<=hi&&i.end>=lo).map(i=>i.record.id));}
export function durationLabel(ms?:number|null):string{return ms==null||!Number.isFinite(ms)?'未记录':ms<1000?`${Math.round(ms)} ms`:`${(ms/1000).toFixed(2)} s`;}
