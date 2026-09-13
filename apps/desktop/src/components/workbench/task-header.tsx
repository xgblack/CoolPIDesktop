import {PanelRight,Menu,MoreHorizontal,Square,RotateCcw,FolderOpen,Copy,Download} from 'lucide-react';
import {Button} from '@/components/ui/button';
import {DropdownMenu,DropdownMenuTrigger,DropdownMenuContent,DropdownMenuItem,DropdownMenuSeparator} from '@/components/ui/dropdown-menu';
import {IconButton} from './shared';
import type {TaskRecord,TaskSnapshot} from '../../../../../packages/host-contract/src';
export type Capabilities={models?:{id:string;provider:string;name?:string}[];state?:{model?:{id:string;provider:string}}};
export function TaskHeader({task,run,busy,onSidebar,onStop,onRestart,onAbort,onRecovery,sidebarVisible,onDetails,detailsOpen,onTrajectory,trajectoryOpen,onRuntime,historyControls,onCopySessionId,onDownloadSession,exporting=false}:{task:TaskRecord;run?:TaskSnapshot;busy:boolean;onSidebar:()=>void;onStop:()=>void;onRestart:()=>void;onAbort:()=>void;onRecovery:()=>void;sidebarVisible:boolean;onDetails?:()=>void;detailsOpen?:boolean;onTrajectory?:()=>void;trajectoryOpen?:boolean;onRuntime?:()=>void;onCopySessionId:()=>void;onDownloadSession:()=>void;exporting?:boolean;historyControls?:import('react').ReactNode}){
 const active=!!run&&['ready','idle','running','interrupted','starting'].includes(run.status);
 return <header className="task-header">
  <div className="task-header-title">{!sidebarVisible&&<IconButton label="打开侧栏" onClick={onSidebar}><Menu/></IconButton>}<h1 title={task.title}>{task.title}</h1></div>
  <div className="task-header-actions">
  {onTrajectory?<Button variant="ghost" size="sm" onClick={onTrajectory} aria-pressed={trajectoryOpen}>{trajectoryOpen?'对话':'轨迹'}</Button>:null}
  {historyControls}
  {run?.status==='running'?<Button variant="outline" size="sm" disabled={busy} onClick={onAbort}><Square/>取消</Button>:null}
  <IconButton label="工具、用量与变更" onClick={onDetails} aria-pressed={detailsOpen}><PanelRight/></IconButton><DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" size="icon-sm" aria-label="任务运行操作"><MoreHorizontal/></Button></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuItem disabled={!task.sessionId} onSelect={onCopySessionId}><Copy/>复制会话 ID</DropdownMenuItem><DropdownMenuItem disabled={exporting||!task.sessionId||!task.sessionFile||!!run&&['running','starting'].includes(run.status)||!!run?.pendingUi?.length} onSelect={onDownloadSession}><Download/>{exporting?'正在下载会话…':'下载会话 Session'}</DropdownMenuItem><DropdownMenuSeparator/><DropdownMenuItem onSelect={onRecovery}><FolderOpen/>目录与会话恢复</DropdownMenuItem>{onRuntime?<DropdownMenuItem onSelect={onRuntime}><RotateCcw/>当前任务运行</DropdownMenuItem>:<><DropdownMenuItem disabled={busy||!active} onSelect={onStop}><Square/>停止进程</DropdownMenuItem><DropdownMenuItem disabled={busy||!run||task.archived} onSelect={onRestart}><RotateCcw/>重启并继续原会话</DropdownMenuItem></>}</DropdownMenuContent></DropdownMenu>
  </div>
 </header>;
}
