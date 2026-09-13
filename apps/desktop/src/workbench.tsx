import {useEffect,useRef,useState} from 'react';
import {AppFrame} from '@/components/workbench/app-frame';
import {FolderOpen,Menu,PanelRight,Plus,Search,RefreshCw} from 'lucide-react';
import {host,hostError,projects,records,downloadSession} from './host';
import type {ApprovalMode,TaskRecord,HostError} from '../../../packages/host-contract/src';
import {Button} from '@/components/ui/button';
import {TooltipProvider} from '@/components/ui/tooltip';
import {Sheet,SheetContent,SheetTitle,SheetDescription} from '@/components/ui/sheet';
import {useWorkbench} from '@/components/workbench/use-workbench';
import {useTheme} from '@/components/workbench/theme';
import {useApprovalPreference} from '@/components/workbench/approval-preference';
import {ProjectSidebar} from '@/components/workbench/project-sidebar';
import {TaskHeader,type Capabilities} from '@/components/workbench/task-header';
import {MessageList} from '@/components/workbench/message-list';
import {TrajectoryView} from '@/components/workbench/trajectory/trajectory-view';
import {Composer} from '@/components/workbench/composer';
import {LocalProjectDialog} from '@/components/workbench/local-project-dialog';
import {ApprovalPanel} from '@/components/workbench/approval-panel';
import {WorkbenchInspector} from '@/components/workbench/workbench-inspector';
import {useTaskRuntime} from '@/components/workbench/use-task-runtime';
import {NameDialog,RuntimeSettings,RecoveryDialog,type SettingsTab} from '@/components/workbench/workbench-dialogs';
import {ErrorNotice,IconButton} from '@/components/workbench/shared';
import './style.css';

