// @vitest-environment jsdom
import {afterEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen} from '@testing-library/react';
import {RuntimeSettings,type SettingsTab} from './workbench-dialogs';

vi.mock('./runtime-management',()=>({RuntimeManagement:()=>null}));
vi.mock('./model-settings',()=>({ModelSettings:()=>null}));
vi.mock('./title-prompt-settings',()=>({TitlePromptSettings:()=>null}));
vi.mock('./agents-settings',()=>({AgentsSettings:()=>null}));
vi.mock('./ecosystem-settings',()=>({ProviderSettings:()=>null,PluginSettings:()=>null}));
vi.mock('../../host',()=>({
 host:{runtime:vi.fn(),chooseRuntime:vi.fn(),discoverRuntime:vi.fn()},
 hostError:(error:unknown)=>error,
 records:{},
 recoverSession:vi.fn(),
}));

afterEach(()=>{cleanup();vi.clearAllMocks();});

function mount(tab:SettingsTab='general'){
 const onTab=vi.fn();
 const onPreferences=vi.fn();
 render(<RuntimeSettings
  tab={tab}
  onTab={onTab}
  runtimeTasks={[]}
  tasks={[]}
  projects={[]}
  projectId=""
  runtimeError={null}
  onRefreshRuntime={vi.fn().mockResolvedValue(undefined)}
  onOpenTask={vi.fn()}
  onProviderLogin={vi.fn()}
  open
  onClose={vi.fn()}
  theme="system"
  onTheme={vi.fn()}
  themeError=""
  preferences={{chatWidth:'standard',fontSize:'standard',codeTheme:'adaptive',thinkingExpanded:false,notifications:true,sound:false}}
  onPreferences={onPreferences}
  preferencesError=""
  restoreFocus={vi.fn()}
  onSaved={vi.fn().mockResolvedValue(undefined)}
 />);
 return {onTab,onPreferences};
}

it('exposes every planned settings destination',()=>{
 const {onTab}=mount();
 const entries:[string,SettingsTab][]=[
  ['常规','general'],['外观','appearance'],['模型','models'],['Provider','providers'],
  ['Skills','skills'],['Agents','agents'],['Plugins','plugins'],['Runtime','runtime'],
 ];
 for(const [name,tab] of entries){
  fireEvent.click(screen.getByRole('button',{name}));
  expect(onTab).toHaveBeenLastCalledWith(tab);
 }
});

it('writes bounded reading and completion preferences through one callback',()=>{
 const {onPreferences}=mount('appearance');
 fireEvent.change(screen.getByLabelText('会话宽度'),{target:{value:'wide'}});
 fireEvent.change(screen.getByLabelText('正文字号'),{target:{value:'large'}});
 fireEvent.change(screen.getByLabelText('代码主题'),{target:{value:'monokai'}});
 fireEvent.click(screen.getByRole('checkbox',{name:'默认展开 Thinking'}));
 fireEvent.click(screen.getByRole('checkbox',{name:'任务完成时显示系统通知'}));
 fireEvent.click(screen.getByRole('checkbox',{name:'任务完成时播放提示音'}));
 expect(onPreferences.mock.calls.map(([patch])=>patch)).toEqual([
  {chatWidth:'wide'},
  {fontSize:'large'},
  {codeTheme:'monokai'},
  {thinkingExpanded:true},
  {notifications:false},
  {sound:true},
 ]);
});

it('states the verified Skills limitation instead of presenting file writes as support',()=>{
 mount('skills');
 expect(screen.getByRole('heading',{name:'Skills'})).toBeTruthy();
 expect(screen.getByText(/发现、启停、安装与更新保持不可用/)).toBeTruthy();
});
