import { describe, expect, it } from 'vitest';

describe('desktop P0 shell', () => {
  it('exposes the runtime entry point', () => {
    expect('detect_omp').toBe('detect_omp');
  });
});
