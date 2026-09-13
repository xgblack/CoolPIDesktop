import {describe, expect, it} from 'vitest';
import {compactTokenLabel, completedAtLabel} from './turn-metadata';

describe('compactTokenLabel', () => {
  it('keeps small counts readable', () => expect(compactTokenLabel(999)).toBe('999'));
  it('compacts large counts while retaining useful precision', () => {
    expect(compactTokenLabel(24438)).toBe('24.4K');
    expect(compactTokenLabel(1234567)).toBe('1.2M');
  });
  it('uses a neutral label when usage is unavailable', () => expect(compactTokenLabel()).toBe('用量'));
});

describe('completedAtLabel', () => {
  const now = new Date(2026, 8, 13, 12, 0, 0).getTime();
  it('shows only the clock for today', () => expect(completedAtLabel(new Date(2026, 8, 13, 10, 16, 38).getTime(), now)).toBe('10:16:38'));
  it('labels yesterday explicitly', () => expect(completedAtLabel(new Date(2026, 8, 12, 10, 16, 38).getTime(), now)).toBe('昨日 10:16:38'));
  it('uses an absolute date for older turns', () => expect(completedAtLabel(new Date(2026, 8, 10, 10, 16, 38).getTime(), now)).toBe('2026-09-10 10:16:38'));
});
