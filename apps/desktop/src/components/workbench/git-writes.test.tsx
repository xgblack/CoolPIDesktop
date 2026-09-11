// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {GitWrites} from './git-writes';
import {gitWrites} from '@/host';
import {TooltipProvider} from '@/components/ui/tooltip';
import type {TaskRecord,GitStatus} from '../../../../../packages/host-contract/src';
vi.mock('@/host',()=>({host:{roots:vi.fn().mockResolvedValue([{mode:'isolated'}])},hostError:(e:unknown)=>e,gitWrites:{preview:vi.fn(),commit:vi.fn(),change:vi.fn()}}));
afterEach(()=>{cleanup();vi.clearAllMocks()});
const task={id:'a',roots:['/root']} as TaskRecord;
const status={available:true,rootIndex:0,branch:'task-a',changes:[{path:'a.txt',indexStatus:'M',worktreeStatus:'M',kind:'modified'}]} as GitStatus;
const preview={root:'/worktree/one',branch:'task-a',head:'old',tree:'tree',paths:['a.txt']};
function mount(){const changed=vi.fn();render(<TooltipProvider><GitWrites task={task} root={0} status={status} onChanged={changed}/></TooltipProvider>);return changed}
it('requires preview confirmation and preserves message on commit failure',async()=>{
 vi.mocked(gitWrites.preview).mockResolvedValue(preview);
 vi.mocked(gitWrites.commit).mockRejectedValue({code:'git_stale_preview',message:'暂存内容已变化'});
 mount();fireEvent.click(await screen.findByRole('button',{name:'提交暂存改动'}));
 await screen.findByRole('dialog');expect(gitWrites.commit).not.toHaveBeenCalled();
 fireEvent.change(screen.getByLabelText('提交信息'),{target:{value:'commit message'}});
 fireEvent.click(screen.getByRole('button',{name:'确认提交'}));
 await waitFor(()=>expect(gitWrites.commit).toHaveBeenCalledWith('a',0,'commit message',preview));
 expect(await screen.findAllByText('暂存内容已变化')).not.toHaveLength(0);
 expect((screen.getByLabelText('提交信息') as HTMLTextAreaElement).value).toBe('commit message');
});
it('requires explicit discard confirmation and refreshes only after success',async()=>{
 vi.mocked(gitWrites.change).mockResolvedValue();const changed=mount();
 fireEvent.click(await screen.findByRole('button',{name:'撤销未暂存修改 a.txt'}));
 expect(gitWrites.change).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'移入回收站并恢复'}));
 await waitFor(()=>expect(changed).toHaveBeenCalledOnce());
 expect(gitWrites.change).toHaveBeenCalledWith('a',0,'a.txt','discard',true);
});
