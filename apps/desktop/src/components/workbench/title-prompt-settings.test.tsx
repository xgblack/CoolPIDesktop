// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {TitlePromptSettings} from './title-prompt-settings';
import {titlePrompt} from '../../host';

vi.mock('../../host',()=>({titlePrompt:{load:vi.fn(),save:vi.fn()},hostError:(e:unknown)=>e}));
const official='Write a ~5 word title for the next user message.\n<examples>official</examples>';

beforeEach(()=>{
 vi.clearAllMocks();
 vi.mocked(titlePrompt.load).mockResolvedValue({prompt:official,isDefault:true});
 vi.mocked(titlePrompt.save).mockImplementation(async prompt=>prompt===null?{prompt:official,isDefault:true}:{prompt,isDefault:false});
});
afterEach(cleanup);

it('loads the OMP official default prompt',async()=>{
 render(<TitlePromptSettings/>);
 expect(await screen.findByText('OMP 官方默认')).toBeTruthy();
 expect((screen.getByLabelText('系统提示词') as HTMLTextAreaElement).value).toBe(official);
 expect((screen.getByRole('button',{name:'恢复默认'}) as HTMLButtonElement).disabled).toBe(true);
});

it('saves a custom prompt and marks it as custom',async()=>{
 render(<TitlePromptSettings/>);
 const input=await screen.findByLabelText('系统提示词');
 fireEvent.change(input,{target:{value:'Generate a short Chinese title.'}});
 fireEvent.click(screen.getByRole('button',{name:'保存提示词'}));
 await waitFor(()=>expect(titlePrompt.save).toHaveBeenCalledWith('Generate a short Chinese title.'));
 expect(await screen.findByText('自定义提示词')).toBeTruthy();
 expect(screen.getByText('标题提示词已保存。')).toBeTruthy();
});

it('restores the official prompt through the Host',async()=>{
 vi.mocked(titlePrompt.load).mockResolvedValue({prompt:'Custom prompt',isDefault:false});
 render(<TitlePromptSettings/>);
 await screen.findByText('自定义提示词');
 fireEvent.click(screen.getByRole('button',{name:'恢复默认'}));
 await waitFor(()=>expect(titlePrompt.save).toHaveBeenCalledWith(null));
 expect((screen.getByLabelText('系统提示词') as HTMLTextAreaElement).value).toBe(official);
 expect(screen.getByText('已恢复 OMP 官方默认提示词。')).toBeTruthy();
});

it('retains edited input after a save failure',async()=>{
 vi.mocked(titlePrompt.save).mockRejectedValue({code:'database_error',message:'保存失败'});
 render(<TitlePromptSettings/>);
 const input=await screen.findByLabelText('系统提示词');
 fireEvent.change(input,{target:{value:'Keep this input'}});
 fireEvent.click(screen.getByRole('button',{name:'保存提示词'}));
 expect(await screen.findByText('保存失败')).toBeTruthy();
 expect((input as HTMLTextAreaElement).value).toBe('Keep this input');
});
