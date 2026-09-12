import {useEffect,useRef,useState} from 'react';
import {AppFrame} from '@/components/workbench/app-frame';
import {FolderOpen,Menu,PanelRight,Plus} from 'lucide-react';
import {host,hostError,projects,records} from './host';
import type {TaskRecord} from '../../../packages/host-contract/src';
import {Button} from '@/components/ui/button';
import {TooltipProvider} from '@/components/ui/tooltip';
import {Sheet,SheetContent,SheetTitle,SheetDescription} from '@/components/ui/sheet';
import {useWorkbench} from '@/components/workbench/use-workbench';
import {useTheme} from '@/components/workbench/theme';
import {ProjectSidebar} from '@/components/workbench/project-sidebar';
import {TaskHeader,type Capabilities} from '@/components/workbench/task-header';
import {MessageList} from '@/components/workbench/message-list';
import {Composer} from '@/components/workbench/composer';
import {LocalProjectDialog} from '@/components/workbench/local-project-dialog';
import {ApprovalPanel} from '@/components/workbench/approval-panel';
import {WorkbenchInspector} from '@/components/workbench/workbench-inspector';
import {NameDialog,RuntimeSettings,RecoveryDialog} from '@/components/workbench/workbench-dialogs';
import {ErrorNotice,IconButton} from '@/components/workbench/shared';
import './style.css';

