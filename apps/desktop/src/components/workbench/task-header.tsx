import {ChevronLeft,ChevronRight,PanelRight,Menu,MoreHorizontal,Square,RotateCcw,FolderOpen,GitBranch,Users} from 'lucide-react';
import {useEffect,useState} from 'react';
import {host} from '@/host';
import {Button} from '@/components/ui/button';
import {DropdownMenu,DropdownMenuTrigger,DropdownMenuContent,DropdownMenuItem} from '@/components/ui/dropdown-menu';
import {IconButton,Status} from './shared';
import type {TaskRecord,TaskRoot,TaskSnapshot} from '../../../../../packages/host-contract/src';
export type Capabilities={models?:{id:string;provider:string;name?:string}[];state?:{model?:{id:string;provider:string}}};
export function TaskHeader({task,run,busy,onSidebar,onContinue,onStop,onRestart,onAbort,onModel,onRecovery,sidebarVisible,projectName,onDetails,detailsOpen,onBack,onForward}:{task:TaskRecord;run?:TaskSnapshot;busy:boolean;onSidebar:()=>void;onContinue:()=>void;onStop:()=>void;onRestart:()=>void;onAbort:()=>void;onModel:(provider:string,id:string)=>void;onRecovery:()=>void;sidebarVisible:boolean;projectName?:string;onDetails?:()=>void;detailsOpen?:boolean;onBack?:()=>void;onForward?:()=>void}){
 const [roots,setRoots]=useState<TaskRoot[]>([]);
 useEffect(()=>{let active=true;void host.roots(task.id).then(value=>{if(active)setRoots(value);}).catch(()=>{if(active)setRoots([]);});return()=>{active=false;}},[task.id]);
 const status=run?.status??task.lastRun?.state;
 const active=!!run&&['ready','idle','running','interrupted','starting'].includes(run.status);
 return <header className="task-header">
  <div className="task-header-title"><div className="task-navigation"><IconButton label="上一个任务" disabled={!onBack} onClick={onBack}><ChevronLeft/></IconButton><IconButton label="下一个任务" disabled={!onForward} onClick={onForward}><ChevronRight/></IconButton></div>{!sidebarVisible&&<IconButton label="打开侧栏" onClick={onSidebar}><Menu/></IconButton>}<div className="task-header-copy"><div className="header-project" title={projectName}>{projectName||'本地项目'}<span aria-hidden="true">/</span></div><h1 title={task.title}>{task.title}</h1><div className="task-header-meta"><Status status={status}/>{roots.length>0&&(roots[0].mode==='isolated'?<span className="workspace-mode"><GitBranch size={12}/>隔离工作区</span>:<span className="workspace-mode"><Users size={12}/>共享工作区</span>)}</div></div></div>
  <div className="task-header-actions">
  {run?.status==='running'?<Button variant="outline" size="sm" disabled={busy} onClick={onAbort}><Square/>取消</Button>:null}
  <IconButton label="工具、用量与变更" onClick={onDetails} aria-pressed={detailsOpen}><PanelRight/></IconButton><DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" size="icon-sm" aria-label="任务运行操作"><MoreHorizontal/></Button></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuItem onSelect={onRecovery}><FolderOpen/>目录与会话恢复</DropdownMenuItem><DropdownMenuItem disabled={busy||!active} onSelect={onStop}><Square/>停止进程</DropdownMenuItem><DropdownMenuItem disabled={busy||!run||task.archived} onSelect={onRestart}><RotateCcw/>重启并继续原会话</DropdownMenuItem></DropdownMenuContent></DropdownMenu>
  </div>
 </header>;
}
