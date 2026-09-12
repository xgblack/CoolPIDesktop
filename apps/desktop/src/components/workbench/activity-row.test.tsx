// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,render,screen} from '@testing-library/react';
import {ActivityRow,toolIcon} from './activity-row';
import {MessageList} from './message-list';
import {BookOpen,Search,Terminal,Wrench} from 'lucide-react';
afterEach(cleanup);
it('maps tool categories without treating unknown tools as commands',()=>{
 expect(toolIcon('read')).toBe(BookOpen);expect(toolIcon('grep')).toBe(Search);expect(toolIcon('bash')).toBe(Terminal);expect(toolIcon('custom')).toBe(Wrench);
});
it('running animation follows state and failures remain explicit',()=>{
 const {container,rerender}=render(<ActivityRow name="bash" state="running">output</ActivityRow>);
 expect(container.querySelector('[data-state=running]')).toBeTruthy();
 rerender(<ActivityRow name="bash" state="failed">output</ActivityRow>);
 expect(container.querySelector('[data-state=running]')).toBeNull();expect(screen.getByText('失败')).toBeTruthy();
});
it('keeps final answer visible with thinking and tool steps in a collapsed process',()=>{
 const {container}=render(<MessageList taskKey="a" messages={[{role:'user',content:'hello'},{role:'assistant',content:[{type:'thinking',thinking:'inspect files'},{type:'toolCall',name:'read',arguments:{path:'a.ts'}}]},{role:'toolResult',content:'file content'},{role:'assistant',content:'final answer'}]} streamingText="" running={false} hasMore={false} loading={false} onMore={vi.fn()} onRefresh={vi.fn()} onError={vi.fn()}/>);
 expect(screen.getByText('final answer').closest('details')).toBeNull();
 expect(screen.getAllByText('inspect files')[0].closest('.activity-group')).toBeTruthy();
 expect(container.querySelector('.activity-group')?.hasAttribute('open')).toBe(false);
});