export function App(){
 const w=useWorkbench(),theme=useTheme();
 const [sessionError,setSessionError]=useState<HostError|null>(null);
 const [sessionNotice,setSessionNotice]=useState(''),[exporting,setExporting]=useState<string|null>(null);
 const exportPending=useRef(false);
 const copySessionId=async(target:TaskRecord)=>{
  setSessionNotice('');setSessionError(null);
  if(!target.sessionId)return;
  try{await navigator.clipboard.writeText(target.sessionId);setSessionNotice(`已复制「${target.title}」的会话 ID`);}
  catch{setSessionError({code:'clipboard_failed',message:'剪贴板不可用，请手动复制会话 ID。',suggestion:target.sessionId});}
 };
 const exportSession=async(target:TaskRecord)=>{
  if(exportPending.current)return;
  exportPending.current=true;setSessionError(null);setExporting(target.id);setSessionNotice(`正在准备「${target.title}」的会话归档…`);
  try{const path=await downloadSession(target.id);setSessionNotice(path?`会话已保存至 ${path}`:'已取消保存会话');}
  catch(e){setSessionNotice('');setSessionError(hostError(e));}
  finally{exportPending.current=false;setExporting(null);}
 };
 const runtime=useTaskRuntime(w.task?.archived?null:w.taskId||null);
 const [search,setSearch]=useState({taskId:'',query:''});
 const query=search.taskId===w.taskId?search.query:'';
 const [inspectorTab,setInspectorTab]=useState('files');
 const [archived,setArchived]=useState(false),[sidebar,setSidebar]=useState(true),[drawer,setDrawer]=useState(false),[details,setDetails]=useState(false);
 const [narrow,setNarrow]=useState(()=>matchMedia('(max-width: 1023px)').matches);
 const [settings,setSettings]=useState(false),[recovery,setRecovery]=useState<'task'|'project'|null>(null),[trajectoryOpen,setTrajectoryOpen]=useState(false);
 const [settingsTab,setSettingsTab]=useState<SettingsTab>('models');
 const openSettings=(tab:SettingsTab='models')=>{remember();setSettingsTab(tab);setSettings(true);setDrawer(false);};
 const [approvalSwitching,setApprovalSwitching]=useState<string|null>(null);
 const approvalPreference=useApprovalPreference();
 const saveApprovalPreference=(mode:ApprovalMode|null)=>{try{approvalPreference.change(mode);}catch{w.setError({code:'approval_preference_save_failed',message:'无法保存应用级审批偏好，请重试。'});}};
 useEffect(()=>{if(approvalPreference.error)w.setError({code:'approval_preference_read_failed',message:approvalPreference.error});},[approvalPreference.error]);
 const changeApproval=(id:string,mode:ApprovalMode|null)=>{setApprovalSwitching(id);void w.act(async()=>{await records.approval(id,mode);saveApprovalPreference(mode);}).finally(()=>setApprovalSwitching(null));};
 const [creating,setCreating]=useState(false),[firstMessage,setFirstMessage]=useState('');
 const newConversation=()=>{if(w.project){w.chooseProject(w.project.id);}else openName({kind:'project',initial:''});};
 const sendFirst=async()=>{const text=w.drafts[`project:${w.projectId}`]??'';if(!text.trim()||creating)return;setCreating(true);setFirstMessage(text);const projectId=w.projectId;const selected=w.projectModels[projectId];if(!selected||!w.models.some(m=>m.provider+'/'+m.id===selected)){setCreating(false);setFirstMessage('');return;}try{const created=await records.create(projectId,text.trim().slice(0,80),'shared',selected);if(approvalPreference.mode)await records.approval(created.id,approvalPreference.mode);await w.refresh();w.chooseTask(created);w.setDraft(`project:${projectId}`,'');await w.send(created.id,false,text,w.references[`project:${projectId}`]??[]);w.setReferences(old=>({...old,[`project:${projectId}`]:[]}));}catch(e){w.setError(hostError(e));}finally{setCreating(false);setFirstMessage('');}};
 const [dialog,setDialog]=useState<{kind:'project'|'task'|'rename';initial:string;task?:TaskRecord;project?:boolean}|null>(null);
 const trigger=useRef<HTMLElement|null>(null),main=useRef<HTMLElement|null>(null);
 const remember=()=>{trigger.current=document.activeElement instanceof HTMLElement?document.activeElement:null;};
 const restore=()=>{requestAnimationFrame(()=>{if(trigger.current?.isConnected)trigger.current.focus();else main.current?.focus();});};
 const openName=(value:NonNullable<typeof dialog>)=>{remember();setDialog(value);};
 useEffect(()=>{const media=matchMedia('(max-width: 1023px)');const change=()=>{setNarrow(media.matches);setDrawer(false);};media.addEventListener('change',change);return()=>media.removeEventListener('change',change);},[]);
 useEffect(()=>{const keydown=(e:KeyboardEvent)=>{if(!(e.metaKey||e.ctrlKey)||e.altKey)return;if(e.key.toLowerCase()==='n'&&!settings&&!dialog){e.preventDefault();newConversation();}if(e.key.toLowerCase()==='k'&&!settings&&!dialog){e.preventDefault();setSidebar(true);setDrawer(true);setTimeout(()=>document.querySelector<HTMLInputElement>('[aria-label="搜索任务"]')?.focus(),100);}};window.addEventListener('keydown',keydown);return()=>window.removeEventListener('keydown',keydown);},[w.project,settings,dialog]);
 const {task,run}=w;
 const runtimeInfo=runtime.tasks.find(t=>t.taskId===task?.id),external=runtimeInfo?.owner==='terminal';
 const openRuntime=(target:TaskRecord)=>{if(target.id!==w.taskId)w.chooseTask(target);setInspectorTab('runtime');setDetails(true);setDrawer(false);};
 const capabilities=run?.runtime?.capabilities as Capabilities|undefined;
 const ready=!external&&!!run&&['ready','idle','interrupted'].includes(run.status),running=!external&&run?.status==='running';
 const key=(task?.id??'')+':'+(run?.runId??'');
 const side=<ProjectSidebar onCopySessionId={t=>void copySessionId(t)} runtimeTasks={runtime.tasks} onRuntime={openRuntime} onRuntimeManagement={()=>openSettings('runtime')} projects={w.projects} tasks={w.tasks} runs={w.runs} projectId={w.projectId} taskId={w.taskId} loading={w.loading} busy={w.busy} showArchived={archived} onArchived={setArchived}
  onProject={w.chooseProject} onTask={t=>{w.chooseTask(t);setDrawer(false);}} onNewProject={()=>openName({kind:'project',initial:''})} onNewTask={newConversation}
  onRenameProject={()=>openName({kind:'rename',initial:w.project?.name??'',project:true})} onArchiveProject={()=>{if(w.project)void w.act(()=>projects.update(w.project!.id,w.project!.name,!w.project!.archived));}}
  onRenameTask={t=>openName({kind:'rename',initial:t.title,task:t})} onUpdateTask={t=>void w.act(()=>records.update(t))}
  onSettings={()=>{openSettings();}} onClose={()=>narrow?setDrawer(false):setSidebar(false)} onPaths={()=>{remember();setRecovery('project');}}/>;
 const inspector=task?<WorkbenchInspector key={task.id} task={task} run={run} busy={w.busy} tab={inspectorTab} onTab={setInspectorTab} runtime={{info:runtimeInfo,error:runtime.error,onRefresh:runtime.refresh}} onRefreshUsage={()=>void w.refreshUsage(task.id)} selected={w.attachments[task.id]??[]} onAttach={a=>w.addAttachment(task.id,a.id)}/>:null;
 const content=<main ref={main} tabIndex={-1} className="conversation">
  {task?<TaskHeader onCopySessionId={()=>void copySessionId(task)} onDownloadSession={()=>void exportSession(task)} exporting={exporting!==null} historyControls={!trajectoryOpen?<><label className="header-conversation-search"><Search size={13}/><input aria-label="搜索当前会话" placeholder="搜索会话" value={query} onChange={e=>setSearch({taskId:w.taskId,query:e.target.value})}/></label><IconButton label="刷新历史" disabled={w.historyBusy||running} onClick={()=>{if(!running)void w.history();}}><RefreshCw size={14}/></IconButton></>:undefined} onRuntime={()=>openRuntime(task)} task={task} run={run} busy={w.busy} sidebarVisible={!narrow} onSidebar={()=>narrow?setDrawer(true):setSidebar(true)} onStop={()=>void w.act(()=>host.stop(task.id))} onRestart={()=>void w.act(()=>host.restart(task.id))} onAbort={()=>void w.act(()=>host.abort(task.id))} onRecovery={()=>{remember();setRecovery('task');}} onDetails={()=>setDetails(v=>!v)} detailsOpen={details} onTrajectory={()=>setTrajectoryOpen(v=>!v)} trajectoryOpen={trajectoryOpen}/>:<header className="task-header"><div className="task-header-title">{(narrow||!sidebar)&&<IconButton label="打开侧栏" onClick={()=>narrow?setDrawer(true):setSidebar(true)}><Menu/></IconButton>}<h1>项目工作台</h1></div></header>}
  <div className="notice-stack"><ErrorNotice error={sessionError}/>{sessionNotice&&<p role="status" className="text-xs text-muted-foreground break-all">{sessionNotice}</p>}{(w.error?.code==='model_required'||run?.error?.code==='model_required')&&<Button variant="outline" onClick={()=>{openSettings();}}>配置模型</Button>}<ErrorNotice error={w.error}/><ErrorNotice error={run?.error}/><ErrorNotice error={w.historyError}/>
   {w.historyError&&ready&&<Button variant="outline" size="sm" disabled={w.historyBusy} onClick={()=>void w.history()}>重新加载历史</Button>}

  </div>

  {task?<>{trajectoryOpen?<TrajectoryView taskId={task.id} run={run} onError={e=>w.setError(hostError(e))}/>:<MessageList startedAt={run?.turnStartedAt??undefined} onFork={timestamp=>w.fork(task.id,timestamp)} forkDisabled={w.busy||external||task.archived||!task.sessionFile} query={query} tools={run?.tools} taskKey={task.id} messages={w.messages} streamingText={running||w.historyBusy?run?.text??'':''} running={running} hasMore={!!w.cursor} loading={w.historyBusy} onMore={()=>{if(!running)void w.history(true);}} onError={e=>w.setError(hostError(e))}/>}
   <ApprovalPanel key={`approval:${key}`} requests={run?.pendingUi??[]} busy={w.busy} onRespond={(id,value,confirmed,cancelled)=>void w.act(()=>host.respond(task.id,id,value,confirmed,cancelled))}/>
   {w.sendFailed&&<div role="alert" className="px-4"><span>消息未发送</span><Button variant="ghost" disabled={w.busy||external} onClick={()=>{if(!external)void w.send(task.id,true);}}>重试发送</Button></div>}
   <Composer modelDisabled={external} key={`composer:${task.id}`} taskId={task.id} approvalMode={task.approvalMode} onApprovalMode={mode=>changeApproval(task.id,mode)} approvalSwitching={approvalSwitching===task.id} approvalDisabled={external||task.archived||run?.status==='starting'||!!run?.pendingUi?.length||!!w.sendFailed} roots={task.roots} references={w.references[task.id]??[]} onReferences={refs=>w.setReferences(old=>({...old,[task.id]:refs}))} models={ready||running?capabilities?.models??[]:w.models} modelValue={ready||running?capabilities?.state?.model?capabilities.state.model.provider+'/'+capabilities.state.model.id:undefined:task.model??undefined} modelsLoading={!ready&&!running&&w.modelsLoading} modelsError={!ready&&!running?w.modelsError?.message:undefined} onRefreshModels={ready||running?undefined:w.refreshModels} onModel={(p,id)=>void w.act(()=>records.model(task.id,p,id))} draft={w.drafts[task.id]??''} onFiles={()=>setDetails(true)} onContinue={external?undefined:()=>void w.act(()=>records.resume(task.id))} onSettings={()=>openSettings()} modelLabel={ready||running?capabilities?.state?.model?.id:undefined} attachmentIds={w.attachments[task.id]??[]} onRemoveAttachment={id=>w.removeAttachment(task.id,id)} onDraft={v=>w.setDraft(task.id,v)} onSend={()=>{if(!external)void w.send(task.id);}} onAbort={()=>void w.act(()=>host.abort(task.id))} canSend={!external&&!task.archived&&!w.sendFailed} running={running} busy={w.busy} disabledReason={external?'终端正在接管此会话，退出终端后可继续发送。':task.archived?'任务已归档，请先恢复任务。':undefined}/>
  </>:<><div className="workspace-empty"><FolderOpen size={32}/><h2>{w.project?`你想在 ${w.project.name} 中构建什么？`:'今天，想一起完成什么？'}</h2>{w.project?<div className="starter-actions">{['探索并理解代码','构建新功能','审查代码','修复问题'].map(text=><Button key={text} variant="outline" disabled={creating} onClick={()=>w.setDraft(`project:${w.projectId}`,text)}>{text}</Button>)}</div>:<Button disabled={w.busy} onClick={()=>openName({kind:'project',initial:''})}><Plus/>添加项目</Button>}</div>{firstMessage&&<div className="user-bubble mx-6" role="status">{firstMessage}</div>}{w.project&&<Composer key={`project:${w.projectId}`} taskId={`project:${w.projectId}`} approvalMode={approvalPreference.mode} onApprovalMode={saveApprovalPreference} models={w.models} modelValue={w.projectModels[w.projectId]} modelsLoading={w.modelsLoading} modelsError={w.modelsError?.message} onRefreshModels={w.refreshModels} onModel={(provider,id)=>void w.act(()=>w.selectProjectModel(provider,id))} roots={w.project.roots} references={w.references[`project:${w.projectId}`]??[]} onReferences={refs=>w.setReferences(old=>({...old,[`project:${w.projectId}`]:refs}))} draft={w.drafts[`project:${w.projectId}`]??''} onDraft={text=>w.setDraft(`project:${w.projectId}`,text)} onSend={()=>void sendFirst()} onAbort={()=>{}} onSettings={()=>openSettings()} canSend={!w.project.archived&&!w.modelsLoading&&!w.modelsError&&w.models.some(m=>m.provider+'/'+m.id===w.projectModels[w.projectId])} busy={creating||w.busy} running={false} disabledReason={w.modelsLoading?'正在加载模型…':w.modelsError?'模型加载失败，请刷新模型列表重试。':!w.models.length?'没有可用模型，请配置模型。':!w.models.some(m=>m.provider+'/'+m.id===w.projectModels[w.projectId])?'请选择模型后开始对话。':undefined}/>}</>}
 </main>;
 return <TooltipProvider><div className="workbench">{narrow?content:<AppFrame sidebar={side} collapsed={!sidebar} onExpand={()=>setSidebar(true)} onNew={newConversation} onSearch={()=>{setSidebar(true);requestAnimationFrame(()=>document.querySelector<HTMLInputElement>('[aria-label="搜索任务"]')?.focus());}} onSettings={()=>{openSettings();}} inspector={inspector} inspectorOpen={details} onCloseInspector={()=>setDetails(false)}>{content}</AppFrame>}</div>
  {narrow&&<Sheet open={drawer} onOpenChange={setDrawer}><SheetContent side="left" showCloseButton={false} className="w-[min(320px,90vw)] gap-0 p-0"><SheetTitle className="sr-only">项目与任务</SheetTitle><SheetDescription className="sr-only">选择项目和任务</SheetDescription>{side}</SheetContent></Sheet>}
  {narrow&&inspector&&<Sheet open={details} onOpenChange={setDetails}><SheetContent side="right" className="w-[min(420px,94vw)] overflow-y-auto p-0 pt-8"><SheetTitle className="sr-only">工具、用量与 Git 变更</SheetTitle><SheetDescription className="sr-only">当前任务的工具执行、上下文用量和只读 Git 变更</SheetDescription>{inspector}</SheetContent></Sheet>}
  {(dialog?.kind==='project'||dialog?.project)?<LocalProjectDialog project={dialog?.project?w.project:undefined} onClose={()=>setDialog(null)} onCreated={async p=>{await w.refresh();if(!dialog?.project)w.chooseProject(p.id);}}/>:dialog&&<NameDialog {...dialog} onClose={()=>setDialog(null)} restoreFocus={restore} onSubmit={async(name,trusted,mode)=>{if(dialog.kind==='task'){const t=await records.create(w.projectId,name,mode);await w.refresh();w.chooseTask(t);setDrawer(false);}else if(dialog.task){await records.update({...dialog.task,title:name});await w.refresh();}else if(dialog.project&&w.project){await projects.update(w.project.id,name,w.project.archived);await w.refresh();}}}/>}
  <RuntimeSettings tab={settingsTab} onTab={setSettingsTab} runtimeTasks={runtime.tasks} tasks={w.tasks} runtimeError={runtime.error} onRefreshRuntime={runtime.refresh} onOpenTask={target=>{setSettings(false);openRuntime(target);}} onSaved={async()=>{await w.refreshModels();await w.refresh();}} open={settings} onClose={()=>setSettings(false)} theme={theme.theme} onTheme={theme.change} themeError={theme.error} restoreFocus={restore}/>
  {recovery&&<RecoveryDialog task={recovery==='task'?task:undefined} roots={(recovery==='task'?task?.roots:w.project?.roots)??[]} onClose={()=>setRecovery(null)} onChanged={w.refresh} restoreFocus={restore}/>}
 </TooltipProvider>;
}
import './design-tokens.css';
import './components/workbench/app-frame.css';
