import {Shield, LoaderCircle} from 'lucide-react';
import type {ApprovalMode} from '../../../../../packages/host-contract/src';
import {Button} from '@/components/ui/button';
import {DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuRadioGroup, DropdownMenuRadioItem} from '@/components/ui/dropdown-menu';

const modes = [
  {value: 'inherit', label: '跟随 OMP 配置', short: '跟随配置'},
  {value: 'always-ask', label: '写入与执行需审批', short: '写入审批'},
  {value: 'write', label: '执行需审批', short: '执行审批'},
  {value: 'yolo', label: '自动批准', short: '自动批准'},
] as const;

export function ApprovalModeSelect({value, onChange, disabled, switching}: {
  value?: ApprovalMode | null; onChange: (mode: ApprovalMode | null) => void;
  disabled?: boolean; switching?: boolean;
}) {
  const selected = value ?? 'inherit';
  const title = switching ? '正在切换审批模式' : modes.find(m => m.value === selected)?.label;
  return <DropdownMenu><DropdownMenuTrigger asChild>
    <Button type="button" variant="ghost" size="sm" aria-label="审批模式" title={title} disabled={disabled || switching} className={`approval-mode-trigger${selected === 'yolo' ? ' approval-mode-danger' : ''}`}>
      {switching ? <LoaderCircle className="animate-spin" size={15}/> : <Shield size={15}/>}
      <span>{switching ? '切换中' : modes.find(m => m.value === selected)?.short}</span>
    </Button>
  </DropdownMenuTrigger><DropdownMenuContent side="top" align="start">
    <DropdownMenuRadioGroup value={selected} onValueChange={mode => onChange(mode === 'inherit' ? null : mode as ApprovalMode)}>
      {modes.map(mode => <DropdownMenuRadioItem key={mode.value} value={mode.value} className={mode.value === 'yolo' ? 'approval-mode-danger' : undefined}>{mode.label}</DropdownMenuRadioItem>)}
    </DropdownMenuRadioGroup>
  </DropdownMenuContent></DropdownMenu>;
}
