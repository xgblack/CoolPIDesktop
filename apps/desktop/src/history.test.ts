import {describe,it,expect,vi} from 'vitest';
import {HistoryFeed} from './history';
import type {HistoryPage} from '../../../packages/host-contract/src';
const page=(text:string,nextCursor?:string):HistoryPage=>({messages:[{role:'user',content:text}],totalMessages:2,nextCursor});
function deferred(){let resolve!:(v:HistoryPage)=>void;let reject!:(e:unknown)=>void;const promise=new Promise<HistoryPage>((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};}
describe('history paging race boundaries',()=>{
 it('discards a late page after task/run switch',async()=>{
  const feed=new HistoryFeed(),old=deferred(),publish=vi.fn();
  const pending=feed.load(()=>old.promise,false,publish);feed.reset();
  await feed.load(async()=>page('new task'),false,publish);
  old.resolve(page('old task'));await pending;
  expect(feed.messages[0].content).toBe('new task');expect(publish).toHaveBeenCalledTimes(1);
 });
 it('appends one page even when load-more is clicked twice',async()=>{
  const feed=new HistoryFeed();await feed.load(async()=>page('one','cursor-1'),false,()=>{});
  const next=deferred(),fetch=vi.fn(()=>next.promise);
  const pending=feed.load(fetch,true,()=>{});await feed.load(fetch,true,()=>{});
  next.resolve(page('two'));await pending;
  expect(fetch).toHaveBeenCalledOnce();expect(fetch).toHaveBeenCalledWith('cursor-1');
  expect(feed.messages.map(m=>m.content)).toEqual(['one','two']);
 });
 it.each(['stale_cursor','session_busy'])('drops all partial history on %s and restarts without cursor',async code=>{
  const feed=new HistoryFeed();await feed.load(async()=>page('old','old-cursor'),false,()=>{});
  await expect(feed.load(async()=>{throw{code};},true,()=>{})).rejects.toEqual({code});
  expect(feed.messages).toEqual([]);expect(feed.cursor).toBeNull();
  const fetch=vi.fn(async()=>page('fresh'));await feed.load(fetch,false,()=>{});
  expect(fetch).toHaveBeenCalledWith(null);expect(feed.messages[0].content).toBe('fresh');
 });
 it('generation invalidation rejects old success and old error without clearing new history',async()=>{
  const feed=new HistoryFeed(),old=deferred();const pending=feed.load(()=>old.promise,false,()=>{});
  feed.invalidate();await feed.load(async()=>page('finished turn'),false,()=>{});
  old.reject({code:'stale_cursor'});await pending;expect(feed.messages[0].content).toBe('finished turn');
 });
});
