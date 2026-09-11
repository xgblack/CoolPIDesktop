import {useEffect, useRef, useState} from 'react';
import {Binary, ChevronDown, ChevronRight, GitBranch, RefreshCw, Wrench} from 'lucide-react';
import {hostError, records} from '@/host';
import {Button} from '@/components/ui/button';
import {Select, SelectContent, SelectItem, SelectTrigger, SelectValue} from '@/components/ui/select';
import {ErrorNotice} from '@/components/workbench/shared';
import type {GitChange, GitDiff, GitStatus, HostError, TaskRecord, TaskSnapshot, ToolActivity, UsageSummary} from '../../../../../packages/host-contract/src';

const display = (value: unknown) => JSON.stringify(value, null, 2);
function NumberValue({value, suffix = ''}: {value?: number; suffix?: string}) { return <span>{value === undefined || value === null ? '未知' : `${new Intl.NumberFormat('zh-CN', {maximumFractionDigits: 2}).format(value)}${suffix}`}</span>; }

function Usage({usage, canRefresh, busy, onRefresh}: {usage?: UsageSummary; canRefresh: boolean; busy: boolean; onRefresh: () => void}) {
  const percent = usage?.contextPercent ?? (usage?.contextTokens != null && usage.contextWindow ? usage.contextTokens / usage.contextWindow * 100 : undefined);
  return <section className="inspector-section" aria-label="上下文与用量"><div className="inspector-heading"><div><h2>上下文与用量</h2><p>仅显示 OMP 实际报告的数据</p></div><Button size="sm" variant="ghost" disabled={!canRefresh || busy} onClick={onRefresh}><RefreshCw size={14}/>刷新</Button></div>
    <dl className="usage-grid"><div><dt>当前上下文</dt><dd><NumberValue value={usage?.contextTokens}/>{usage?.contextWindow != null && <> / <NumberValue value={usage.contextWindow}/></>}</dd></div><div><dt>上下文占用</dt><dd><NumberValue value={percent} suffix="%"/></dd></div><div><dt>累计输入</dt><dd><NumberValue value={usage?.inputTokens}/></dd></div><div><dt>累计输出</dt><dd><NumberValue value={usage?.outputTokens}/></dd></div><div><dt>缓存读取 / 写入</dt><dd><NumberValue value={usage?.cacheReadTokens}/> / <NumberValue value={usage?.cacheWriteTokens}/></dd></div><div><dt>成本</dt><dd>{usage?.cost == null ? '未知' : `$${usage.cost.toFixed(6)}`}</dd></div></dl>
    <p className="inspector-note">“当前上下文”不等于累计输入。未由 OMP 报告的值不会以 0 代替。</p>
  </section>;
}

function ToolRow({tool}: {tool: ToolActivity}) {
  const [open, setOpen] = useState(tool.status === 'running');
  return <article className="tool-row"><button className="tool-summary" onClick={() => setOpen(value => !value)} aria-expanded={open}>{open ? <ChevronDown size={14}/> : <ChevronRight size={14}/>}<span className={`tool-status tool-${tool.status}`}>{tool.status === 'running' ? '运行中' : tool.status === 'succeeded' ? '完成' : tool.status === 'cancelled' ? '已取消' : '失败'}</span><code>{tool.name}</code></button>{open && <div className="tool-detail"><h3>参数</h3><pre>{display(tool.args)}</pre>{tool.result !== undefined && <><h3>结果</h3><pre>{display(tool.result)}</pre></>}</div>}</article>;
}
function Tools({tools}: {tools?: ToolActivity[]}) { return <section className="inspector-section" aria-label="工具执行"><div className="inspector-heading"><div><h2><Wrench size={15}/>工具执行</h2><p>当前运行的 OMP 工具调用</p></div></div>{!tools?.length ? <p className="inspector-empty">本次运行尚未收到工具调用。</p> : <div className="tool-list">{[...tools].reverse().map(tool => <ToolRow key={tool.id} tool={tool}/>)}</div>}</section>; }

