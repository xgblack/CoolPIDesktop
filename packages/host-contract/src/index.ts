export type RuntimeStatus='not_found'|'not_executable'|'version_unreadable'|'version_unsupported'|'starting'|'handshake_failed'|'capability_query_failed'|'partially_available'|'ready'|'process_exited';
export interface HostError{code:string;message:string;suggestion?:string}
export interface RuntimeInfo{status:RuntimeStatus;executable?:string|null;version?:string|null;protocol?:number|null;capabilities?:unknown;detail?:string|null;error?:HostError|null}
export type TaskStatus='starting'|'ready'|'running'|'idle'|'failed'|'interrupted'|'stopped';
export interface HostEvent{taskId:string;runId:string;seq:number;eventType:string;payload:unknown}
export interface TaskSnapshot{taskId:string;runId:string;seq:number;status:TaskStatus;runtime?:RuntimeInfo;events:HostEvent[];error?:HostError|null}
export interface ObserverInfo{url:string;token:string}
