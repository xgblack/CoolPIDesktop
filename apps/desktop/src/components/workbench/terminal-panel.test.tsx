// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {act,cleanup,render,screen,waitFor} from '@testing-library/react';
import {TerminalSession} from './terminal-panel';
import {terminal} from '@/host';
import {TooltipProvider} from '@/components/ui/tooltip';
const mocks=vi.hoisted(()=>({write:vi.fn((_b:unknown,cb:()=>void)=>cb()),dispose:vi.fn()}));
vi.mock('@xterm/xterm',()=>({Terminal:class {options={};cols=80;rows=24;loadAddon(){}open(){}focus(){}reset(){}writeln(){}write=mocks.write;dispose=mocks.dispose;onData(){return{dispose(){}}}}}));
vi.mock('@xterm/addon-fit',()=>({FitAddon:class{fit(){}}}));
vi.mock('@/host',()=>({terminal:{create:vi.fn(),close:vi.fn().mockResolvedValue(undefined),snapshot:vi.fn(),resize:vi.fn().mockResolvedValue(undefined),write:vi.fn()},hostError:(e:unknown)=>e}));
vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});
afterEach(()=>{cleanup();vi.clearAllMocks()});
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