function ChangeButton({change, staged, onOpen}: {change: GitChange; staged: boolean; onOpen: (change: GitChange, staged: boolean) => void}) {
  const changed = staged ? change.indexStatus : change.worktreeStatus;
  if (!changed || (staged && change.kind === 'untracked')) return null;
  return <Button variant="ghost" size="sm" className="git-change" onClick={() => onOpen(change, staged)}><span className={`git-code git-${change.kind}`}>{changed === '?' ? '未跟踪' : staged ? `暂存 ${changed}` : `工作区 ${changed}`}</span><span className="truncate">{change.path}{change.originalPath ? ` ← ${change.originalPath}` : ''}</span></Button>;
}
function Git({task}: {task: TaskRecord}) {
  const [root, setRoot] = useState('0'), [status, setStatus] = useState<GitStatus>(), [diff, setDiff] = useState<GitDiff>(), [loading, setLoading] = useState(true), [error, setError] = useState<HostError>();
  const epoch = useRef(0);
  const [diffLoading, setDiffLoading] = useState(false);
  const load = async (index = Number(root)) => {
    const request = ++epoch.current;
    setLoading(true); setError(undefined); setDiff(undefined); setStatus(undefined); setDiffLoading(false);
    try { const value = await records.gitStatus(task.id, index); if (request === epoch.current) setStatus(value); }
    catch (reason) { if (request === epoch.current) setError(hostError(reason)); }
    finally { if (request === epoch.current) setLoading(false); }
  };
  useEffect(() => { void load(); return () => { epoch.current++; }; }, [task.id, root]);
  const open = async (change: GitChange, staged: boolean) => {
    const request = ++epoch.current;
    setError(undefined); setDiff(undefined); setDiffLoading(true);
    try { const value = await records.gitDiff(task.id, Number(root), change.path, staged, change.kind === 'untracked'); if (request === epoch.current) setDiff(value); }
    catch (reason) { if (request === epoch.current) setError(hostError(reason)); }
    finally { if (request === epoch.current) setDiffLoading(false); }
  };
  return <section className="inspector-section git-section" aria-label="Git 变更"><div className="inspector-heading"><div><h2><GitBranch size={15}/>版本变更</h2><p>只读 Git 视图，不会写入仓库</p></div><Button size="sm" variant="ghost" disabled={loading} onClick={() => void load()}><RefreshCw size={14}/>刷新</Button></div>
    {task.roots.length > 1 && <Select value={root} onValueChange={setRoot}><SelectTrigger className="w-full min-w-0 [&>span]:truncate" aria-label="选择任务目录"><SelectValue/></SelectTrigger><SelectContent>{task.roots.map((path, index) => <SelectItem value={String(index)} key={path}>{index === 0 ? `主目录 · ${path}` : `附加目录 · ${path}`}</SelectItem>)}</SelectContent></Select>}
    {error && <ErrorNotice error={error}/>} {loading ? <div className="inspector-skeleton" role="status"><span/><span/><span/></div> : !status ? null : !status.available ? <p className="inspector-empty">所选目录不是 Git 仓库。</p> : <>{status.branch && <p className="git-branch">分支：<code>{status.branch}</code></p>}{!status.changes.length ? <p className="inspector-empty">工作区干净，没有可显示的变更。</p> : <div className="git-list">{status.changes.flatMap(change => [<ChangeButton key={`${change.path}:staged`} change={change} staged onOpen={open}/>, <ChangeButton key={`${change.path}:worktree`} change={change} staged={false} onOpen={open}/>])}</div>}</>}
    {diffLoading && <p role="status">正在加载差异…</p>}
    {diff && <div className="git-diff"><div><strong>{diff.path}</strong>{diff.binary && <span className="binary"><Binary size={13}/>二进制文件</span>}</div>{diff.binary ? <p>Git 将此变更识别为二进制内容，不能在此显示文本 diff。</p> : diff.text ? <pre>{diff.text}</pre> : <p>该视图没有可显示的文本差异。</p>}</div>}
  </section>;
}

export function WorkbenchInspector({task, run, busy, onRefreshUsage}: {task: TaskRecord; run?: TaskSnapshot; busy: boolean; onRefreshUsage: () => void}) {
  return <aside className="workbench-inspector" aria-label="工作台详情"><Usage usage={run?.usage} canRefresh={!!run && ['ready', 'idle', 'interrupted'].includes(run.status)} busy={busy} onRefresh={onRefreshUsage}/><Tools key={run?.runId} tools={run?.tools}/><Git key={task.id + JSON.stringify(task.roots)} task={task}/></aside>;
}
