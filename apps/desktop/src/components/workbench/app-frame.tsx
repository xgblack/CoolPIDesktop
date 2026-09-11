import {useEffect, useState, type ReactNode} from 'react';
import {Group, Panel, Separator, usePanelRef} from 'react-resizable-panels';
import {PanelLeftOpen, Plus, Search, Settings2, Maximize2, Minimize2, X} from 'lucide-react';
import {IconButton} from './shared';

export function AppFrame({sidebar, children, inspector, inspectorOpen, collapsed, onExpand, onNew, onSearch, onSettings, onCloseInspector}: {
 sidebar: ReactNode; children: ReactNode; inspector?: ReactNode; inspectorOpen:boolean; collapsed:boolean;
 onExpand:()=>void; onNew:()=>void; onSearch:()=>void; onSettings:()=>void; onCloseInspector:()=>void;
}) {
 const [fullscreen,setFullscreen]=useState(false);
 const panel=usePanelRef();
 useEffect(()=>{const frame=requestAnimationFrame(()=>{if(inspectorOpen)panel.current?.expand();else panel.current?.collapse();});if(!inspectorOpen)setFullscreen(false);return()=>cancelAnimationFrame(frame);},[inspectorOpen,!!inspector]);
 return <div className="app-frame">
  <Group orientation="horizontal">
   {collapsed?<div className="sidebar-rail"><button className="rail-brand" aria-label="展开侧栏" onClick={onExpand}>π</button><IconButton label="展开项目" onClick={onExpand}><PanelLeftOpen/></IconButton><IconButton label="新建任务" onClick={onNew}><Plus/></IconButton><IconButton label="搜索任务" onClick={onSearch}><Search/></IconButton><div className="rail-spacer"/><IconButton label="打开设置" onClick={onSettings}><Settings2/></IconButton></div>:<><Panel id="sidebar" defaultSize="280px" minSize="264px" maxSize="420px">{sidebar}</Panel><Separator className="panel-separator"/></>}
   <Panel id="conversation" minSize="400px">{children}</Panel>
   {inspector&&<>{inspectorOpen&&<Separator className="panel-separator"/>}<Panel id="inspector" panelRef={panel} collapsible collapsedSize="0px" defaultSize={inspectorOpen?"360px":"0px"} minSize="300px" maxSize="520px"><div inert={!inspectorOpen} className={fullscreen?'inspector-shell inspector-fullscreen':'inspector-shell'}><div className="inspector-shell-header"><span>任务上下文</span><div><IconButton label={fullscreen?'退出详情全屏':'详情全屏'} onClick={()=>setFullscreen(!fullscreen)}>{fullscreen?<Minimize2/>:<Maximize2/>}</IconButton><IconButton label="关闭详情" onClick={()=>{setFullscreen(false);onCloseInspector();}}><X/></IconButton></div></div>{inspector}</div></Panel></>}
  </Group>
 </div>;
}
