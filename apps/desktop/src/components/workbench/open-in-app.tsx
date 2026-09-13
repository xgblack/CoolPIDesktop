import {useEffect, useRef, useState} from 'react';
import {AppWindow, Check, ChevronDown, RefreshCw} from 'lucide-react';
import {Button} from '@/components/ui/button';
import {DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator} from '@/components/ui/dropdown-menu';
import {Tooltip, TooltipContent, TooltipTrigger} from '@/components/ui/tooltip';
import {hostError, openInApp} from '@/host';
import type {OpenApp} from '../../../../../packages/host-contract/src';
import './open-in-app.css';

const CHOICE_KEY = 'cool-pi-desktop.open-in-app.choice';
let availability: Promise<OpenApp[]> | undefined;
const icons = new Map<string, Promise<string>>();
function loadApps(refresh = false) {
 if (refresh) icons.clear();
 if (refresh || !availability) {
  const request = openInApp.list(refresh);
  availability = request;
  void request.catch(() => {if (availability === request) availability = undefined;});
 }
 return availability;
}
function AppIcon({app, revision}: {app: OpenApp; revision: number}) {
 const [image, setImage] = useState<string>();
 const [error, setError] = useState<string>();
 useEffect(() => {
  let active = true;
  setImage(undefined); setError(undefined);
  let request = icons.get(app.id);
  if (!request) {request = openInApp.icon(app.id); icons.set(app.id, request);}
  void request.then(value => {if (active) setImage(value);}, reason => {if (active) setError(hostError(reason).message);});
  return () => {active = false;};
 }, [app.id, revision]);
 return image ? <img src={image} alt="" className="open-app-icon" onError={() => {setImage(undefined); setError('无法显示应用图标');}}/> : <AppWindow className="open-app-icon" aria-hidden="true"><title>{error}</title></AppWindow>;
}
export function OpenInApp({taskId, disabled = false}: {taskId: string; disabled?: boolean}) {
 const [apps, setApps] = useState<OpenApp[] | null>(null);
 const [choice, setChoice] = useState(() => {try {return localStorage.getItem(CHOICE_KEY) ?? '';} catch {return '';}});
 const [error, setError] = useState('');
 const [busy, setBusy] = useState(false);
 const [revision, setRevision] = useState(0);
 const pending = useRef(false);
 const mounted = useRef(false);
 const errorTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
 useEffect(() => {
  mounted.current = true;
  void loadApps().then(value => {if (mounted.current) setApps(value);}, reason => {if (mounted.current) setError(hostError(reason).message);});
  return () => {mounted.current = false; clearTimeout(errorTimer.current);};
 }, []);
 const selected = apps?.find(app => app.id === choice) ?? apps?.[0];
 async function refresh() {
  if (pending.current) return;
  pending.current = true; setBusy(true); setError('');
  try {const value = await loadApps(true); if (mounted.current) {setApps(value); setRevision(v => v + 1);}}
  catch (reason) {if (mounted.current) setError(hostError(reason).message);}
  finally {pending.current = false; if (mounted.current) setBusy(false);}
 }
 async function launch(app: OpenApp) {
  if (pending.current || disabled) return;
  pending.current = true; setError(''); clearTimeout(errorTimer.current);
  setChoice(app.id);
  try {localStorage.setItem(CHOICE_KEY, app.id);} catch {setError('无法保存打开方式，本次选择仅在当前窗口生效');}
  const timer = setTimeout(() => {if (mounted.current) setBusy(true);}, 250);
  try {await openInApp.open(taskId, app.id);}
  catch (reason) {
   if (mounted.current) {
    setError(hostError(reason).message);
    errorTimer.current = setTimeout(() => {if (mounted.current) setError('');}, 2000);
   }
  } finally {clearTimeout(timer); pending.current = false; if (mounted.current) setBusy(false);}
 }
 if (apps?.length === 0 && !error) return null;
 const title = error || (selected ? `在${selected.name}中打开工作目录` : '正在检测打开方式');
 return <div className="open-in-app" data-error={!!error} aria-busy={busy}>
  <Tooltip><TooltipTrigger asChild><Button variant="ghost" size="icon-sm" aria-label={title} disabled={!selected || disabled || busy} onClick={() => {if (selected) void launch(selected);}}>
   {selected ? <AppIcon app={selected} revision={revision}/> : <AppWindow size={16}/>}
  </Button></TooltipTrigger><TooltipContent>{title}</TooltipContent></Tooltip>
  <DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" size="icon-sm" className="open-app-chevron" aria-label="选择打开方式" disabled={disabled || busy}><ChevronDown size={12}/></Button></DropdownMenuTrigger>
   <DropdownMenuContent align="end" className="open-app-menu" aria-label="在应用中打开工作目录">
    {apps?.map(app => <DropdownMenuItem key={app.id} className="open-app-item" data-selected={selected?.id === app.id} onSelect={() => void launch(app)}>
     <AppIcon app={app} revision={revision}/><span>在{app.name}中打开</span>{selected?.id === app.id && <Check className="open-app-check"/>}
    </DropdownMenuItem>)}
    {error && <div role="alert" className="open-app-error">{error}</div>}
    <DropdownMenuSeparator/><DropdownMenuItem onSelect={() => void refresh()}><RefreshCw/>刷新应用列表</DropdownMenuItem>
   </DropdownMenuContent>
  </DropdownMenu>
  <span className="sr-only" role="status" aria-live="polite">{error || (busy ? '正在打开应用' : '')}</span>
 </div>;
}
