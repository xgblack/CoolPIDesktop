import {ChevronLeft,ChevronRight,PanelRight,Menu,MoreHorizontal,Square,RotateCcw,FolderOpen} from 'lucide-react';
import {Button} from '@/components/ui/button';
import {DropdownMenu,DropdownMenuTrigger,DropdownMenuContent,DropdownMenuItem} from '@/components/ui/dropdown-menu';
import {IconButton} from './shared';
import type {TaskRecord,TaskSnapshot} from '../../../../../packages/host-contract/src';
export type Capabilities={models?:{id:string;provider:string;name?:string}[];state?:{model?:{id:string;provider:string}}};
export function TaskHeader({task,run,busy,onSidebar,onStop,onRestart,onAbort,onRecovery,sidebarVisible,onDetails,detailsOpen,onBack,onForward,onTrajectory,trajectoryOpen}:{task:TaskRecord;run?:TaskSnapshot;busy:boolean;onSidebar:()=>void;onStop:()=>void;onRestart:()=>void;onAbort:()=>void;onRecovery:()=>void;sidebarVisible:boolean;onDetails?:()=>void;detailsOpen?:boolean;onBack?:()=>void;onForward?:()=>void;onTrajectory?:()=>void;trajectoryOpen?:boolean}){
 const active=!!run&&['ready','idle','running','interrupted','starting'].includes(run.status);
 return <header className="task-header">
  <div className="task-header-title"><div className="task-navigation"><IconButton label="上一个任务" disabled={!onBack} onClick={onBack}><ChevronLeft/></IconButton><IconButton label="下一个任务" disabled={!onForward} onClick={onForward}><ChevronRight/></IconButton></div>{!sidebarVisible&&<IconButton label="打开侧栏" onClick={onSidebar}><Menu/></IconButton>}<h1 title={task.title}>{task.title}</h1></div>
  <div className="task-header-actions">
  {onTrajectory?<Button variant="ghost" size="sm" onClick={onTrajectory} aria-pressed={trajectoryOpen}>{trajectoryOpen?'对话':'轨迹'}</Button>:null}
  {run?.status==='running'?<Button variant="outline" size="sm" disabled={busy} onClick={onAbort}><Square/>取消</Button>:null}
  <IconButton label="工具、用量与变更" onClick={onDetails} aria-pressed={detailsOpen}><PanelRight/></IconButton><DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" size="icon-sm" aria-label="任务运行操作"><MoreHorizontal/></Button></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuItem onSelect={onRecovery}><FolderOpen/>目录与会话恢复</DropdownMenuItem><DropdownMenuItem disabled={busy||!active} onSelect={onStop}><Square/>停止进程</DropdownMenuItem><DropdownMenuItem disabled={busy||!run||task.archived} onSelect={onRestart}><RotateCcw/>重启并继续原会话</DropdownMenuItem></DropdownMenuContent></DropdownMenu>
  </div>
 </header>;
}
