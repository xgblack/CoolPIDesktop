import {BookOpen, Brain, ChevronRight, FileText, Globe, Pencil, Search, Terminal, Wrench, AlertCircle, CircleDashed} from 'lucide-react';
import type {ComponentType, ReactNode} from 'react';

export function toolIcon(name:string) {
 const n=name.toLowerCase();
 if(/search|grep|glob/.test(n))return Search;
 if(/read/.test(n))return BookOpen;
 if(/edit|write|patch/.test(n))return Pencil;
 if(/bash|shell|exec|terminal/.test(n))return Terminal;
 if(/web|browser|fetch/.test(n))return Globe;
 return Wrench;
}
export function activitySummary(value:unknown):string {
 if(typeof value==='string')return value.split('\n')[0];
 if(value&&typeof value==='object'){
  const v=value as Record<string,unknown>;
  for(const key of ['path','file_path','command','cmd','query','pattern','url'])if(typeof v[key]==='string')return activitySummary(v[key]);
 }
 return '';
}
export function ActivityRow({kind='tool',name='',preview='',state,label,children,defaultOpen=false}:{kind?:'tool'|'thinking'|'context'|'progress';name?:string;preview?:string;state?:string;label?:string;children?:ReactNode;defaultOpen?:boolean}){
 const Icon:ComponentType<{size?:number}>=kind==='thinking'?Brain:kind==='context'?FileText:kind==='progress'?CircleDashed:toolIcon(name);
 const failed=state==='failed';
 return <details className="activity-row" data-kind={kind} data-state={state} open={defaultOpen||undefined}>
  <summary><span className="activity-icon"><Icon size={14}/></span><span className="activity-label">{label??(kind==='thinking'?'思考':kind==='context'?'上下文':name||'工具')}</span><span className="activity-preview">{preview}</span>{state&&<span className="activity-state">{failed&&<AlertCircle size={12}/>}{{running:'执行中',succeeded:'已完成',failed:'失败',cancelled:'已取消',unknown:'状态未知'}[state]}</span>}<ChevronRight size={12} className="activity-chevron"/></summary>
  <div className="activity-body">{children}</div>
 </details>;
}
