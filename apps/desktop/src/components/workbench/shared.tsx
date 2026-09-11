import type {ReactNode} from 'react';
import {AlertCircle,CheckCircle2,LoaderCircle,PauseCircle,Circle} from 'lucide-react';
import type {HostError} from '../../../../../packages/host-contract/src';
import {Button} from '@/components/ui/button';
import {Tooltip,TooltipContent,TooltipTrigger} from '@/components/ui/tooltip';
export function IconButton({label,children,onClick,disabled,'aria-pressed':pressed}:{'aria-pressed'?:boolean;label:string;children:ReactNode;onClick?:()=>void;disabled?:boolean}){return <Tooltip><TooltipTrigger asChild><Button aria-pressed={pressed} aria-label={label} variant="ghost" size="icon-sm" onClick={onClick} disabled={disabled}>{children}</Button></TooltipTrigger><TooltipContent>{label}</TooltipContent></Tooltip>;}
export function ErrorNotice({error}:{error?:HostError|null}){return error?<div role="alert" className="error-notice"><AlertCircle size={16}/><div><strong>{error.message}</strong><p>{error.suggestion}</p><code>{error.code}</code></div></div>:null;}
const statusLabels:Record<string,string>={ready:'就绪',running:'生成中',idle:'已完成',starting:'启动中',interrupted:'已中断',stopped:'已停止',failed:'运行失败'};
export function Status({status}:{status?:string}){const Icon=status==='running'||status==='starting'?LoaderCircle:status==='failed'?AlertCircle:status==='idle'?CheckCircle2:status==='interrupted'?PauseCircle:Circle;return <span className={'status status-'+status}><Icon size={12} className={status==='running'||status==='starting'?'animate-spin':''}/>{statusLabels[status??'']??'未运行'}</span>;}
