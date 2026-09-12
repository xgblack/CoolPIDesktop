import {describe,it,expect} from 'vitest';
import {ledgerRows,timelineItems,overlapping,searchRecords,type TrajectoryRecord} from './trajectory-model';
import {mergeRecords,projectLive} from './use-trajectory';
const record=(id:string,extra:Partial<TrajectoryRecord>={}):TrajectoryRecord=>({id,aliases:[],kind:'assistant',turn:1,step:1,name:id,status:'succeeded',content:'text',images:[],truncated:false,...extra});
describe('DSH trajectory semantics',()=>{
 it('keeps discontiguous turn and step headers unique for virtual layout',()=>{
  const records=[record('a'),record('context',{kind:'context',turn:null,step:null}),record('b'),record('c',{step:null}),record('d')];
  const rows=ledgerRows(records,new Set(),false,null,null);
  expect(new Set(rows.map(r=>r.key)).size).toBe(rows.length);
  expect(rows.filter(r=>r.type==='step')).toHaveLength(3);
 });
 it('compresses only idle gaps and preserves concurrency; running entries remain markers',()=>{
  const spans=timelineItems([record('a',{startedAt:100,durationMs:20}),record('b',{startedAt:105,durationMs:10,kind:'tool'}),record('c',{startedAt:200,durationMs:30}),record('d',{startedAt:220,status:'running',durationMs:500}),record('unknown')],true);
  expect(spans.map(s=>[s.start,s.end])).toEqual([[0,20],[5,15],[20,50],[40,40]]);
  expect([...overlapping(spans,15,20)]).toEqual(['a','b','c']);
 });
 it('folds tools under assistants and expands matches through folds',()=>{
  const records=[record('a'),record('tool',{kind:'tool',parentId:'a',input:{path:'needle'}}),record('child',{kind:'tool',parentId:'tool'})];
  expect(ledgerRows(records,new Set(),true,null,null).filter(r=>r.type==='record').map(r=>r.key)).toEqual(['a']);
  expect(ledgerRows(records,new Set(),false,null,null,new Set(['a'])).filter(r=>r.type==='record').map(r=>r.key)).toEqual(['a']);
  const matches=searchRecords(records,'needle');
  expect(ledgerRows(records,new Set([1]),true,matches,null).filter(r=>r.type==='record').map(r=>r.key)).toEqual(['tool']);
 });
 it('prepends pages without changing existing stable IDs, and replaces an updated record once',()=>{
  expect(mergeRecords([record('b'),record('c')],[record('a'),record('b',{content:'updated'})],true).map(r=>[r.id,r.content])).toEqual([['a','text'],['b','updated'],['c','text']]);
 });
 it('reconciles persisted identity with live timestamp alias and maps tool parents',()=>{
  const history=[record('entry',{turn:8,aliases:['message:assistant:100'],durationMs:null})];
  const live=[record('live',{aliases:['message:assistant:100'],durationMs:123}),record('tool',{kind:'tool',parentId:'live'})];
  const merged=projectLive(history,live);
  expect(merged.map(r=>r.id)).toEqual(['entry','tool']);expect(merged[0].durationMs).toBe(123);expect(merged[1].parentId).toBe('entry');expect(merged[1].turn).toBe(8);
 });
});
