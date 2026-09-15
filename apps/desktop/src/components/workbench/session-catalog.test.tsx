// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {SessionCatalog} from './session-catalog';
import type {SessionCatalogEntry} from '../../../../../packages/host-contract/src';

afterEach(()=>{cleanup();vi.clearAllMocks()});

const bound:SessionCatalogEntry={sessionKey:'bound',sessionId:'session-a',cwd:'/trusted/project',title:'已绑定会话',createdAt:1,updatedAt:2,relationKind:'root',status:'complete',messageCount:4,trusted:true,projectId:'project-a',taskId:'task-a',running:false,unread:true,archived:false,parseState:'ready'};
const external:SessionCatalogEntry={sessionKey:'external',sessionId:'opaque-parent',cwd:'/outside/project',title:'外部会话',createdAt:1,updatedAt:2,relationKind:'fork',parentSession:'not-a-uuid',status:'unknown',messageCount:2,trusted:false,running:false,unread:false,archived:false,parseState:'partial'};

it('opens only bound tasks and archives catalog metadata without starting external sessions',async()=>{
 const service={list:vi.fn().mockResolvedValue({entries:[bound,external],total:2,nextOffset:null}),refresh:vi.fn(),updateView:vi.fn().mockResolvedValue(undefined)};
 const openTask=vi.fn(),openChange=vi.fn();
 render(<SessionCatalog open onOpenChange={openChange} onOpenTask={openTask} service={service as never}/>);
 expect(await screen.findByText('已绑定会话')).toBeTruthy();
 const externalButton=screen.getByRole('button',{name:'外部会话 尚未绑定到工作台任务'}) as HTMLButtonElement;
 expect(externalButton.disabled).toBe(true);
 expect(screen.getByText('只读元数据')).toBeTruthy();
 expect(screen.getByText('索引不完整')).toBeTruthy();

 fireEvent.click(screen.getAllByRole('button',{name:'归档会话目录项'})[0]);
 await waitFor(()=>expect(service.updateView).toHaveBeenCalledWith('bound',true));
 fireEvent.click(screen.getByRole('button',{name:'打开 已绑定会话'}));
 await waitFor(()=>expect(openTask).toHaveBeenCalledWith('task-a'));
 expect(service.updateView).toHaveBeenCalledWith('bound',null);
 expect(openChange).toHaveBeenCalledWith(false);
});
