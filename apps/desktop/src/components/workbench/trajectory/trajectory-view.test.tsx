// @vitest-environment jsdom
import {render,screen,cleanup} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {afterEach,beforeEach,it,expect,vi} from 'vitest';
import {withSystemPrompt} from './trajectory-model';
import {TrajectoryView} from './trajectory-view';
vi.mock('./use-trajectory',()=>({useTrajectory:(_id:string,run:any)=>({records:withSystemPrompt([],null,run?.runtime?.capabilities),warnings:[],totalRecords:1,loading:false})}));
vi.mock('@tanstack/react-virtual',()=>({useVirtualizer:({count}: {count:number})=>({getVirtualItems:()=>Array.from({length:count},(_,index)=>({index,key:index,start:index*32,end:(index+1)*32,size:32})),getTotalSize:()=>count*32,scrollToOffset:vi.fn(),scrollToIndex:vi.fn()})}));
vi.mock('./trajectory-timeline',()=>({TrajectoryTimeline:()=>null}));
vi.mock('../message-list',()=>({Markdown:({text}:{text:string})=><p>{text}</p>}));
beforeEach(()=>vi.stubGlobal('matchMedia',()=>({matches:false,addEventListener:vi.fn(),removeEventListener:vi.fn()})));
afterEach(()=>{cleanup();vi.unstubAllGlobals();});
it('opens actual runtime prompt from the table even before the first message',async()=>{
 render(<TrajectoryView taskId="a" run={{taskId:'a',runId:'r',seq:0,status:'ready',events:[],runtime:{status:'ready',capabilities:{state:{systemPrompt:['Actual instructions','Workspace rules']}}}}} onError={vi.fn()}/>);
 await userEvent.click(screen.getByRole('cell',{name:/检查 当前运行时系统提示词/}));
 expect(screen.getByRole('tabpanel').textContent).toContain('Actual instructions');
 expect(screen.getByRole('tabpanel').textContent).toContain('Workspace rules');
 expect(screen.getByRole('tabpanel').textContent).toContain('不代表历史请求');
});
it('shows an unavailable entry when the runtime did not supply a prompt',()=>{
 render(<TrajectoryView taskId="b" onError={vi.fn()}/>);
 expect(screen.getByRole('cell',{name:/检查 系统提示词未记录/}).textContent).toContain('会话历史没有保存');
 expect(screen.queryByRole('button',{name:'系统提示词'})).toBeNull();
});
