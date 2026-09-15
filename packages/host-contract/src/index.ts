export type RuntimeStatus='not_found'|'not_executable'|'version_unreadable'|'version_unsupported'|'starting'|'handshake_failed'|'capability_query_failed'|'partially_available'|'ready'|'process_exited'|'model_required';
export interface HostError{code:string;message:string;suggestion?:string}
export interface RuntimeInfo{status:RuntimeStatus;executable?:string|null;version?:string|null;protocol?:number|null;capabilities?:unknown;detail?:string|null;error?:HostError|null}
export type TaskStatus='starting'|'ready'|'running'|'compacting'|'retrying'|'pending_ui'|'recovering'|'idle'|'failed'|'interrupted'|'stopping'|'stopped';
export interface HostEvent{taskId:string;runId:string;seq:number;eventType:string;payload:unknown;trajectory?:TrajectoryRecord[]}
export interface ToolActivity{id:string;name:string;status:'running'|'succeeded'|'failed'|'cancelled'|string;args:unknown;result?:unknown;seq:number}
export interface UsageSummary{contextTokens?:number;contextWindow?:number;contextPercent?:number;inputTokens?:number;outputTokens?:number;reasoningTokens?:number;cacheReadTokens?:number;cacheWriteTokens?:number;totalTokens?:number;cost?:number}
export interface PendingUiRequest{id:string;method:'confirm'|'select'|'input'|'editor';title?:string;message?:string;options?:string[];optionDetails?:{description?:string}[];placeholder?:string;prefill?:string;promptStyle?:boolean;timeout?:number}
export interface ExtensionUiState {statuses:Record<string,string>;widgets:Record<string,{widgetLines?:string[];widgetPlacement?:'aboveEditor'|'belowEditor'}>;title?:string|null;editorText?:string|null;openUrl?:{url:string;launchUrl?:string;instructions?:string}|null}
export interface AvailableCommand {name:string;aliases?:string[];description?:string;input?:{hint?:string};subcommands?:{name:string;description?:string;usage?:string}[];source:string}
export interface QueuedMessage {id:string;kind:'steer'|'follow_up'|'prompt'|string;message:string}
export interface SubagentSnapshot {id:string;index?:number;agent?:string;agentSource?:string;description?:string;status?:string;task?:string;assignment?:string;sessionFile?:string;lastUpdate?:number;progress?:unknown;parentToolCallId?:string}
export interface TaskSnapshot{taskId:string;runId:string;seq:number;status:TaskStatus;turnStartedAt?:number|null;turnCompletedAt?:number|null;runtime?:RuntimeInfo;events:HostEvent[];text?:string;truncated?:boolean;error?:HostError|null;pendingUi?:PendingUiRequest[];tools?:ToolActivity[];trajectory?:TrajectoryRecord[];usage?:UsageSummary;availableCommands?:AvailableCommand[];authoritativeQueuedCount?:number;queuedMessages?:QueuedMessage[];subagents?:SubagentSnapshot[];extensionUi?:ExtensionUiState}
export interface GitChange{path:string;originalPath?:string|null;indexStatus:string;worktreeStatus:string;kind:'modified'|'renamed'|'untracked'|'deleted'|'conflicted'|string}
export interface GitStatus{rootIndex:number;available:boolean;branch?:string|null;changes:GitChange[]}
export interface CommitPreview{branch:string;root:string;head:string;tree:string;paths:string[]}
export interface GitDiff{rootIndex:number;path:string;staged:boolean;text:string;binary:boolean}
export interface TaskRoot{taskId:string;rootIndex:number;originalRoot:string;executionPath:string;gitTopLevel?:string|null;relativePath?:string|null;mode:'shared'|'isolated'|string;baselineCommit?:string|null;branch?:string|null;worktreePath?:string|null;status:string;createdByClient:boolean;sourceDirty:boolean}
export interface FileEntry{name:string;path:string;kind:'file'|'directory'|'symlink'|'special'|'unsupported'|'unavailable';size:number|null}
export interface DirectoryPage{entries:FileEntry[];truncated:boolean}
export interface FilePreview{name:string;size:number;state:'text'|'binary'|'too_large';text:string|null}
export interface Attachment{id:string;taskId:string;name:string;mime:string;size:number;createdAt:number}
export interface TerminalSnapshot{id:string;taskId:string;output:string;start:number;end:number;exited:boolean;exitCode:number|null;error:HostError|null}
export interface ObserverInfo{url:string;token:string}
export interface Project{id:string;name:string;roots:string[];archived:boolean;trusted:boolean}
export type ApprovalMode='always-ask'|'write'|'yolo';
export interface TaskRecord{thinking?:ThinkingLevel|null;approvalMode?:ApprovalMode|null;titleSource?:'initial'|'auto'|'user'|string;id:string;projectId:string;title:string;roots:string[];pinned:boolean;archived:boolean;sessionId:string|null;sessionFile:string|null;model:string|null;lastRun?:{id:string;taskId:string;state:string;errorCode:string|null}|null}
export interface HistoryPage{messages:Message[];nextCursor?:string|null;totalMessages:number}
export interface MessageUsage extends Partial<Record<'input'|'output'|'reasoning'|'cacheRead'|'cacheWrite'|'inputTokens'|'outputTokens'|'reasoningTokens'|'cacheReadTokens'|'cacheWriteTokens'|'totalTokens',number>> {cost?:number|{total?:number}|null}
export interface Message{role:string;content:unknown;timestamp?:number;completedAt?:number;duration?:number;ttft?:number;stopReason?:string;usage?:MessageUsage;toolCallId?:string}
export type TrajectoryKind='user'|'assistant'|'tool'|'context'|'system'|'compaction';
export type TrajectoryStatus='running'|'succeeded'|'failed'|'cancelled'|'interrupted'|'unknown';
export interface TrajectoryRecord {
 id:string; aliases:string[]; kind:TrajectoryKind; turn:number|null; step:number|null;
 parentId?:string|null; toolCallId?:string|null; name:string; status:TrajectoryStatus;
 content:unknown; input?:unknown; output?:unknown; model?:string|null;
 startedAt?:number|null; completedAt?:number|null; durationMs?:number|null; ttftMs?:number|null;
 timingSource?:'omp'|'host'|null; usage?:UsageSummary|null; error?:string|null;
 images:TrajectoryImage[]; truncated:boolean;
}
export interface TrajectoryImage {id:string;mime:string;label:string}
export interface TrajectoryPage {initialSystemPrompt?:TrajectoryRecord|null;afterCursor?:string|null;records:TrajectoryRecord[];nextCursor:string|null;totalRecords:number;revision:string;warnings:string[]}
export interface TrajectoryImageData {mime:string;data:string}
export interface ConfigModel { id:string; originalId?:string; name?:string; api?:string; contextWindow?:number; maxTokens?:number; reasoning?:boolean; thinking?:Record<string,unknown>; input?:string[]; cost?:Record<string,unknown> }
export interface ModelProvider { id:string; baseUrl:string|null; api:string|null; auth:string|null; authHeader:boolean|null; credentialConfigured:boolean; models:ConfigModel[] }
export interface ModelConfig {path:string;exists:boolean;revision:string;providers:ModelProvider[]}
export interface ModelEdit {revision:string;originalId:string|null;provider:ModelProvider;credentialAction:'keep'|'replace'|'clear';credential:string|null;deleted:boolean}
export interface ModelRoles {tiny:string|null;commit:string|null;smol:string|null}
export interface ModelRolesConfig {path:string;exists:boolean;revision:string;roles:ModelRoles}
export interface ModelRolesEdit {revision:string;roles:ModelRoles}
export interface ModelCatalog {version:string;source:string;cached:boolean;models:(ConfigModel&{provider:string;baseUrl:string})[]}
export type ThinkingLevel='off'|'minimal'|'low'|'medium'|'high'|'xhigh'|'max';
export type CapabilitySource='omp-runtime'|'catalog'|'config'|'unknown';
export type ThinkingSupport='supported'|'unsupported'|'unknown';
export interface ModelThinkingCapability {support:ThinkingSupport;levels:ThinkingLevel[];source:CapabilitySource;defaultLevel?:ThinkingLevel|null;unknownLevels?:unknown[]}
export interface ModelCapability {reasoning:boolean|null;thinking:ModelThinkingCapability}
// Missing capability is treated as unknown for older snapshots. Host discovery always supplies it.
export interface DiscoveredModel {id:string;provider:string;name?:string;capability?:ModelCapability}
export interface ModelVerification {defaultModel?:string|null;projectModel?:string|null;stage:'loaded'|'connected';message:string;models:DiscoveredModel[]}

