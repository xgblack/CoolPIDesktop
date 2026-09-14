import {invoke} from '@tauri-apps/api/core';import type {HostError,TaskSnapshot,RuntimeInfo,ObserverInfo} from '../../../packages/host-contract/src';
import type {Project,TaskRecord,HistoryPage,GitStatus,GitDiff,TaskRoot} from '../../../packages/host-contract/src';
import type {CommitPreview} from '../../../packages/host-contract/src';
export interface FileReference {rootIndex:number;path:string;name:string}
export const localProjects={edit:(id:string,name:string,folders:({existing:number}|{selected:string})[],trusted:boolean)=>invoke<Project>('edit_local_project',{id,name,folders,trusted}),choose:()=>invoke<{token:string;path:string}[]>('choose_project_folders'),create:(name:string,tokens:string[],trusted:boolean)=>invoke<Project>('create_local_project',{name,tokens,trusted})};
export const sendPrompt=(taskId:string,message:string,attachmentIds:string[],references:FileReference[])=>invoke<TaskSnapshot>('prompt_task',{taskId,message,attachmentIds,references});
export const searchFiles=(taskId:string,rootIndex:number,query:string)=>invoke<DirectoryPage>('search_task_files',{taskId,rootIndex,query});
export const gitWrites={change:(taskId:string,rootIndex:number,path:string,action:'stage'|'unstage'|'discard',confirmed=false)=>invoke<void>('task_git_change',{taskId,rootIndex,path,action,confirmed}),preview:(taskId:string,rootIndex:number)=>invoke<CommitPreview>('task_git_commit_preview',{taskId,rootIndex}),commit:(taskId:string,rootIndex:number,message:string,expected:CommitPreview)=>invoke<string>('task_git_commit',{taskId,rootIndex,message,expected})};
export const projects={model:(projectId:string,provider:string,modelId:string)=>invoke<void>("select_project_model",{projectId,provider,modelId}),list:()=>invoke<Project[]>('list_projects'),register:(name:string,trusted:boolean)=>invoke<Project|null>('register_project',{name,trusted}),update:(id:string,name:string,archived:boolean)=>invoke<void>('update_project',{id,name,archived})};
export const records={fork:(taskId:string,timestamp:number)=>invoke<TaskRecord>('fork_task',{taskId,timestamp}),approval:(taskId:string,mode:import('../../../packages/host-contract/src').ApprovalMode|null)=>invoke<TaskRecord>('set_task_approval',{taskId,mode}),list:()=>invoke<TaskRecord[]>('task_records'),create:(projectId:string,title:string,mode:'shared'|'isolated'='shared',model?:string,autoTitle=false)=>invoke<TaskRecord>('create_task',{projectId,title,mode,model,autoTitle}),update:(task:TaskRecord)=>invoke<TaskRecord>('update_task',{id:task.id,title:task.title,pinned:task.pinned,archived:task.archived}),relocate:(taskId:string,trusted:boolean)=>invoke<TaskRecord|null>('relocate_task',{taskId,trusted}),resume:(taskId:string)=>invoke<TaskSnapshot>('continue_task',{taskId}),history:(taskId:string,cursor:string|null)=>invoke<HistoryPage>('task_history',{taskId,cursor}),usage:(taskId:string)=>invoke<TaskSnapshot>('task_usage',{taskId}),gitStatus:(taskId:string,rootIndex:number)=>invoke<GitStatus>('task_git_status',{taskId,rootIndex}),gitDiff:(taskId:string,rootIndex:number,path:string,staged:boolean,untracked:boolean)=>invoke<GitDiff>('task_git_diff',{taskId,rootIndex,path,staged,untracked}),model:(taskId:string,provider:string,modelId:string)=>invoke<Record<string,unknown>>('select_task_model',{taskId,provider,modelId}),thinking:(taskId:string)=>invoke<string|null>('task_thinking',{taskId}),setThinking:(taskId:string,level:string|null)=>invoke<void>('set_task_thinking',{taskId,level})};
export const host={runtime:(explicit:string)=>invoke<RuntimeInfo>('runtime_status',{explicit:explicit.trim()||null}),list:()=>invoke<TaskSnapshot[]>('list_tasks'),snapshot:(taskId:string)=>invoke<TaskSnapshot>('task_snapshot',{taskId}),prompt:(taskId:string,message:string,attachmentIds:string[]=[])=>invoke<TaskSnapshot>('prompt_task',{taskId,message,attachmentIds}),abort:(taskId:string)=>invoke<TaskSnapshot>('abort_task',{taskId}),stop:(taskId:string)=>invoke<TaskSnapshot>('stop_task',{taskId}),restart:(taskId:string)=>invoke<TaskSnapshot>('restart_task',{taskId}),respond:(taskId:string,requestId:string,value:string|null,confirmed:boolean|null,cancelled:boolean)=>invoke<TaskSnapshot>('respond_ui',{taskId,requestId,value,confirmed,cancelled}),roots:(taskId:string)=>invoke<TaskRoot[]>('task_roots',{taskId}),isolate:(taskId:string)=>invoke<TaskRoot[]>('isolate_task',{taskId}),cleanupWorktrees:(taskId:string)=>invoke<TaskRoot[]>('cleanup_worktrees',{taskId}),recoverWorktrees:(taskId:string)=>invoke<TaskRoot[]>('recover_worktrees',{taskId}),observer:()=>invoke<ObserverInfo>('observer_info')};
export function hostError(e:unknown):HostError{if(typeof e==='object'&&e&&'message'in e){const v=e as any;return{code:v.code||'host_error',message:String(v.message),suggestion:v.suggestion}}return{code:'host_unavailable',message:String(e||'无法连接桌面 Host'),suggestion:'请检查 Host 进程。'}}
export const recoverSession=(taskId:string,confirmed:boolean)=>invoke<string>('recover_session',{taskId,confirmed});
import type {ModelConfig,ModelEdit,ModelCatalog,ModelVerification} from '../../../packages/host-contract/src';
export const modelConfig={apply:()=>invoke<{applied:string[];skipped:string[];errors:{taskId:string;error:HostError}[]}>('model_config_apply'),load:()=>invoke<ModelConfig>('model_config_load'),save:(edit:ModelEdit)=>invoke<ModelConfig>('model_config_save',{edit}),catalog:()=>invoke<ModelCatalog>('model_catalog'),verify:(provider:string|null=null,modelId:string|null=null,projectId:string|null=null)=>invoke<ModelVerification>('model_config_verify',{provider,modelId,projectId})};

