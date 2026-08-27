import { describe, it, expect } from 'vitest';
import {
  backupReminderReason,
  daysSince,
  EXPORT_REMINDER_DAYS,
  formatBytes,
} from '../useVaultArchive';

const NOW = new Date('2026-08-18T12:00:00Z');
const daysAgo = (n: number) => new Date(NOW.getTime() - n * 86_400_000);

/**
 * When to tell somebody their notes are one uninstall away from gone. Getting
 * this wrong in the quiet direction loses a vault; getting it wrong in the
 * noisy direction teaches people to ignore the banner, which loses a vault
 * later.
 */
describe('backup reminder', () => {
  it('says nothing on the desktop, whatever the history', () => {
    expect(backupReminderReason(null, NOW, false)).toBe(null);
    expect(backupReminderReason(daysAgo(9999), NOW, false)).toBe(null);
  });

  it('asks for a first backup when there has never been one', () => {
    expect(backupReminderReason(null, NOW, true)).toBe('never');
  });

  it('stays quiet while a backup is recent', () => {
    expect(backupReminderReason(daysAgo(0), NOW, true)).toBe(null);
    expect(backupReminderReason(daysAgo(EXPORT_REMINDER_DAYS - 1), NOW, true)).toBe(null);
  });

  it('speaks up on the day the backup goes stale, and after', () => {
    expect(backupReminderReason(daysAgo(EXPORT_REMINDER_DAYS), NOW, true)).toBe('stale');
    expect(backupReminderReason(daysAgo(365), NOW, true)).toBe('stale');
  });

  /**
   * A timestamp in the future is a clock that moved, not a stale backup.
   * Reporting it as "backed up -3 days ago" would be nonsense on screen.
   */
  it('treats a future timestamp as recent rather than stale', () => {
    const tomorrow = new Date(NOW.getTime() + 86_400_000);
    expect(backupReminderReason(tomorrow, NOW, true)).toBe(null);
    expect(daysSince(tomorrow, NOW)).toBe(0);
  });

  it('counts whole days since the last backup', () => {
    expect(daysSince(daysAgo(30), NOW)).toBe(30);
    expect(daysSince(new Date(NOW.getTime() - 47 * 3_600_000), NOW)).toBe(1);
  });
});

describe('formatBytes', () => {
  it('scales to something a person reads', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2.0 KB');
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB');
  });
});
