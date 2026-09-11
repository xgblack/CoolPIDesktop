export type RuntimeStatus='not_found'|'not_executable'|'version_unreadable'|'version_unsupported'|'starting'|'handshake_failed'|'capability_query_failed'|'partially_available'|'ready'|'process_exited'|'model_required';
export interface HostError{code:string;message:string;suggestion?:string}
export interface RuntimeInfo{status:RuntimeStatus;executable?:string|null;version?:string|null;protocol?:number|null;capabilities?:unknown;detail?:string|null;error?:HostError|null}
export type TaskStatus='starting'|'ready'|'running'|'idle'|'failed'|'interrupted'|'stopped';
export interface HostEvent{taskId:string;runId:string;seq:number;eventType:string;payload:unknown}
export interface ToolActivity{id:string;name:string;status:'running'|'succeeded'|'failed'|'cancelled'|string;args:unknown;result?:unknown;seq:number}
export interface UsageSummary{contextTokens?:number;contextWindow?:number;contextPercent?:number;inputTokens?:number;outputTokens?:number;reasoningTokens?:number;cacheReadTokens?:number;cacheWriteTokens?:number;totalTokens?:number;cost?:number}
export interface PendingUiRequest{id:string;method:'confirm'|'select'|'input'|'editor';title?:string;message?:string;options?:string[]}
export interface TaskSnapshot{taskId:string;runId:string;seq:number;status:TaskStatus;runtime?:RuntimeInfo;events:HostEvent[];text?:string;truncated?:boolean;error?:HostError|null;pendingUi?:PendingUiRequest[];tools?:ToolActivity[];usage?:UsageSummary}
export interface GitChange{path:string;originalPath?:string|null;indexStatus:string;worktreeStatus:string;kind:'modified'|'renamed'|'untracked'|'deleted'|'conflicted'|string}
export interface GitStatus{rootIndex:number;available:boolean;branch?:string|null;changes:GitChange[]}
export interface GitDiff{rootIndex:number;path:string;staged:boolean;text:string;binary:boolean}
export interface TaskRoot{taskId:string;rootIndex:number;originalRoot:string;executionPath:string;gitTopLevel?:string|null;relativePath?:string|null;mode:'shared'|'isolated'|string;baselineCommit?:string|null;branch?:string|null;worktreePath?:string|null;status:string;createdByClient:boolean;sourceDirty:boolean}
export interface ObserverInfo{url:string;token:string}
export interface Project{id:string;name:string;roots:string[];archived:boolean;trusted:boolean}
export interface TaskRecord{id:string;projectId:string;title:string;roots:string[];pinned:boolean;archived:boolean;sessionId:string|null;sessionFile:string|null;model:string|null;lastRun?:{id:string;taskId:string;state:string;errorCode:string|null}|null}
export interface HistoryPage{messages:Message[];nextCursor?:string|null;totalMessages:number}
export interface Message{role:string;content:unknown;timestamp?:number;toolCallId?:string}
export interface ConfigModel { id:string; originalId?:string; name?:string; api?:string; contextWindow?:number; maxTokens?:number; reasoning?:boolean; thinking?:Record<string,unknown>; input?:string[]; cost?:Record<string,unknown> }
export interface ModelProvider { id:string; baseUrl:string|null; api:string|null; auth:string|null; authHeader:boolean|null; credentialConfigured:boolean; models:ConfigModel[] }
export interface ModelConfig {path:string;exists:boolean;revision:string;providers:ModelProvider[]}
export interface ModelEdit {revision:string;originalId:string|null;provider:ModelProvider;credentialAction:'keep'|'replace'|'clear';credential:string|null;deleted:boolean}
export interface ModelCatalog {version:string;source:string;cached:boolean;models:(ConfigModel&{provider:string;baseUrl:string})[]}
export interface ModelVerification {stage:'loaded'|'connected';message:string;models:{id:string;provider:string;name?:string}[]}
