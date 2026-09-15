// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {AgentsSettings} from './agents-settings';
import {agentProfiles} from '@/host';

vi.mock('@/host',()=>({agentProfiles:{list:vi.fn(),save:vi.fn()},hostError:(value:unknown)=>value}));
afterEach(()=>{cleanup();vi.clearAllMocks()});

it('keeps complete Markdown input when an external edit causes a revision conflict',async()=>{
 const original='---\nname: reviewer\nunknown: keep\n---\n\nOriginal instructions\n';
 vi.mocked(agentProfiles.list).mockResolvedValue([{scope:'user',name:'reviewer',content:original,revision:'rev-1'}]);
 vi.mocked(agentProfiles.save).mockRejectedValue({code:'config_conflict',message:'Profile 已被外部修改'});
 render(<AgentsSettings projects={[]} projectId="" onDirtyChange={vi.fn()}/>);
 const editor=await screen.findByRole('textbox',{name:'Markdown'});
 const changed=original+'\nDo not lose this input.\n';
 fireEvent.change(editor,{target:{value:changed}});
 fireEvent.click(screen.getByRole('button',{name:'保存'}));
 expect(await screen.findByText('Profile 已被外部修改')).toBeTruthy();
 expect((editor as HTMLTextAreaElement).value).toBe(changed);
 await waitFor(()=>expect(agentProfiles.save).toHaveBeenCalledWith('user',null,null,'reviewer',changed,'rev-1'));
});
