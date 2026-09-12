import {useEffect,useState} from 'react';
import {trajectory,hostError} from '../../../host';
import {Markdown} from '../message-list';
import {durationLabel,kindLabels,statusLabels,stringify,type TrajectoryRecord} from './trajectory-model';
import type {TrajectoryImageData} from '../../../../../../packages/host-contract/src';
export type ImageCache=Map<string,Promise<TrajectoryImageData>>;
function Image({taskId,record,id,label,cache}:{taskId:string;record:string;id:string;label:string;cache:ImageCache}){
 const [src,setSrc]=useState(''),[error,setError]=useState(''),[attempt,setAttempt]=useState(0);
 useEffect(()=>{let disposed=false;setError('');setSrc('');const key=`${taskId}:${record}:${id}`;let request=cache.get(key);if(!request){request=trajectory.image(taskId,record,id);cache.set(key,request);}
 void request.then(data=>{if(!/^image\/(png|jpeg|gif|webp)$/.test(data.mime)||!/^[A-Za-z0-9+/]*={0,2}$/.test(data.data))throw new Error('图片格式不受支持');if(!disposed)setSrc(`data:${data.mime};base64,${data.data}`);}).catch(e=>{cache.delete(key);if(!disposed)setError(hostError(e).message);});return()=>{disposed=true;};},[taskId,record,id,cache,attempt]);
 return <figure>{error?<div role="alert">{error}<button onClick={()=>setAttempt(v=>v+1)}>重试图片</button></div>:src?<img src={src} alt={label} onError={()=>setError('图片解码失败')}/>:<span>正在读取图片…</span>}<figcaption>{label}</figcaption></figure>;
}
export function TrajectoryDetails({taskId,record,cache,onError,onClose}:{taskId:string;record:TrajectoryRecord;cache:ImageCache;onError:(e:unknown)=>void;onClose:()=>void}){
 const u=record.usage;
 const time=(value?:number|null)=>value==null?'未记录':new Date(value).toISOString();
 const entries:[string,unknown][]=[['状态',statusLabels[record.status]],['记录 ID',record.id],['父记录',record.parentId??'无'],['工具调用 ID',record.toolCallId??'无'],['模型',record.model??'未记录'],['开始',time(record.startedAt)],['结束',time(record.completedAt)],['耗时',durationLabel(record.status==='running'?null:record.durationMs)],['TTFT',durationLabel(record.ttftMs)],['计时来源',record.timingSource==='host'?'Host 观测':record.timingSource==='omp'?'OMP':'未记录'],['输入 token',u?.inputTokens??'未记录'],['输出 token',u?.outputTokens??'未记录'],['思考 token',u?.reasoningTokens??'未记录'],['缓存读取',u?.cacheReadTokens??'未记录'],['缓存写入',u?.cacheWriteTokens??'未记录'],['总 token',u?.totalTokens??'未记录'],['费用',u?.cost==null?'未记录':`$${u.cost}`]];
 const content=record.content;
 const blocks=Array.isArray(content)?content:typeof content==='string'?[{type:'text',text:content}]:[];
 return <div className="trajectory-details"><header><h3>{kindLabels[record.kind]} · {record.name}</h3><button onClick={onClose} aria-label="关闭轨迹详情">关闭</button></header><dl className="trajectory-meta">{entries.map(([k,v])=><div key={k}><dt>{k}</dt><dd>{String(v)}</dd></div>)}</dl>
 {record.truncated&&<p role="status">显示内容已截断，完整内容仍保存在 OMP 会话中。</p>}
 {record.input!=null&&<><h4>输入</h4><pre>{stringify(record.input)}</pre></>}
 {record.output!=null&&<><h4>输出</h4><pre>{stringify(record.output)}</pre></>}
 <h4>内容</h4>{blocks.map((b,i)=>b.type==='thinking'||b.type==='reasoning'?<details key={i}><summary>思考</summary><Markdown text={b.thinking??b.text??''} onError={onError}/></details>:typeof b.text==='string'?<Markdown key={i} text={b.text} onError={onError}/>:b.type==='image'?null:<pre key={i}>{stringify(b)}</pre>)}
 {!blocks.length&&<pre>{stringify(content)||'未记录'}</pre>}
 {record.images.map(img=><Image key={img.id} taskId={taskId} record={record.id} id={img.id} label={img.label} cache={cache}/>)}
 {record.error&&<pre role="alert" className="trajectory-error">{record.error}</pre>}
 <details><summary>原始投影记录</summary><button onClick={()=>void navigator.clipboard.writeText(stringify(record)).catch(onError)}>复制记录</button><pre>{stringify(record)}</pre></details></div>;
}
