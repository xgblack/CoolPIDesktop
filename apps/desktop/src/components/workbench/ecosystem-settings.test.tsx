// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {PluginSettings,ProviderSettings} from './ecosystem-settings';
import {plugins,providers} from '@/host';

vi.mock('@/host',()=>({
 providers:{loginOptions:vi.fn(),login:vi.fn(),logout:vi.fn(),usage:vi.fn()},
 plugins:{overview:vi.fn(),setEnabled:vi.fn(),mutate:vi.fn()},
 hostError:(value:unknown)=>value,
}));
vi.mock('@/platform-shell',()=>({desktopPlatformShell:{notify:vi.fn().mockResolvedValue(undefined)}}));

beforeEach(()=>{
 vi.clearAllMocks();
 vi.mocked(providers.loginOptions).mockResolvedValue({providers:[{id:'oauth-test',name:'OAuth Test',available:true,authenticated:false}]});
 vi.mocked(providers.login).mockResolvedValue({providerId:'oauth-test'});
 vi.mocked(providers.logout).mockResolvedValue(undefined);
 vi.mocked(providers.usage).mockResolvedValue({generatedAt:1,reports:[{provider:'oauth-test',fetchedAt:1,limits:[{id:'daily',label:'Daily',status:'ok',amount:{remaining:80,limit:100,unit:'requests'}}]}]});
 vi.mocked(plugins.overview).mockResolvedValue({plugins:{npm:[{name:'@omp/example',version:'1.2.3',enabled:true,manifest:{description:'Example tools'}}],marketplace:[]},diagnostics:[{name:'plugins_directory',status:'ok',message:'Ready'}]});
 vi.mocked(plugins.setEnabled).mockResolvedValue({disabled:'@omp/example'});
 vi.mocked(plugins.mutate).mockResolvedValue({plugins:{npm:[],marketplace:[]},diagnostics:[]});
});
it('requires explicit confirmation before a scoped marketplace installation',async()=>{
 render(<PluginSettings projects={[{id:'project-a',name:'Project',roots:['/workspace'],archived:false,trusted:true}]} projectId="project-a"/>);
 await screen.findByText('@omp/example');
 fireEvent.change(screen.getByLabelText('Marketplace Plugin ID'),{target:{value:'review@official'}});
 fireEvent.click(screen.getByRole('button',{name:'安装'}));
 expect(plugins.mutate).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'确认安装'}));
 await waitFor(()=>expect(plugins.mutate).toHaveBeenCalledWith('project-a','install','review@official','project',true));
});
afterEach(cleanup);

it('loads redacted usage and starts login through the selected task',async()=>{
 const started=vi.fn();
 render(<ProviderSettings taskId="task-a" onLoginStarted={started}/>);
 expect(await screen.findByText('OAuth Test')).toBeTruthy();
 expect(screen.getByText('80 / 100 requests')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'登录'}));
 expect(providers.login).toHaveBeenCalledWith('task-a','oauth-test');
 expect(started).toHaveBeenCalledOnce();
});
it('requires confirmation before OMP deletes stored provider credentials',async()=>{
 vi.mocked(providers.loginOptions).mockResolvedValue({providers:[{id:'oauth-test',name:'OAuth Test',available:true,authenticated:true}]});
 render(<ProviderSettings taskId="task-a" onLoginStarted={vi.fn()}/>);
 fireEvent.click(await screen.findByRole('button',{name:'登出 OAuth Test'}));
 expect(providers.logout).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'确认登出'}));
 await waitFor(()=>expect(providers.logout).toHaveBeenCalledWith('oauth-test',true));
});

it('does not offer login without an OMP task',async()=>{
 render(<ProviderSettings onLoginStarted={vi.fn()}/>);
 await waitFor(()=>expect(providers.usage).toHaveBeenCalled());
 expect(providers.loginOptions).not.toHaveBeenCalled();
 expect(screen.getByText('当前没有可用于登录的运行任务。')).toBeTruthy();
});

it('keeps the provider list usable when usage refresh fails',async()=>{
 vi.mocked(providers.usage).mockRejectedValue({code:'usage_failed',message:'部分用量不可用'});
 render(<ProviderSettings taskId="task-a" onLoginStarted={vi.fn()}/>);
 expect(await screen.findByText('OAuth Test')).toBeTruthy();
 expect(screen.getByText('部分用量不可用')).toBeTruthy();
 expect(screen.getByRole('button',{name:'登录'})).toBeTruthy();
});

it('uses a typed plugin toggle and refreshes the authoritative list',async()=>{
 render(<PluginSettings projects={[]} projectId=""/>);
 expect(await screen.findByText('@omp/example')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'禁用'}));
 await waitFor(()=>expect(plugins.setEnabled).toHaveBeenCalledWith(null,'@omp/example',false,'user'));
 expect(plugins.overview).toHaveBeenCalledTimes(2);
 expect(screen.getByText('Ready')).toBeTruthy();
});
