import {useEffect,useState} from 'react';
import {File} from 'lucide-react';
import {searchFiles,hostError,type FileReference} from '@/host';

export function useFileCompletion(taskId:string,rootCount:number,query:string|null){
 const [items,setItems]=useState<FileReference[]>([]),[loading,setLoading]=useState(false),[error,setError]=useState(''),[truncated,setTruncated]=useState(false);
 useEffect(()=>{let active=true;setItems([]);setError('');setTruncated(false);if(query===null){setLoading(false);return;}setLoading(true);
  const timer=setTimeout(()=>{void Promise.all(Array.from({length:rootCount},(_,rootIndex)=>searchFiles(taskId,rootIndex,query).then(page=>({page,rootIndex})))).then(results=>{if(active){setItems(results.flatMap(({page,rootIndex})=>page.entries.map(e=>({rootIndex,path:e.path,name:e.name}))).slice(0,100));setTruncated(results.some(r=>r.page.truncated));}}).catch(e=>{if(active)setError(hostError(e).message);}).finally(()=>{if(active)setLoading(false);});},100);
  return()=>{active=false;clearTimeout(timer);};
 },[taskId,rootCount,query]);
 return {items,loading,error,truncated};
}

export function FileCompletion({items,selected,onSelect,loading,error,truncated}:{items:FileReference[];selected:number;onSelect:(item:FileReference)=>void;loading:boolean;error:string;truncated:boolean}){
 return <div className="file-completion" role="listbox" id="file-candidates" aria-label="文件候选">
  {loading?<p role="status">正在搜索文件…</p>:error?<p role="alert">{error}</p>:!items.length?<p role="status">没有匹配的文件</p>:items.map((item,i)=><button type="button" role="option" id={`file-candidate-${i}`} aria-selected={selected===i} key={`${item.rootIndex}:${item.path}`} onMouseDown={e=>e.preventDefault()} onClick={()=>onSelect(item)}><File size={15}/><span><strong>{item.name}</strong><small>{item.rootIndex===0?'主目录':`附加目录 ${item.rootIndex}`} · {item.path}</small></span></button>)}
  {truncated&&<p>结果已截取，请输入更完整的路径。</p>}
 </div>;
}
