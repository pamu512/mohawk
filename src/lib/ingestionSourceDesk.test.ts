import { describe, expect, it } from 'vitest';
import { ingestionSourceDeskPresentation } from './ingestionSourceDesk';

const marked = {
  label: 'Consumer Safety Briefings',
  known_not_serving_rss: true,
};

const unmarked = {
  label: 'CISA Alert Feeds',
  known_not_serving_rss: false,
};

describe('ingestionSourceDeskPresentation', () => {
  it('shows a known-not-serving note for a marked source and never uses the idle green dot', () => {
    const idle = ingestionSourceDeskPresentation(marked, false);
    const syncing = ingestionSourceDeskPresentation(marked, true);

    expect(idle.dotClass).not.toContain('bg-emerald-500');
    expect(syncing.dotClass).not.toContain('bg-emerald-500');
    expect(idle.note).toBe(
      'Consumer Safety Briefings is not serving RSS (known not serving, not a live health check).',
    );
    expect(syncing.note).toBe(idle.note);
  });

  it('keeps an unmarked idle source visually neutral, not verified-live green', () => {
    const idle = ingestionSourceDeskPresentation(unmarked, false);

    expect(idle.dotClass).not.toContain('bg-emerald-500');
    expect(idle.note).toBeNull();
  });

  it('pulses cyan for an unmarked source only while a sync is in progress', () => {
    const syncing = ingestionSourceDeskPresentation(unmarked, true);

    expect(syncing.dotClass).toContain('bg-cyan-400');
    expect(syncing.dotClass).toContain('animate-pulse');
    expect(syncing.note).toBeNull();
  });
});
