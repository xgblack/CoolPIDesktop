import {useEffect,useMemo,useState} from 'react';
import {Activity,ArrowUpCircle,LoaderCircle,LogIn,PackagePlus,RefreshCw,ShieldCheck,Trash2} from 'lucide-react';
import type {HostError,LoginProvider,PluginOverview,Project,ProviderUsage} from '../../../../../packages/host-contract/src';
import {hostError,plugins,providers} from '@/host';
import {desktopPlatformShell} from '@/platform-shell';
import {Button} from '@/components/ui/button';
import {Input} from '@/components/ui/input';
import {ErrorNotice} from './shared';

type UsageReport={provider?:string;fetchedAt?:number;limits?:{id?:string;label?:string;status?:string;amount?:{used?:number;limit?:number;remaining?:number;unit?:string}}[];notes?:string[]};
type PluginRow={id:string;version:string;description:string;enabled:boolean;scope:'user'|'project';marketplace:boolean};

function usageReports(value:ProviderUsage|null):UsageReport[]{return Array.isArray(value?.reports)?value.reports.filter(item=>!!item&&typeof item==='object') as UsageReport[]:[]}

export function ProviderSettings({taskId,onLoginStarted}:{taskId?:string;onLoginStarted:()=>void}){
 const [items,setItems]=useState<LoginProvider[]>([]),[usage,setUsage]=useState<ProviderUsage|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState<HostError|null>(null),[logoutTarget,setLogoutTarget]=useState<LoginProvider>();
 const load=async()=>{setBusy(true);setError(null);const [login,usageResult]=await Promise.allSettled([taskId?providers.loginOptions(taskId):Promise.resolve({providers:[]}),providers.usage()]);if(login.status==='fulfilled')setItems(login.value.providers);if(usageResult.status==='fulfilled')setUsage(usageResult.value);const failed=login.status==='rejected'?login.reason:usageResult.status==='rejected'?usageResult.reason:null;if(failed)setError(hostError(failed));setBusy(false)};
 useEffect(()=>{void load()},[taskId]);
 const login=(provider:LoginProvider)=>{if(!taskId)return;const pending=providers.login(taskId,provider.id);onLoginStarted();void pending.then(()=>desktopPlatformShell.notify('Provider 登录完成',{body:provider.name})).catch(value=>desktopPlatformShell.notify('Provider 登录失败',{body:hostError(value).message}));};
 const logout=async()=>{if(!logoutTarget)return;setBusy(true);setError(null);try{await providers.logout(logoutTarget.id,true);setLogoutTarget(undefined);await load()}catch(value){setError(hostError(value));setBusy(false)}};
 const reports=usageReports(usage);
 return <div className="ecosystem-settings">
  <div className="settings-toolbar"><p>{taskId?'登录通过当前任务的 OMP RPC 完成，凭据不会返回客户端。':'选择并启动一个任务后可管理 Provider 登录。'}</p><Button size="sm" variant="outline" disabled={busy} onClick={()=>void load()}>{busy?<LoaderCircle className="animate-spin"/>:<RefreshCw/>}刷新</Button></div>
  <section className="settings-section settings-section-plain" aria-label="登录 Provider"><h3>登录方式</h3><div className="ecosystem-list">{items.map(provider=><div key={provider.id} className="ecosystem-row"><div><strong>{provider.name}</strong><small>{provider.id} · {provider.available?'可用':'当前环境不可用'} · {provider.authenticated?'已认证':'未认证'}</small></div><div className="ecosystem-actions"><Button size="sm" variant="outline" disabled={busy||!provider.available||!taskId} onClick={()=>login(provider)}><LogIn/>{provider.authenticated?'重新登录':'登录'}</Button>{provider.authenticated&&<Button size="icon-sm" variant="ghost" aria-label={`登出 ${provider.name}`} disabled={busy} onClick={()=>setLogoutTarget(provider)}><Trash2/></Button>}</div></div>)}{!busy&&!items.length&&<p className="settings-empty">{taskId?'OMP 未提供可登录的 Provider。':'当前没有可用于登录的运行任务。'}</p>}</div></section>
  {logoutTarget&&<div className="plugin-confirm" role="alert"><strong>确认登出 {logoutTarget.name}</strong><p>OMP 将删除该 Provider 的本地已存凭据。环境变量和 models.yml 中的密钥不会被修改；运行中的任务可能需要重启才能刷新状态。</p><div><Button variant="outline" size="sm" disabled={busy} onClick={()=>setLogoutTarget(undefined)}>取消</Button><Button variant="destructive" size="sm" disabled={busy} onClick={()=>void logout()}>{busy?<LoaderCircle className="animate-spin"/>:null}确认登出</Button></div></div>}
  <section className="settings-section settings-section-plain" aria-label="Provider 用量"><h3>用量</h3>{reports.length?<div className="ecosystem-list">{reports.map((report,index)=><div className="usage-report" key={`${report.provider??'provider'}:${index}`}><div><strong>{report.provider??'未知 Provider'}</strong><small>{report.fetchedAt?new Date(report.fetchedAt).toLocaleString():'更新时间未知'}</small></div>{(report.limits??[]).map((limit,limitIndex)=><div className="usage-limit" key={limit.id??limitIndex}><span>{limit.label??limit.id??'额度'}</span><strong>{limit.amount?.remaining??'—'} / {limit.amount?.limit??'—'} {limit.amount?.unit??''}</strong><small>{limit.status??'unknown'}</small></div>)}{report.notes?.map(note=><p key={note}>{note}</p>)}</div>)}</div>:<p className="settings-empty"><Activity/>没有可展示的 Provider 用量。未提供 usage endpoint 的账号不会伪装为失败。</p>}</section>
  <p className="settings-security-note"><ShieldCheck/>用量命令固定使用 <code>omp usage --json --redact</code>，账号标识在进入 UI 前由 OMP 脱敏。</p><ErrorNotice error={error}/>
 </div>;
}

