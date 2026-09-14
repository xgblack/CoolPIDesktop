// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {NameDialog} from './workbench-dialogs';

afterEach(()=>{cleanup();vi.clearAllMocks();});

const base={
 initial:'原任务标题',
 onClose:vi.fn(),
 onSubmit:vi.fn().mockResolvedValue(undefined),
 restoreFocus:vi.fn(),
};

it('shows title generation only when the task rename caller provides it',()=>{
 const {rerender}=render(<NameDialog {...base} kind="rename"/>);
 expect(screen.queryByRole('button',{name:'自动生成'})).toBeNull();
 rerender(<NameDialog {...base} kind="task"/>);
 expect(screen.queryByRole('button',{name:'自动生成'})).toBeNull();
 rerender(<NameDialog {...base} kind="rename" onGenerate={vi.fn().mockResolvedValue('候选标题')}/>);
 expect(screen.getByRole('button',{name:'自动生成'})).toBeTruthy();
});

it('generates one candidate and applies it to the input without saving',async()=>{
 let finish!:(title:string)=>void;
 const onGenerate=vi.fn(()=>new Promise<string>(resolve=>{finish=resolve;}));
 const onSubmit=vi.fn().mockResolvedValue(undefined);
 render(<NameDialog {...base} kind="rename" onGenerate={onGenerate} onSubmit={onSubmit}/>);

 const generate=screen.getByRole('button',{name:'自动生成'});
 fireEvent.click(generate);
 fireEvent.click(screen.getByRole('button',{name:'生成中'}));
 expect(onGenerate).toHaveBeenCalledTimes(1);
 expect((screen.getByLabelText('名称') as HTMLInputElement).value).toBe('原任务标题');

 finish('检查 OMP 会话标题');
 expect(await screen.findByText('检查 OMP 会话标题')).toBeTruthy();
 expect(onSubmit).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'应用'}));
 expect((screen.getByLabelText('名称') as HTMLInputElement).value).toBe('检查 OMP 会话标题');
 expect(onSubmit).not.toHaveBeenCalled();

 fireEvent.click(screen.getByRole('button',{name:'保存'}));
 await waitFor(()=>expect(onSubmit).toHaveBeenCalledWith('检查 OMP 会话标题',false,'shared'));
});

it('reports generation errors and allows retry',async()=>{
 const onGenerate=vi.fn()
  .mockRejectedValueOnce({code:'title_timeout',message:'生成超时',suggestion:'请重试'})
  .mockResolvedValueOnce('重试后的标题');
 render(<NameDialog {...base} kind="rename" onGenerate={onGenerate}/>);

 fireEvent.click(screen.getByRole('button',{name:'自动生成'}));
 expect(await screen.findByText('生成超时')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'自动生成'}));
 expect(await screen.findByText('重试后的标题')).toBeTruthy();
 expect(onGenerate).toHaveBeenCalledTimes(2);
});