import type {DirectoryPage,FilePreview,Attachment,TrajectoryPage,TrajectoryImageData} from '../../../packages/host-contract/src';
export const trajectory={read:(taskId:string,cursor:string|null=null,after=false)=>invoke<TrajectoryPage>('task_trajectory',{taskId,cursor,after}),image:(taskId:string,recordId:string,imageId:string)=>invoke<TrajectoryImageData>('task_trajectory_image',{taskId,recordId,imageId})};
export const resources={
 list:(taskId:string,rootIndex:number,path:string)=>invoke<DirectoryPage>('list_task_files',{taskId,rootIndex,path}),
 preview:(taskId:string,rootIndex:number,path:string)=>invoke<FilePreview>('preview_task_file',{taskId,rootIndex,path}),
 attachments:(taskId:string)=>invoke<Attachment[]>('task_attachments',{taskId}),
 import:(taskId:string)=>invoke<Attachment|null>('import_task_attachment',{taskId}),
 previewAttachment:(taskId:string,resourceId:string)=>invoke<FilePreview>('preview_task_attachment',{taskId,resourceId}),
};
import type {TerminalSnapshot} from '../../../packages/host-contract/src';
export const terminal={create:(taskId:string,rootIndex:number)=>invoke<TerminalSnapshot>('terminal_create',{taskId,rootIndex}),snapshot:(taskId:string,terminalId:string,after=0)=>invoke<TerminalSnapshot>('terminal_snapshot',{taskId,terminalId,after}),write:(taskId:string,terminalId:string,input:string)=>invoke<void>('terminal_write',{taskId,terminalId,input}),resize:(taskId:string,terminalId:string,cols:number,rows:number)=>invoke<void>('terminal_resize',{taskId,terminalId,cols,rows}),close:(taskId:string,terminalId:string)=>invoke<void>('terminal_close',{taskId,terminalId})};

import type {RuntimeTaskInfo,RuntimeCommand} from '../../../packages/host-contract/src';
export const taskRuntime={
 list:()=>invoke<RuntimeTaskInfo[]>('runtime_tasks'),
 focus:(taskId:string|null)=>invoke<void>('runtime_focus',{taskId}),
 keepAlive:(taskId:string,keepAlive:boolean)=>invoke<void>('runtime_keep_alive',{taskId,keepAlive}),
 action:(taskId:string,action:'start'|'stop'|'restart'|'cancel',expectedRunId:string|null)=>invoke<RuntimeTaskInfo>('runtime_action',{taskId,action,expectedRunId}),
 releaseIdle:()=>invoke<string[]>('runtime_release_idle'),
 command:(taskId:string)=>invoke<RuntimeCommand>('runtime_command',{taskId}),
};

export const downloadSession=(taskId:string)=>invoke<string|null>('download_task_session',{taskId});

export const openInApp={
 list:(refresh=false)=>invoke<import('../../../packages/host-contract/src').OpenApp[]>('open_in_app_apps',{refresh}),
 icon:(appId:string)=>invoke<string>('open_in_app_icon',{appId}),
 open:(taskId:string,appId:string)=>invoke<void>('open_in_app',{taskId,appId}),
};
