import { describe, it, expect, vi } from 'vitest';
import { useThingsLock } from '../composables/useThingsLock';

const SECRET = 'Notes/secret.md';

function store(opts: { enabled?: boolean; protectedIds?: string[]; unlockedIds?: string[] } = {}) {
  const protectedIds = new Set(opts.protectedIds ?? [SECRET]);
  const unlockedIds = new Set(opts.unlockedIds ?? []);
  return {
    isEnabled: opts.enabled ?? true,
    isNoteProtected: vi.fn((id: string) => protectedIds.has(id)),
    isNoteAccessible: vi.fn((id: string) => !protectedIds.has(id) || unlockedIds.has(id)),
    unlockNote: vi.fn((id: string) => { unlockedIds.add(id); }),
    touchNoteSession: vi.fn(),
    toggleProtectedNote: vi.fn(async (id: string) => { protectedIds.add(id); }),
  };
}

describe('useThingsLock', () => {
  it('lets an unprotected node straight through', async () => {
    const lock = useThingsLock(store());
    const open = vi.fn();

    await lock.whenUnlocked('Notes/open.md', open);

    expect(open).toHaveBeenCalledOnce();
    expect(lock.pending.value).toBeNull();
  });

  // The whole point: Things opened a protected note on one click.
  it('holds a locked note until its PIN is given', async () => {
    const appLock = store();
    const lock = useThingsLock(appLock);
    const open = vi.fn();

    await lock.whenUnlocked(SECRET, open);
    expect(open).not.toHaveBeenCalled();
    expect(lock.pending.value?.id).toBe(SECRET);

    await lock.unlocked();
    expect(appLock.unlockNote).toHaveBeenCalledWith(SECRET);
    expect(open).toHaveBeenCalledOnce();
    expect(lock.pending.value).toBeNull();
  });

  it('does nothing at all when the PIN is not given', async () => {
    const lock = useThingsLock(store());
    const open = vi.fn();

    await lock.whenUnlocked(SECRET, open);
    lock.cancel();
    await lock.unlocked();

    expect(open).not.toHaveBeenCalled();
  });

  it('does not ask again inside an unlocked session', async () => {
    const lock = useThingsLock(store({ unlockedIds: [SECRET] }));
    const open = vi.fn();

    await lock.whenUnlocked(SECRET, open);

    expect(open).toHaveBeenCalledOnce();
  });

  it('asks for nothing when no PIN is set up', async () => {
    const lock = useThingsLock(store({ enabled: false }));
    const open = vi.fn();

    await lock.whenUnlocked(SECRET, open);

    expect(open).toHaveBeenCalledOnce();
  });

  // Nexus's rule: a protected note's text stays out of previews even unlocked.
  it('keeps a protected body out of previews, unlocked or not', () => {
    expect(useThingsLock(store()).hidesBody(SECRET)).toBe(true);
    expect(useThingsLock(store({ unlockedIds: [SECRET] })).hidesBody(SECRET)).toBe(true);
    expect(useThingsLock(store()).hidesBody('Notes/open.md')).toBe(false);
  });

  // A duplicate carries the body, so an unprotected copy is the lock removed.
  it('protects the copy of a protected note', async () => {
    const appLock = store();
    await useThingsLock(appLock).protectCopy(SECRET, 'Notes/copy.md');
    expect(appLock.toggleProtectedNote).toHaveBeenCalledWith('Notes/copy.md');
  });

  it('leaves the copy of an unprotected note alone', async () => {
    const appLock = store();
    await useThingsLock(appLock).protectCopy('Notes/open.md', 'Notes/copy.md');
    expect(appLock.toggleProtectedNote).not.toHaveBeenCalled();
  });
});
