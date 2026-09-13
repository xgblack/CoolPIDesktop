export type RuntimeStatus='not_found'|'not_executable'|'version_unreadable'|'version_unsupported'|'starting'|'handshake_failed'|'capability_query_failed'|'partially_available'|'ready'|'process_exited'|'model_required';
export interface HostError{code:string;message:string;suggestion?:string}
export interface RuntimeInfo{status:RuntimeStatus;executable?:string|null;version?:string|null;protocol?:number|null;capabilities?:unknown;detail?:string|null;error?:HostError|null}
export type TaskStatus='starting'|'ready'|'running'|'idle'|'failed'|'interrupted'|'stopped';
export interface HostEvent{taskId:string;runId:string;seq:number;eventType:string;payload:unknown;trajectory?:TrajectoryRecord[]}
export interface ToolActivity{id:string;name:string;status:'running'|'succeeded'|'failed'|'cancelled'|string;args:unknown;result?:unknown;seq:number}
export interface UsageSummary{contextTokens?:number;contextWindow?:number;contextPercent?:number;inputTokens?:number;outputTokens?:number;reasoningTokens?:number;cacheReadTokens?:number;cacheWriteTokens?:number;totalTokens?:number;cost?:number}
export interface PendingUiRequest{id:string;method:'confirm'|'select'|'input'|'editor';title?:string;message?:string;options?:string[]}
export interface TaskSnapshot{taskId:string;runId:string;seq:number;status:TaskStatus;runtime?:RuntimeInfo;events:HostEvent[];text?:string;truncated?:boolean;error?:HostError|null;pendingUi?:PendingUiRequest[];tools?:ToolActivity[];trajectory?:TrajectoryRecord[];usage?:UsageSummary}
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
export interface TaskRecord{approvalMode?:ApprovalMode|null;id:string;projectId:string;title:string;roots:string[];pinned:boolean;archived:boolean;sessionId:string|null;sessionFile:string|null;model:string|null;lastRun?:{id:string;taskId:string;state:string;errorCode:string|null}|null}
export interface HistoryPage{messages:Message[];nextCursor?:string|null;totalMessages:number}
export interface Message{role:string;content:unknown;timestamp?:number;toolCallId?:string}
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
export interface ModelCatalog {version:string;source:string;cached:boolean;models:(ConfigModel&{provider:string;baseUrl:string})[]}
export interface ModelVerification {defaultModel?:string|null;projectModel?:string|null;stage:'loaded'|'connected';message:string;models:{id:string;provider:string;name?:string}[]}

export interface RuntimeTaskInfo {
 reason?:string|null;
 taskId:string; runId:string|null;
 status:'starting'|'ready'|'idle'|'running'|'interrupted'|'stopped'|'failed'|'external'|'unknown';
 owner:'desktop'|'terminal'|'none'; pid:number|null; startedAt:number|null; idleSince:number|null;
 keepAlive:boolean; autoStartSuppressed:boolean; executable:string|null; version:string|null; error:HostError|null;
}
export interface RuntimeCommand {command:string;executable:string;arguments:string[]}