export function App(){
 const w=useWorkbench(),theme=useTheme();
 const [archived,setArchived]=useState(false),[sidebar,setSidebar]=useState(true),[drawer,setDrawer]=useState(false),[details,setDetails]=useState(false);
 const [narrow,setNarrow]=useState(()=>matchMedia('(max-width: 1023px)').matches);
 const [settings,setSettings]=useState(false),[recovery,setRecovery]=useState<'task'|'project'|null>(null);
 const [creating,setCreating]=useState(false),[firstMessage,setFirstMessage]=useState('');
 const newConversation=()=>{if(w.project)w.chooseProject(w.project.id);else openName({kind:'project',initial:''});};
 const sendFirst=async()=>{const text=w.drafts[`project:${w.projectId}`]??'';if(!text.trim()||creating)return;setCreating(true);setFirstMessage(text);const projectId=w.projectId;try{const created=await records.create(projectId,text.trim().slice(0,80));await w.refresh();w.chooseTask(created);w.setDraft(`project:${projectId}`,'');await w.send(created.id,false,text,w.references[`project:${projectId}`]??[]);w.setReferences(old=>({...old,[`project:${projectId}`]:[]}));}catch(e){w.setError(hostError(e));}finally{setCreating(false);setFirstMessage('');}};
 const [dialog,setDialog]=useState<{kind:'project'|'task'|'rename';initial:string;task?:TaskRecord;project?:boolean}|null>(null);
 const trigger=useRef<HTMLElement|null>(null),main=useRef<HTMLElement|null>(null);
 const remember=()=>{trigger.current=document.activeElement instanceof HTMLElement?document.activeElement:null;};
 const restore=()=>{requestAnimationFrame(()=>{if(trigger.current?.isConnected)trigger.current.focus();else main.current?.focus();});};
 const openName=(value:NonNullable<typeof dialog>)=>{remember();setDialog(value);};
 useEffect(()=>{const media=matchMedia('(max-width: 1023px)');const change=()=>{setNarrow(media.matches);setDrawer(false);};media.addEventListener('change',change);return()=>media.removeEventListener('change',change);},[]);
 useEffect(()=>{const keydown=(e:KeyboardEvent)=>{if(!(e.metaKey||e.ctrlKey)||e.altKey)return;if(e.key.toLowerCase()==='n'&&!settings&&!dialog){e.preventDefault();newConversation();}if(e.key.toLowerCase()==='k'&&!settings&&!dialog){e.preventDefault();setSidebar(true);setDrawer(true);setTimeout(()=>document.querySelector<HTMLInputElement>('[aria-label="搜索任务"]')?.focus(),100);}};window.addEventListener('keydown',keydown);return()=>window.removeEventListener('keydown',keydown);},[w.project,settings,dialog]);
 const [navigation,setNavigation]=useState<{ids:string[];index:number}>({ids:[],index:-1});
 useEffect(()=>{if(!w.taskId)return;setNavigation(old=>old.ids[old.index]===w.taskId?old:{ids:[...old.ids.slice(0,old.index+1),w.taskId],index:old.index+1});},[w.taskId]);
 const navigate=(delta:number)=>{const index=navigation.index+delta;const target=w.tasks.find(t=>t.id===navigation.ids[index]);if(target){setNavigation({...navigation,index});w.chooseTask(target);}};
 const {task,run}=w;
 const capabilities=run?.runtime?.capabilities as Capabilities|undefined;
 const ready=!!run&&['ready','idle','interrupted'].includes(run.status),running=run?.status==='running';
 const key=(task?.id??'')+':'+(run?.runId??'');
 const side=<ProjectSidebar projects={w.projects} tasks={w.tasks} runs={w.runs} projectId={w.projectId} taskId={w.taskId} loading={w.loading} busy={w.busy} showArchived={archived} onArchived={setArchived}
  onProject={w.chooseProject} onTask={t=>{w.chooseTask(t);setDrawer(false);}} onNewProject={()=>openName({kind:'project',initial:''})} onNewTask={newConversation}
  onRenameProject={()=>openName({kind:'rename',initial:w.project?.name??'',project:true})} onArchiveProject={()=>{if(w.project)void w.act(()=>projects.update(w.project!.id,w.project!.name,!w.project!.archived));}}
  onRenameTask={t=>openName({kind:'rename',initial:t.title,task:t})} onUpdateTask={t=>void w.act(()=>records.update(t))}
  onSettings={()=>{remember();setSettings(true);}} onClose={()=>narrow?setDrawer(false):setSidebar(false)} onPaths={()=>{remember();setRecovery('project');}}/>;
 const inspector=task?<WorkbenchInspector key={task.id} task={task} run={run} busy={w.busy} onRefreshUsage={()=>void w.refreshUsage(task.id)} selected={w.attachments[task.id]??[]} onAttach={a=>w.addAttachment(task.id,a.id)}/>:null;
 const content=<main ref={main} tabIndex={-1} className="conversation">
  {task?<TaskHeader task={task} run={run} busy={w.busy} sidebarVisible={!narrow} onSidebar={()=>narrow?setDrawer(true):setSidebar(true)} onStop={()=>void w.act(()=>host.stop(task.id))} onRestart={()=>void w.act(()=>host.restart(task.id))} onAbort={()=>void w.act(()=>host.abort(task.id))} onRecovery={()=>{remember();setRecovery('task');}} onBack={navigation.index>0?()=>navigate(-1):undefined} onForward={navigation.index<navigation.ids.length-1?()=>navigate(1):undefined} onDetails={()=>setDetails(v=>!v)} detailsOpen={details}/>:<header className="task-header"><div className="task-header-title">{(narrow||!sidebar)&&<IconButton label="打开侧栏" onClick={()=>narrow?setDrawer(true):setSidebar(true)}><Menu/></IconButton>}<h1>项目工作台</h1></div></header>}
  <div className="notice-stack">{(w.error?.code==='model_required'||run?.error?.code==='model_required')&&<Button variant="outline" onClick={()=>{remember();setSettings(true);}}>配置模型</Button>}<ErrorNotice error={w.error}/><ErrorNotice error={run?.error}/><ErrorNotice error={w.historyError}/>
   {w.historyError&&ready&&<Button variant="outline" size="sm" disabled={w.historyBusy} onClick={()=>void w.history()}>重新加载历史</Button>}

  </div>

  {task?<><MessageList taskKey={key} messages={w.messages} streamingText={running||w.historyBusy?run?.text??'':''} running={running} hasMore={!!w.cursor} loading={w.historyBusy} onMore={()=>{if(!running)void w.history(true);}} onRefresh={()=>{if(!running)void w.history();}} onError={e=>w.setError(hostError(e))}/>
   <ApprovalPanel key={`approval:${key}`} requests={run?.pendingUi??[]} busy={w.busy} onRespond={(id,value,confirmed,cancelled)=>void w.act(()=>host.respond(task.id,id,value,confirmed,cancelled))}/>
   {w.sendFailed&&<div role="alert" className="px-4"><span>消息未发送</span><Button variant="ghost" disabled={w.busy} onClick={()=>void w.send(task.id,true)}>重试发送</Button></div>}
   <Composer key={`composer:${task.id}`} taskId={task.id} roots={task.roots} references={w.references[task.id]??[]} onReferences={refs=>w.setReferences(old=>({...old,[task.id]:refs}))} models={capabilities?.models?.length?capabilities.models:w.models} onModel={(p,id)=>void w.act(()=>records.model(task.id,p,id))} draft={w.drafts[task.id]??''} onFiles={()=>setDetails(true)} onContinue={()=>void w.act(()=>records.resume(task.id))} onSettings={()=>setSettings(true)} modelLabel={capabilities?.state?.model?.id??task.model?.split('/').at(-1)} attachmentIds={w.attachments[task.id]??[]} onRemoveAttachment={id=>w.removeAttachment(task.id,id)} onDraft={v=>w.setDraft(task.id,v)} onSend={()=>void w.send(task.id)} onAbort={()=>void w.act(()=>host.abort(task.id))} canSend={!task.archived&&!w.sendFailed&&run?.status!=='starting'} running={running} busy={w.busy} disabledReason={task.archived?'任务已归档，请先恢复任务。':undefined}/>
  </>:<><div className="workspace-empty"><FolderOpen size={32}/><h2>{w.project?`你想在 ${w.project.name} 中构建什么？`:'今天，想一起完成什么？'}</h2>{w.project?<div className="starter-actions">{['探索并理解代码','构建新功能','审查代码','修复问题'].map(text=><Button key={text} variant="outline" disabled={creating} onClick={()=>w.setDraft(`project:${w.projectId}`,text)}>{text}</Button>)}</div>:<Button disabled={w.busy} onClick={()=>openName({kind:'project',initial:''})}><Plus/>添加项目</Button>}</div>{firstMessage&&<div className="user-bubble mx-6" role="status">{firstMessage}</div>}{w.project&&<Composer key={`project:${w.projectId}`} taskId={`project:${w.projectId}`} roots={w.project.roots} references={w.references[`project:${w.projectId}`]??[]} onReferences={refs=>w.setReferences(old=>({...old,[`project:${w.projectId}`]:refs}))} draft={w.drafts[`project:${w.projectId}`]??''} onDraft={text=>w.setDraft(`project:${w.projectId}`,text)} onSend={()=>void sendFirst()} onAbort={()=>{}} onSettings={()=>setSettings(true)} canSend={!w.project.archived} busy={creating} running={false}/>}</>}
 </main>;
 return <TooltipProvider><div className="workbench">{narrow?content:<AppFrame sidebar={side} collapsed={!sidebar} onExpand={()=>setSidebar(true)} onNew={newConversation} onSearch={()=>{setSidebar(true);requestAnimationFrame(()=>document.querySelector<HTMLInputElement>('[aria-label="搜索任务"]')?.focus());}} onSettings={()=>{remember();setSettings(true);}} inspector={inspector} inspectorOpen={details} onCloseInspector={()=>setDetails(false)}>{content}</AppFrame>}</div>
  {narrow&&<Sheet open={drawer} onOpenChange={setDrawer}><SheetContent side="left" showCloseButton={false} className="w-[min(320px,90vw)] gap-0 p-0"><SheetTitle className="sr-only">项目与任务</SheetTitle><SheetDescription className="sr-only">选择项目和任务</SheetDescription>{side}</SheetContent></Sheet>}
  {narrow&&inspector&&<Sheet open={details} onOpenChange={setDetails}><SheetContent side="right" className="w-[min(420px,94vw)] overflow-y-auto p-0 pt-8"><SheetTitle className="sr-only">工具、用量与 Git 变更</SheetTitle><SheetDescription className="sr-only">当前任务的工具执行、上下文用量和只读 Git 变更</SheetDescription>{inspector}</SheetContent></Sheet>}
  {dialog?.kind==='project'?<LocalProjectDialog onClose={()=>setDialog(null)} onCreated={async p=>{await w.refresh();w.chooseProject(p.id);}}/>:dialog&&<NameDialog {...dialog} onClose={()=>setDialog(null)} restoreFocus={restore} onSubmit={async(name,trusted,mode)=>{if(dialog.kind==='task'){const t=await records.create(w.projectId,name,mode);await w.refresh();w.chooseTask(t);setDrawer(false);}else if(dialog.task){await records.update({...dialog.task,title:name});await w.refresh();}else if(dialog.project&&w.project){await projects.update(w.project.id,name,w.project.archived);await w.refresh();}}}/>}
  <RuntimeSettings onSaved={w.refresh} open={settings} onClose={()=>setSettings(false)} theme={theme.theme} onTheme={theme.change} themeError={theme.error} restoreFocus={restore}/>
  {recovery&&<RecoveryDialog task={recovery==='task'?task:undefined} roots={(recovery==='task'?task?.roots:w.project?.roots)??[]} onClose={()=>setRecovery(null)} onChanged={w.refresh} restoreFocus={restore}/>}
 </TooltipProvider>;
}
import './design-tokens.css';
import './components/workbench/app-frame.css';