export interface RuntimeTaskInfo {
 reason?:string|null;
 taskId:string; runId:string|null;
 status:'starting'|'ready'|'idle'|'running'|'compacting'|'retrying'|'pending_ui'|'recovering'|'interrupted'|'stopping'|'stopped'|'failed'|'external'|'unknown';
 owner:'desktop'|'terminal'|'none'; pid:number|null; startedAt:number|null; idleSince:number|null;
 keepAlive:boolean; autoStartSuppressed:boolean; executable:string|null; version:string|null; error:HostError|null;
}
export interface RuntimeCommand {command:string;executable:string;arguments:string[]}
export interface TitlePromptSettings {prompt:string;isDefault:boolean}
export interface SessionCatalogEntry {
 sessionKey:string;sessionId:string;cwd:string;title:string;createdAt:number;updatedAt:number;
 modelProvider?:string|null;modelId?:string|null;parentSession?:string|null;relationKind:'root'|'fork'|'subagent'|string;
 status:string;messageCount:number;totalTokens?:number|null;cost?:number|null;trusted:boolean;projectId?:string|null;
 taskId?:string|null;taskTitle?:string|null;running:boolean;unread:boolean;archived:boolean;
 parseState:'ready'|'partial'|'invalid'|string;parseErrorCode?:string|null;snippet?:string|null;
}
export interface SessionCatalogPage {entries:SessionCatalogEntry[];total:number;nextOffset?:number|null}
export interface SessionCatalogRefresh {discovered:number;indexed:number;unchanged:number;invalid:number}
export interface AgentProfile {scope:'user'|'project';projectId?:string|null;rootIndex?:number|null;name:string;content:string;revision:string}
export interface LoginProvider {id:string;name:string;available:boolean;authenticated:boolean}
export interface ProviderUsage {generatedAt?:number;reports?:unknown[];accountsWithoutUsage?:unknown[];disabledCredentials?:unknown[];capacity?:Record<string,unknown>;[key:string]:unknown}
export interface PluginOverview {plugins:{npm?:unknown[];marketplace?:unknown[];[key:string]:unknown};diagnostics:{name:string;status:'ok'|'warning'|'error'|string;message:string;fixed?:boolean}[]}

