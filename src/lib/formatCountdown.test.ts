import { describe, expect, it } from 'vitest';
import { formatCountdown } from './formatCountdown';

describe('formatCountdown', () => {
  it('returns due message at zero', () => {
    expect(formatCountdown(0)).toBe('DUE NOW — AWAITING CYCLE');
    expect(formatCountdown(-5)).toBe('DUE NOW — AWAITING CYCLE');
  });

  it('formats days hours minutes seconds', () => {
    const oneDayOneHour = 86_400 + 3_600 + 125;
    expect(formatCountdown(oneDayOneHour)).toBe('1d 01h 02m 05s');
  });
});
