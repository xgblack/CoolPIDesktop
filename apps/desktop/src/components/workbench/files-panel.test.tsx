// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {FilesPanel} from './files-panel';
import {resources} from '@/host';
import {TooltipProvider} from '@/components/ui/tooltip';
import type {TaskRecord} from '../../../../../packages/host-contract/src';

vi.mock('@/host',()=>({
 resources:{
  list:vi.fn(),search:vi.fn(),preview:vi.fn(),attachments:vi.fn(),
  import:vi.fn(),previewAttachment:vi.fn(),
 },
 hostError:(error:unknown)=>error,
}));

afterEach(()=>{cleanup();vi.clearAllMocks();});

const task:TaskRecord={id:'task-a',projectId:'project-a',title:'Task',roots:['/workspace'],pinned:false,archived:false,sessionId:null,sessionFile:null,model:null};

it('searches inside the selected trusted root and opens results with line numbers',async()=>{
 vi.mocked(resources.list).mockResolvedValue({entries:[{name:'src',path:'src',kind:'directory',size:0}],truncated:false});
 vi.mocked(resources.search).mockResolvedValue({entries:[{name:'main.ts',path:'src/main.ts',kind:'file',size:24}],truncated:false});
 vi.mocked(resources.preview).mockResolvedValue({name:'src/main.ts',size:24,state:'text',text:'const value = 1;\nreturn value;'});
 render(<TooltipProvider><FilesPanel task={task}/></TooltipProvider>);
 await screen.findByText('src');
 fireEvent.change(screen.getByLabelText('搜索任务文件'),{target:{value:'main'}});
 expect(await screen.findByText('src/main.ts')).toBeTruthy();
 expect(resources.search).toHaveBeenCalledWith('task-a',0,'main');
 fireEvent.click(screen.getByTitle('src/main.ts'));
 await screen.findByLabelText('在预览中查找');
 expect(document.querySelectorAll('.file-line-number')).toHaveLength(2);
 fireEvent.change(screen.getByLabelText('在预览中查找'),{target:{value:'value'}});
 await waitFor(()=>expect(document.querySelectorAll('.file-source mark')).toHaveLength(2));
});
