// @vitest-environment jsdom
import {afterEach, expect, it, vi} from 'vitest';
import {act, cleanup, renderHook} from '@testing-library/react';
import {useApprovalPreference} from './approval-preference';

afterEach(() => {cleanup(); localStorage.clear(); vi.restoreAllMocks();});

it('defaults to inherit and restores the last explicit selection across app mounts', () => {
  const first = renderHook(useApprovalPreference);
  expect(first.result.current.mode).toBeNull();
  act(() => first.result.current.change('yolo'));
  first.unmount();
  const second = renderHook(useApprovalPreference);
  expect(second.result.current.mode).toBe('yolo');
  act(() => second.result.current.change('write'));
  second.unmount();
  const third = renderHook(useApprovalPreference);
  expect(third.result.current.mode).toBe('write');
  act(() => third.result.current.change(null));
  expect(renderHook(useApprovalPreference).result.current.mode).toBeNull();
});

it('reports invalid stored data and does not silently accept failed writes', () => {
  localStorage.setItem('omp-default-approval-mode', 'invalid');
  const hook = renderHook(useApprovalPreference);
  expect(hook.result.current.mode).toBeNull();
  expect(hook.result.current.error).toBeTruthy();
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {throw new Error('storage blocked');});
  expect(() => act(() => hook.result.current.change('yolo'))).toThrow('storage blocked');
  expect(hook.result.current.mode).toBeNull();
});
