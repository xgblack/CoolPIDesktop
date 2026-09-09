import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core'; import './style.css';
type Runtime={status:string;executable?:string;version?:string;protocol?:number;error?:string};
export function App(){const [r,setR]=useState<Runtime>({status:'未检测'});const [b,setB]=useState(false);async function detect(){setB(true);try{const x=await fetch('/api/runtime/omp');setR(await x.json())}catch(e){setR({status:'Host 未连接',error:String(e)})}finally{setB(false)}}return <main><h1>OMP Desktop · P0</h1><section><h2>系统 OMP Runtime</h2><p className="status">{r.status}</p><dl><dt>路径</dt><dd>{r.executable??'—'}</dd><dt>版本</dt><dd>{r.version??'—'}</dd><dt>RPC 协议</dt><dd>{r.protocol??'—'}</dd></dl>{r.error&&<pre>{r.error}</pre>}<button onClick={detect} disabled={b}>{b?'检测中…':'检测 OMP'}</button><p className="hint">当前检测使用 Tauri Rust Host 的系统 PATH；未安装时应显示明确错误。</p></section></main>}

