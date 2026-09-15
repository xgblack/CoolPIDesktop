// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {act,cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {TerminalPanel,TerminalSession} from './terminal-panel';
import {terminal} from '@/host';
import {TooltipProvider} from '@/components/ui/tooltip';
const mocks=vi.hoisted(()=>({write:vi.fn((_b:unknown,cb:()=>void)=>cb()),dispose:vi.fn()}));
vi.mock('@xterm/xterm',()=>({Terminal:class {options={};cols=80;rows=24;loadAddon(){}open(){}focus(){}reset(){}writeln(){}write=mocks.write;dispose=mocks.dispose;onData(){return{dispose(){}}}}}));
vi.mock('@xterm/addon-fit',()=>({FitAddon:class{fit(){}}}));
vi.mock('@/host',()=>({terminal:{create:vi.fn(),close:vi.fn().mockResolvedValue(undefined),snapshot:vi.fn(),resize:vi.fn().mockResolvedValue(undefined),write:vi.fn()},hostError:(e:unknown)=>e}));
vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});
afterEach(()=>{cleanup();localStorage.clear();vi.clearAllMocks()});
it('closes a late creation after unmount instead of attaching to another task',async()=>{
 let resolve!:(x:any)=>void;vi.mocked(terminal.create).mockImplementationOnce(()=>new Promise(r=>{resolve=r}));
 const view=render(<TooltipProvider><TerminalSession taskId="a" root={0} onClosed={()=>{}}/></TooltipProvider>);view.unmount();
 await act(async()=>resolve({id:'old',taskId:'a',output:'',start:0,end:0,exited:false}));
 await waitFor(()=>expect(terminal.close).toHaveBeenCalledWith('a','old'));
 expect(mocks.write).not.toHaveBeenCalled();
});
it('renders byte data and disables input at a real exit',async()=>{
 vi.mocked(terminal.create).mockResolvedValue({id:'new',taskId:'a',output:btoa('hello'),start:0,end:5,exited:true,exitCode:7,error:null});
 render(<TooltipProvider><TerminalSession taskId="a" root={0} onClosed={()=>{}}/></TooltipProvider>);
 await screen.findByText('已退出 · 7');expect(mocks.write.mock.calls[0][0]).toEqual(Uint8Array.from([104,101,108,108,111]));
});
it('keeps multiple PTYs mounted in tabs and binds each to its selected root',async()=>{
 vi.mocked(terminal.create).mockImplementation(async(taskId,root)=>({id:`terminal-${root}`,taskId,output:'',start:0,end:0,exited:true,exitCode:0,error:null}));
 render(<TooltipProvider><TerminalPanel task={{id:'task-a',projectId:'project-a',title:'Task',roots:['/main','/docs'],pinned:false,archived:false,sessionId:null,sessionFile:null,model:null}}/></TooltipProvider>);
 fireEvent.click(screen.getByRole('button',{name:'启动终端'}));
 fireEvent.change(screen.getByRole('combobox',{name:'终端执行目录'}),{target:{value:'1'}});
 fireEvent.click(screen.getByRole('button',{name:'启动终端'}));
 await waitFor(()=>expect(terminal.create).toHaveBeenCalledTimes(2));
 expect(terminal.create).toHaveBeenNthCalledWith(1,'task-a',0);
 expect(terminal.create).toHaveBeenNthCalledWith(2,'task-a',1);
 expect(screen.getAllByRole('tab')).toHaveLength(2);
 expect(screen.getAllByLabelText('终端屏幕')).toHaveLength(2);
});
it('restores only terminal tab intent and creates a fresh PTY after confirmation',async()=>{
 localStorage.setItem('cool-pi-desktop.terminal-tabs:task-a',JSON.stringify([{root:1,title:'Docs shell'}]));
 vi.mocked(terminal.create).mockResolvedValue({id:'fresh',taskId:'task-a',output:'',start:0,end:0,exited:true,exitCode:0,error:null});
 render(<TooltipProvider><TerminalPanel task={{id:'task-a',projectId:'project-a',title:'Task',roots:['/main','/docs'],pinned:false,archived:false,sessionId:null,sessionFile:null,model:null}}/></TooltipProvider>);
 expect(screen.getByText(/上次终端进程已退出/)).toBeTruthy();
 expect(terminal.create).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'重新启动终端'}));
 await waitFor(()=>expect(terminal.create).toHaveBeenCalledWith('task-a',1));
});