function pluginRows(value:PluginOverview|null):PluginRow[]{
 const npm=Array.isArray(value?.plugins?.npm)?value!.plugins.npm as Record<string,unknown>[]:[];
 const marketplace=Array.isArray(value?.plugins?.marketplace)?value!.plugins.marketplace as Record<string,unknown>[]:[];
 return [
  ...npm.map(item=>({id:String(item.name??''),version:String(item.version??'unknown'),description:String((item.manifest as Record<string,unknown>|undefined)?.description??''),enabled:item.enabled!==false,scope:'user' as const,marketplace:false})).filter(item=>item.id),
  ...marketplace.map(item=>{const entries=Array.isArray(item.entries)?item.entries as Record<string,unknown>[]:[];const entry=entries[0]??{};return{id:String(item.id??''),version:String(entry.version??'unknown'),description:item.shadowedBy?'被项目级安装覆盖':'Marketplace',enabled:entry.enabled!==false,scope:item.scope==='project'?'project' as const:'user' as const,marketplace:true}}).filter(item=>item.id),
 ];
}

export function PluginSettings({projects,projectId}:{projects:Project[];projectId:string}){
 const [selectedProject,setSelectedProject]=useState(projectId),[overview,setOverview]=useState<PluginOverview|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState<HostError|null>(null),[installId,setInstallId]=useState(''),[installScope,setInstallScope]=useState<'user'|'project'>(projectId?'project':'user'),[pending,setPending]=useState<{action:'install'|'uninstall'|'upgrade';id:string;scope:'user'|'project'}>();
 const selected=projects.find(project=>project.id===selectedProject);
 const load=async()=>{setBusy(true);setError(null);try{setOverview(await plugins.overview(selectedProject||null))}catch(value){setError(hostError(value))}finally{setBusy(false)}};
 useEffect(()=>{void load()},[selectedProject]);
 const rows=useMemo(()=>pluginRows(overview),[overview]);
 const change=async(row:PluginRow)=>{setBusy(true);setError(null);try{await plugins.setEnabled(row.scope==='project'?selectedProject:null,row.id,!row.enabled,row.scope);await load()}catch(value){setError(hostError(value));setBusy(false)}};
 const mutate=async()=>{if(!pending)return;setBusy(true);setError(null);try{setOverview(await plugins.mutate(pending.scope==='project'?selectedProject:null,pending.action,pending.id,pending.scope,true));if(pending.action==='install')setInstallId('');setPending(undefined)}catch(value){setError(hostError(value))}finally{setBusy(false)}};
 const installValid=installId.length<=160&&installId.lastIndexOf('@')>0&&installId.lastIndexOf('@')<installId.length-1&&!installId.includes(' ')&&!installId.includes('..');
 return <div className="ecosystem-settings">
  <div className="settings-toolbar"><label>项目上下文<select value={selectedProject} onChange={event=>setSelectedProject(event.target.value)}><option value="">仅用户级</option>{projects.map(project=><option key={project.id} value={project.id}>{project.name}</option>)}</select></label><Button size="sm" variant="outline" disabled={busy} onClick={()=>void load()}>{busy?<LoaderCircle className="animate-spin"/>:<RefreshCw/>}刷新</Button></div>
  {selected&&!selected.trusted&&<p role="alert">该项目未受信任，Host 不会从其目录加载插件信息。</p>}
  <section className="settings-section settings-section-plain plugin-install" aria-label="安装 Marketplace Plugin"><h3>安装 Marketplace Plugin</h3><div><Input aria-label="Marketplace Plugin ID" placeholder="name@marketplace" value={installId} maxLength={160} onChange={event=>setInstallId(event.target.value.trim())}/><select aria-label="Plugin 安装范围" value={installScope} onChange={event=>setInstallScope(event.target.value as 'user'|'project')}><option value="user">用户级</option><option value="project" disabled={!selectedProject}>项目级</option></select><Button size="sm" disabled={busy||!installValid||installScope==='project'&&(!selectedProject||!selected?.trusted)} onClick={()=>setPending({action:'install',id:installId,scope:installScope})}><PackagePlus/>安装</Button></div><p>仅接受 OMP Marketplace 的 <code>name@marketplace</code> 标识；URL、Git、本地路径和任意 npm 源不会从此入口执行。</p></section>
  {pending&&<div className="plugin-confirm" role="alert"><strong>确认{pending.action==='install'?'安装':pending.action==='upgrade'?'升级':'卸载'} {pending.id}</strong><p>OMP 将在{pending.scope==='project'?'当前受信任项目':'用户级'}范围修改插件文件和登记信息。运行中的任务不会自动重载插件。</p><div><Button variant="outline" size="sm" disabled={busy} onClick={()=>setPending(undefined)}>取消</Button><Button variant={pending.action==='uninstall'?'destructive':'default'} size="sm" disabled={busy} onClick={()=>void mutate()}>{busy?<LoaderCircle className="animate-spin"/>:null}确认{pending.action==='install'?'安装':pending.action==='upgrade'?'升级':'卸载'}</Button></div></div>}
  <section className="settings-section settings-section-plain" aria-label="OMP Plugins"><div className="ecosystem-list">{rows.map(row=><div key={`${row.scope}:${row.id}`} className="ecosystem-row"><div><strong>{row.id}</strong><small>{row.version} · {row.scope==='project'?'项目级':'用户级'}{row.description?` · ${row.description}`:''}</small></div><div className="ecosystem-actions"><Button size="sm" variant={row.enabled?'outline':'secondary'} disabled={busy||(row.scope==='project'&&!selectedProject)} onClick={()=>void change(row)}>{row.enabled?'禁用':'启用'}</Button>{row.marketplace&&<><Button size="icon-sm" variant="ghost" aria-label={`升级 ${row.id}`} disabled={busy} onClick={()=>setPending({action:'upgrade',id:row.id,scope:row.scope})}><ArrowUpCircle/></Button><Button size="icon-sm" variant="ghost" aria-label={`卸载 ${row.id}`} disabled={busy} onClick={()=>setPending({action:'uninstall',id:row.id,scope:row.scope})}><Trash2/></Button></>}</div></div>)}{!busy&&!rows.length&&<p className="settings-empty">没有已安装的插件。可输入已配置 Marketplace 中的插件 ID 进行安装。</p>}</div></section>
  {!!overview?.diagnostics?.length&&<section className="settings-section settings-section-plain" aria-label="Plugin Doctor"><h3>诊断</h3><div className="plugin-diagnostics">{overview.diagnostics.map(item=><p key={item.name} data-status={item.status}><strong>{item.name}</strong><span>{item.message}</span></p>)}</div></section>}<ErrorNotice error={error}/>
 </div>;
}