/** Installed catalog application; launch paths remain Host-owned. */
export interface OpenApp {id:string;name:string}

export type CapabilitySupport='supported'|'unsupported'|'unknown';
export interface HostCapabilities {
 hostVersion:string;
 minimumOmpVersion:string;
 transport:'desktop'|'browser';
 features:Record<string,CapabilitySupport>;
 runtimeCommands:Record<string,CapabilitySupport>;
 platform:{desktop:boolean;browser:boolean;notifications:CapabilitySupport;updater:CapabilitySupport};
}

export interface FileReference {rootIndex:number;path:string;name:string}
export type ProjectFolderSelection={existing:number}|{selected:string};
export interface ModelApplyResult {applied:string[];skipped:string[];errors:{taskId:string;error:HostError}[]}

export interface ProjectTransport {
 chooseFolders():Promise<{token:string;path:string}[]>;
 createLocal(name:string,tokens:string[],trusted:boolean):Promise<Project>;
 editLocal(id:string,name:string,folders:ProjectFolderSelection[],trusted:boolean):Promise<Project>;
 list():Promise<Project[]>;
 register(name:string,trusted:boolean):Promise<Project|null>;
 update(id:string,name:string,archived:boolean):Promise<void>;
 selectModel(projectId:string,provider:string,modelId:string):Promise<void>;
}
export interface TaskTransport {
 listRecords():Promise<TaskRecord[]>;
 create(projectId:string,title:string,mode:'shared'|'isolated',model?:string,autoTitle?:boolean):Promise<TaskRecord>;
 update(task:TaskRecord):Promise<TaskRecord>;
 fork(taskId:string,timestamp:number):Promise<TaskRecord>;
 approval(taskId:string,mode:ApprovalMode|null):Promise<TaskRecord>;
 suggestTitle(taskId:string):Promise<string>;
 relocate(taskId:string,trusted:boolean):Promise<TaskRecord|null>;
 resume(taskId:string):Promise<TaskSnapshot>;
 history(taskId:string,cursor:string|null):Promise<HistoryPage>;
 usage(taskId:string):Promise<TaskSnapshot>;
 gitStatus(taskId:string,rootIndex:number):Promise<GitStatus>;
 gitDiff(taskId:string,rootIndex:number,path:string,staged:boolean,untracked:boolean):Promise<GitDiff>;
 selectModel(taskId:string,provider:string,modelId:string):Promise<Record<string,unknown>>;
 thinking(taskId:string):Promise<string|null>;
 setThinking(taskId:string,level:string|null):Promise<void>;
}
export interface RuntimeTransport {
 probe(explicit:string):Promise<RuntimeInfo>;
 chooseExecutable():Promise<string|null>;
 discoverExecutable():Promise<string|null>;
 list():Promise<TaskSnapshot[]>;
 snapshot(taskId:string):Promise<TaskSnapshot>;
 prompt(taskId:string,message:string,attachmentIds?:string[],references?:FileReference[]):Promise<TaskSnapshot>;
 abort(taskId:string):Promise<TaskSnapshot>;
 stop(taskId:string):Promise<TaskSnapshot>;
 restart(taskId:string):Promise<TaskSnapshot>;
 respond(taskId:string,requestId:string,value:string|null,confirmed:boolean|null,cancelled:boolean):Promise<TaskSnapshot>;
 roots(taskId:string):Promise<TaskRoot[]>;
 isolate(taskId:string):Promise<TaskRoot[]>;
 cleanupWorktrees(taskId:string):Promise<TaskRoot[]>;
 recoverWorktrees(taskId:string):Promise<TaskRoot[]>;
 observer():Promise<ObserverInfo>;
 listManaged():Promise<RuntimeTaskInfo[]>;
 focus(taskId:string|null):Promise<void>;
 keepAlive(taskId:string,keepAlive:boolean):Promise<void>;
 action(taskId:string,action:'start'|'stop'|'restart'|'cancel',expectedRunId:string|null):Promise<RuntimeTaskInfo>;
 releaseIdle():Promise<string[]>;
 terminalCommand(taskId:string):Promise<RuntimeCommand>;
 send(taskId:string,action:'prompt'|'steer'|'follow_up'|'abort_and_prompt',message:string,streamingBehavior?:'steer'|'followUp'):Promise<TaskSnapshot>;
 setMode(taskId:string,type:'steering'|'follow_up'|'interrupt',mode:'all'|'one-at-a-time'|'immediate'|'wait'):Promise<TaskSnapshot>;
 setToggle(taskId:string,setting:'auto_compaction'|'auto_retry',enabled:boolean):Promise<TaskSnapshot>;
 compact(taskId:string,customInstructions?:string):Promise<TaskSnapshot>;
 abortRetry(taskId:string):Promise<TaskSnapshot>;
 commands(taskId:string):Promise<TaskSnapshot>;
 subagents(taskId:string):Promise<TaskSnapshot>;
 subagentMessages(taskId:string,subagentId?:string,sessionFile?:string,fromByte?:number):Promise<unknown>;
 setThinking(taskId:string,level:ThinkingLevel):Promise<TaskSnapshot>;
}
export interface FileTransport {
 search(taskId:string,rootIndex:number,query:string):Promise<DirectoryPage>;
 list(taskId:string,rootIndex:number,path:string):Promise<DirectoryPage>;
 preview(taskId:string,rootIndex:number,path:string):Promise<FilePreview>;
 attachments(taskId:string):Promise<Attachment[]>;
 importAttachment(taskId:string):Promise<Attachment|null>;
 previewAttachment(taskId:string,resourceId:string):Promise<FilePreview>;
}
export interface GitTransport {
 change(taskId:string,rootIndex:number,path:string,action:'stage'|'unstage'|'discard',confirmed?:boolean):Promise<void>;
 previewCommit(taskId:string,rootIndex:number):Promise<CommitPreview>;
 commit(taskId:string,rootIndex:number,message:string,expected:CommitPreview):Promise<string>;
}
export interface TerminalTransport {
 create(taskId:string,rootIndex:number):Promise<TerminalSnapshot>;
 snapshot(taskId:string,terminalId:string,after?:number):Promise<TerminalSnapshot>;
 write(taskId:string,terminalId:string,input:string):Promise<void>;
 resize(taskId:string,terminalId:string,cols:number,rows:number):Promise<void>;
 close(taskId:string,terminalId:string):Promise<void>;
}
export interface ModelTransport {
 apply():Promise<ModelApplyResult>;
 load():Promise<ModelConfig>;
 save(edit:ModelEdit):Promise<ModelConfig>;
 rolesLoad():Promise<ModelRolesConfig>;
 rolesSave(edit:ModelRolesEdit):Promise<ModelRolesConfig>;
 catalog():Promise<ModelCatalog>;
 verify(provider?:string|null,modelId?:string|null,projectId?:string|null):Promise<ModelVerification>;
}
export interface TrajectoryTransport {
 read(taskId:string,cursor?:string|null,after?:boolean):Promise<TrajectoryPage>;
 image(taskId:string,recordId:string,imageId:string):Promise<TrajectoryImageData>;
}
export interface OpenAppTransport {
 list(refresh?:boolean):Promise<OpenApp[]>;
 icon(appId:string):Promise<string>;
 open(taskId:string,appId:string):Promise<void>;
}
export interface AgentProfileTransport {
 list(scope:'user'|'project',projectId?:string|null,rootIndex?:number|null):Promise<AgentProfile[]>;
 save(scope:'user'|'project',projectId:string|null,rootIndex:number|null,name:string,content:string,revision:string):Promise<AgentProfile>;
}
export interface ProviderTransport {
 loginOptions(taskId:string):Promise<{providers:LoginProvider[]}>;
 login(taskId:string,providerId:string):Promise<{providerId:string}>;
 logout(providerId:string,confirmed:boolean):Promise<void>;
 usage():Promise<ProviderUsage>;
}
export interface PluginTransport {
 overview(projectId?:string|null):Promise<PluginOverview>;
 setEnabled(projectId:string|null,pluginId:string,enabled:boolean,scope:'user'|'project'):Promise<unknown>;
 mutate(projectId:string|null,action:'install'|'uninstall'|'upgrade',pluginId:string,scope:'user'|'project',confirmed:boolean):Promise<PluginOverview>;
}
export interface HostTransport {
 capabilities():Promise<HostCapabilities>;
 projects:ProjectTransport;
 tasks:TaskTransport;
 runtime:RuntimeTransport;
 files:FileTransport;
 git:GitTransport;
 terminals:TerminalTransport;
 models:ModelTransport;
	trajectory:TrajectoryTransport;
	agents:AgentProfileTransport;
 providers:ProviderTransport;
 plugins:PluginTransport;
 titlePrompt:{load():Promise<TitlePromptSettings>;save(prompt:string|null):Promise<TitlePromptSettings>};
 sessions:{
  recover(taskId:string,confirmed:boolean):Promise<string>;
  download(taskId:string):Promise<string|null>;
  refresh(rebuild?:boolean):Promise<SessionCatalogRefresh>;
  catalog(query:string,includeArchived:boolean,offset?:number,limit?:number):Promise<SessionCatalogPage>;
  updateView(sessionKey:string,archived?:boolean|null,lastSeenEntryId?:string|null,scrollAnchorEntryId?:string|null,scrollAnchorOffset?:number|null):Promise<void>;
 };
 openInApp:OpenAppTransport;
}
