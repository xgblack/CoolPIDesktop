// @vitest-environment jsdom
import {render,screen,cleanup} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {afterEach,describe,it,expect,vi} from 'vitest';
import {TrajectoryDetails} from './trajectory-details';
import type {TrajectoryRecord} from './trajectory-model';
vi.mock('../message-list',()=>({Markdown:({text}:{text:string})=><p>{text}</p>}));
const record:TrajectoryRecord={id:'tool:1',aliases:[],kind:'tool',turn:1,step:1,name:'read',status:'succeeded',content:'read complete',input:{path:'a.ts'},output:'result',startedAt:1000,completedAt:1250,durationMs:250,timingSource:'omp',images:[],truncated:false};
afterEach(cleanup);
describe('trajectory inspector',()=>{
 it('uses the actual matching OMP wire schema and separates timing and payload',async()=>{
  render(<TrajectoryDetails taskId="a" record={record} capabilities={{state:{dumpTools:[{name:'other',parameters:{wrong:true}},{name:'read',description:'Read a file',parameters:{type:'object',required:['path']}}]}}} cache={new Map()} onError={vi.fn()} onClose={vi.fn()}/>);
  await userEvent.click(screen.getByRole('tab',{name:'Schema'}));
  expect(screen.getByRole('tabpanel').textContent).toContain('"required"');
  expect(screen.getByRole('tabpanel').textContent).not.toContain('wrong');
  await userEvent.click(screen.getByRole('tab',{name:'计时'}));
  expect(screen.getByRole('tabpanel').textContent).toContain('250 ms');
  expect(screen.getByRole('tabpanel').textContent).not.toContain('a.ts');
  await userEvent.click(screen.getByRole('tab',{name:'参数'}));
  expect(screen.getByRole('tabpanel').textContent).toContain('a.ts');
  await userEvent.click(screen.getByRole('tab',{name:'结果'}));
  expect(screen.getByRole('tabpanel').textContent).toContain('result');
 });
 it('does not invent a schema from the observed arguments',async()=>{
  render(<TrajectoryDetails taskId="a" record={record} cache={new Map()} onError={vi.fn()} onClose={vi.fn()}/>);
  await userEvent.click(screen.getByRole('tab',{name:'Schema'}));
  expect(screen.getByRole('tabpanel').textContent).toContain('没有可用的工具 Schema');
 });
});
