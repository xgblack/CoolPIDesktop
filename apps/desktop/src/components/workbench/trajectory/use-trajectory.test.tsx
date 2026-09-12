// @vitest-environment jsdom
import {act,renderHook,waitFor} from '@testing-library/react';
import {beforeEach,describe,it,expect,vi} from 'vitest';
import {trajectory} from '../../../host';
import {useTrajectory} from './use-trajectory';
import type {TrajectoryPage,TrajectoryRecord} from '../../../../../../packages/host-contract/src';
vi.mock('../../../host',()=>({trajectory:{read:vi.fn()},hostError:(e:Error)=>({code:'test',message:e.message})}));
const r=(id:string):TrajectoryRecord=>({id,aliases:[],kind:'user',turn:1,step:null,name:id,status:'succeeded',content:id,images:[],truncated:false});
const page=(ids:string[],nextCursor:string|null=null):TrajectoryPage=>({records:ids.map(r),nextCursor,afterCursor:ids.at(-1),totalRecords:100,revision:'v1',warnings:[]});
describe('trajectory window requests',()=>{
 beforeEach(()=>vi.mocked(trajectory.read).mockReset());
 it('sends the older cursor and preserves the loaded tail',async()=>{
  vi.mocked(trajectory.read).mockResolvedValueOnce(page(['b','c'],'b')).mockResolvedValueOnce(page(['a']));
  const {result,unmount}=renderHook(()=>useTrajectory('task'));
  await waitFor(()=>expect(result.current.records).toHaveLength(2));
  await act(()=>result.current.loadOlder());
  expect(trajectory.read).toHaveBeenNthCalledWith(2,'task','b',false);expect(result.current.records.map(r=>r.id)).toEqual(['a','b','c']);unmount();
 });
 it('discards a late response after switching tasks',async()=>{
  let resolve!:(p:TrajectoryPage)=>void;
  vi.mocked(trajectory.read).mockImplementation(task=>task==='a'?new Promise(r=>{resolve=r;}):Promise.resolve(page(['B'])));
  const {result,rerender,unmount}=renderHook(({id})=>useTrajectory(id),{initialProps:{id:'a'}});
  rerender({id:'b'});await waitFor(()=>expect(result.current.records[0]?.id).toBe('B'));
  await act(async()=>{resolve(page(['A']));});expect(result.current.records.map(r=>r.id)).toEqual(['B']);unmount();
 });
});
