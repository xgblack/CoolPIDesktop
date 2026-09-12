import {useState} from 'react';
import type {ApprovalMode} from '../../../../../packages/host-contract/src';

const key = 'omp-default-approval-mode';

export function useApprovalPreference() {
  const [saved] = useState(() => {
    try {
      const value = localStorage.getItem(key);
      if (value === null || value === 'inherit') return {mode: null, error: ''};
      if (value === 'always-ask' || value === 'write' || value === 'yolo') return {mode: value as ApprovalMode, error: ''};
      return {mode: null, error: '保存的审批偏好无效，请重新选择。'};
    } catch {
      return {mode: null, error: '无法读取审批偏好，请重新选择。'};
    }
  });
  const [mode, setMode] = useState<ApprovalMode | null>(saved.mode);
  const change = (next: ApprovalMode | null) => {
    localStorage.setItem(key, next ?? 'inherit');
    setMode(next);
  };
  return {mode, change, error: saved.error};
}
