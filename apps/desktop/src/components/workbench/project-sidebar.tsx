import {FolderOpen,Plus,Settings2,MoreHorizontal,Pin,Archive,PanelLeftClose,ChevronDown,MessageSquare} from 'lucide-react';
import type {Project,TaskRecord,TaskSnapshot} from '../../../../../packages/host-contract/src';
import {Button} from '@/components/ui/button';
import {DropdownMenu,DropdownMenuTrigger,DropdownMenuContent,DropdownMenuItem,DropdownMenuSeparator,DropdownMenuCheckboxItem} from '@/components/ui/dropdown-menu';
import {IconButton,Status} from './shared';

export function ProjectSidebar({projects,tasks,runs,projectId,taskId,loading,busy,showArchived,onArchived,onProject,onTask,onNewProject,onNewTask,onRenameProject,onArchiveProject,onRenameTask,onUpdateTask,onSettings,onClose,onPaths}: {
 projects:Project[];tasks:TaskRecord[];runs:TaskSnapshot[];projectId:string;taskId:string;loading:boolean;busy:boolean;showArchived:boolean;onArchived:(v:boolean)=>void;onProject:(id:string)=>void;onTask:(t:TaskRecord)=>void;onNewProject:()=>void;onNewTask:()=>void;onRenameProject:()=>void;onArchiveProject:()=>void;onRenameTask:(t:TaskRecord)=>void;onUpdateTask:(t:TaskRecord)=>void;onSettings:()=>void;onClose:()=>void;onPaths:()=>void;
}){
 const project=projects.find(p=>p.id===projectId);
 const visible=tasks.filter(t=>t.projectId===projectId&&(showArchived||!t.archived));
 return <aside className="project-sidebar" aria-label="项目与任务">
  <div className="sidebar-brand"><span className="brand-mark">π</span><strong>Cool PI</strong><span className="brand-caption">工作台</span><IconButton label="收起侧栏" onClick={onClose}><PanelLeftClose/></IconButton></div>
  <div className="project-picker"><DropdownMenu><DropdownMenuTrigger asChild><Button variant="outline" className="w-full justify-start min-w-0"><FolderOpen/><span className="truncate flex-1 text-left">{project?.name??'选择项目'}</span><ChevronDown/></Button></DropdownMenuTrigger><DropdownMenuContent className="w-64" align="start">
   {projects.filter(p=>showArchived||!p.archived).map(p=><DropdownMenuItem key={p.id} onSelect={()=>onProject(p.id)}><FolderOpen/><span className="truncate">{p.name}{p.archived?' · 已归档':''}</span></DropdownMenuItem>)}
   <DropdownMenuSeparator/><DropdownMenuItem onSelect={onNewProject}><Plus/>添加项目</DropdownMenuItem>
   <DropdownMenuCheckboxItem checked={showArchived} onCheckedChange={onArchived}>显示归档</DropdownMenuCheckboxItem>
  </DropdownMenuContent></DropdownMenu></div>
  {project&&<div className="project-caption"><button className="truncate text-left hover:text-foreground" onClick={onPaths} title={project.roots[0]}>{project.roots[0].split('/').filter(Boolean).at(-1)}{project.roots.length>1?` +${project.roots.length-1}`:''}</button><DropdownMenu><DropdownMenuTrigger asChild><Button aria-label="项目操作" variant="ghost" size="icon-xs"><MoreHorizontal/></Button></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuItem onSelect={onPaths}>查看项目目录</DropdownMenuItem><DropdownMenuItem disabled={busy} onSelect={onRenameProject}>重命名项目</DropdownMenuItem><DropdownMenuSeparator/><DropdownMenuItem disabled={busy} onSelect={onArchiveProject}>{project.archived?'恢复项目':'归档项目'}</DropdownMenuItem></DropdownMenuContent></DropdownMenu></div>}
  <div className="task-list-heading"><span>任务</span><span className="text-xs text-muted-foreground">{runs.filter(r=>r.status==='running').length} 个运行中</span><IconButton label="创建任务" disabled={!project||project.archived||busy} onClick={onNewTask}><Plus/></IconButton></div>
  <nav className="task-list" aria-label="任务列表">
   {loading?<div aria-label="正在加载项目" className="space-y-3 p-3">{[0,1,2,3].map(i=><div key={i} className="h-8 rounded bg-muted animate-pulse"/>)}</div>:!project?<div className="sidebar-empty"><FolderOpen/><p>把工作目录添加为项目，开始第一段对话。</p><Button size="sm" variant="outline" onClick={onNewProject}>添加项目</Button></div>:visible.length===0?<div className="sidebar-empty"><MessageSquare/><p>这个项目还没有任务</p><Button size="sm" variant="outline" disabled={project.archived} onClick={onNewTask}>创建任务</Button></div>:visible.sort((a,b)=>Number(b.pinned)-Number(a.pinned)).map(t=>{
    const status=runs.find(r=>r.taskId===t.id)?.status??t.lastRun?.state;
    return <div key={t.id} className={'task-row '+(t.id===taskId?'active':'')}><button className="task-select" onClick={()=>onTask(t)} aria-current={t.id===taskId?'page':undefined}><span className="task-name">{t.pinned&&<Pin size={12}/>}<span className="truncate">{t.title}</span>{t.archived&&<Archive size={12}/>}</span><Status status={status}/></button><DropdownMenu><DropdownMenuTrigger asChild><Button aria-label={`${t.title}的操作`} variant="ghost" size="icon-xs" className="task-menu"><MoreHorizontal/></Button></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuItem disabled={busy} onSelect={()=>onRenameTask(t)}>重命名</DropdownMenuItem><DropdownMenuItem disabled={busy} onSelect={()=>onUpdateTask({...t,pinned:!t.pinned})}>{t.pinned?'取消置顶':'置顶'}</DropdownMenuItem><DropdownMenuSeparator/><DropdownMenuItem disabled={busy} onSelect={()=>onUpdateTask({...t,archived:!t.archived})}>{t.archived?'恢复任务':'归档任务'}</DropdownMenuItem></DropdownMenuContent></DropdownMenu></div>;
   })}
  </nav>
  <footer className="sidebar-footer"><Button variant="ghost" className="w-full justify-start" onClick={onSettings}><Settings2/>设置<span className="ml-auto text-xs text-muted-foreground">本机 OMP</span></Button></footer>
 </aside>;
}
