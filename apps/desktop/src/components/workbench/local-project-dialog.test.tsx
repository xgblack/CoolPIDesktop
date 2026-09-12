// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {LocalProjectDialog} from './local-project-dialog';
import {localProjects} from '../../host';
vi.mock('../../host',()=>({localProjects:{edit:vi.fn().mockResolvedValue({id:'p'}),choose:vi.fn().mockResolvedValue([{token:'new',path:'/new'}])},hostError:(e:unknown)=>e}));
vi.stubGlobal('ResizeObserver',class {observe(){} unobserve(){} disconnect(){}});
afterEach(()=>{cleanup();vi.unstubAllGlobals();});
it('edits ordered directories and promotes a secondary directory without submitting until save',async()=>{
 render(<LocalProjectDialog project={{id:'p',name:'project',roots:['/a','/b'],archived:false,trusted:true}} onClose={vi.fn()} onCreated={async()=>{}}/>);
 fireEvent.click(screen.getByRole('button',{name:'设为主要'}));
 expect(localProjects.edit).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'添加文件夹'}));
 await screen.findByText('new');
 fireEvent.click(screen.getByRole('button',{name:'保存'}));
 await waitFor(()=>expect(localProjects.edit).toHaveBeenCalledWith('p','project',[{existing:1},{existing:0},{selected:'new'}],true));
});
