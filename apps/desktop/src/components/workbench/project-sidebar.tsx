import {useState} from 'react';
import {FolderOpen,Plus,Settings2,MoreHorizontal,Pin,PanelLeftClose,ChevronDown,Activity} from 'lucide-react';
import type {Project,TaskRecord,TaskSnapshot,RuntimeTaskInfo} from '../../../../../packages/host-contract/src';
import {runtimeLabel} from './use-task-runtime';
import {Button} from '@/components/ui/button';
import {DropdownMenu,DropdownMenuTrigger,DropdownMenuContent,DropdownMenuItem,DropdownMenuSeparator,DropdownMenuCheckboxItem} from '@/components/ui/dropdown-menu';
import {IconButton} from './shared';

export function ProjectSidebar({projects,tasks,runs,projectId,taskId,loading,busy,showArchived,onArchived,onProject,onTask,onNewProject,onNewTask,onRenameProject,onArchiveProject,onRenameTask,onUpdateTask,onSettings,onClose,onPaths,runtimeTasks=[],onRuntime,onRuntimeManagement}: {
 projects:Project[];tasks:TaskRecord[];runs:TaskSnapshot[];projectId:string;taskId:string;loading:boolean;busy:boolean;showArchived:boolean;onArchived:(v:boolean)=>void;onProject:(id:string)=>void;onTask:(t:TaskRecord)=>void;onNewProject:()=>void;onNewTask:()=>void;onRenameProject:()=>void;onArchiveProject:()=>void;onRenameTask:(t:TaskRecord)=>void;onUpdateTask:(t:TaskRecord)=>void;onSettings:()=>void;onClose:()=>void;onPaths:()=>void;
 onRuntimeManagement:()=>void;runtimeTasks?:RuntimeTaskInfo[];onRuntime?:(task:TaskRecord)=>void;
}){
 const [query,setQuery]=useState('');
 const [collapsed,setCollapsed]=useState<Record<string,boolean>>({});
 const project=projects.find(p=>p.id===projectId);
 const visible=tasks.filter(t=>t.projectId===projectId&&(showArchived||!t.archived)&&t.title.toLocaleLowerCase().includes(query.toLocaleLowerCase()));
 return <aside className="project-sidebar" aria-label="项目与任务">
  <div className="sidebar-brand"><span className="brand-mark">π</span><strong>Cool PI</strong><IconButton label="收起侧栏" onClick={onClose}><PanelLeftClose/></IconButton></div>
  <div className="sidebar-primary"><Button variant="ghost" onClick={onNewTask} disabled={!project||project.archived||busy}><Plus/>新对话<span>⌘ N</span></Button><input aria-label="搜索任务" placeholder="搜索任务…" value={query} onChange={e=>setQuery(e.target.value)}/></div>
  <div className="task-list-heading"><span>项目</span><Button variant="ghost" size="icon-sm" aria-label="添加项目" onClick={onNewProject}><Plus/></Button><DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" size="icon-sm" aria-label="项目列表选项"><MoreHorizontal/></Button></DropdownMenuTrigger><DropdownMenuContent><DropdownMenuCheckboxItem checked={showArchived} onCheckedChange={onArchived}>显示归档</DropdownMenuCheckboxItem></DropdownMenuContent></DropdownMenu></div>
  <nav className="task-list" aria-label="任务列表">
   {loading&&<p role="status">正在加载项目…</p>}
   {!loading&&!projects.length&&<div className="sidebar-empty"><p>添加本地文件夹，开始第一段对话。</p><Button onClick={onNewProject}>添加项目</Button></div>}
   {projects.filter(p=>showArchived||!p.archived).map(p=>{const children=tasks.filter(t=>t.projectId===p.id&&(showArchived||!t.archived)&&t.title.toLocaleLowerCase().includes(query.toLocaleLowerCase())).sort((a,b)=>Number(b.pinned)-Number(a.pinned));const expanded=!collapsed[p.id]||!!query;return <section key={p.id} className="sidebar-project">
    <div className={'sidebar-project-title '+(projectId===p.id&&!taskId?'active':'')}><button aria-label={`${expanded?'收起':'展开'} ${p.name}`} aria-expanded={expanded} onClick={()=>setCollapsed(old=>({...old,[p.id]:expanded}))}><ChevronDown size={13} style={{transform:expanded?'none':'rotate(-90deg)'}}/></button><button onClick={()=>onProject(p.id)}><FolderOpen size={16}/><span>{p.name}</span></button>{projectId===p.id&&<DropdownMenu><DropdownMenuTrigger asChild><Button aria-label="项目操作" variant="ghost" size="icon-xs"><MoreHorizontal/></Button></DropdownMenuTrigger><DropdownMenuContent><DropdownMenuItem onSelect={onPaths}>查看项目目录</DropdownMenuItem><DropdownMenuItem onSelect={onNewTask} disabled={p.archived}>新对话</DropdownMenuItem><DropdownMenuItem onSelect={onRenameProject}>编辑项目</DropdownMenuItem><DropdownMenuItem onSelect={onArchiveProject}>{p.archived?'恢复项目':'归档项目'}</DropdownMenuItem></DropdownMenuContent></DropdownMenu>}</div>
{expanded&&<div className="project-conversations">{!children.length&&<p className="empty-conversations">{query?'没有匹配的会话':'暂无聊天'}</p>}{children.map(t=>{const info=runtimeTasks.find(r=>r.taskId===t.id),pending=!!runs.find(r=>r.taskId===t.id)?.pendingUi?.length;return <div key={t.id} className={'task-row '+(t.id===taskId?'active':'')}><button className="task-select" onClick={()=>onTask(t)} aria-current={t.id===taskId?'page':undefined}><span className="task-name">{t.pinned&&<Pin size={12}/>}<span className="truncate">{t.title}</span></span></button><button className={`task-runtime-state runtime-${info?.status??'stopped'}`} aria-label={`${t.title}运行详情`} onClick={()=>onRuntime?.(t)}><span className="runtime-dot"/>{runtimeLabel(info,pending)}</button><DropdownMenu><DropdownMenuTrigger asChild><Button aria-label={`${t.title}的操作`} variant="ghost" size="icon-xs" className="task-menu"><MoreHorizontal/></Button></DropdownMenuTrigger><DropdownMenuContent><DropdownMenuItem onSelect={()=>onRenameTask(t)} disabled={busy}>重命名</DropdownMenuItem><DropdownMenuItem onSelect={()=>onUpdateTask({...t,pinned:!t.pinned})} disabled={busy}>{t.pinned?'取消置顶':'置顶'}</DropdownMenuItem><DropdownMenuItem onSelect={()=>onUpdateTask({...t,archived:!t.archived})} disabled={busy}>{t.archived?'恢复任务':'归档任务'}</DropdownMenuItem></DropdownMenuContent></DropdownMenu></div>;})}</div>}
   </section>;})}
  </nav>
  <footer className="sidebar-footer"><Button variant="ghost" className="w-full justify-start" onClick={onRuntimeManagement}><Activity/>运行管理<span className="ml-auto text-xs text-muted-foreground">{runtimeTasks.filter(t=>t.pid).length} 个进程</span></Button><Button variant="ghost" className="w-full justify-start" onClick={onSettings}><Settings2/>设置<span className="ml-auto text-xs text-muted-foreground">本机 OMP</span></Button></footer>
 </aside>;
}
