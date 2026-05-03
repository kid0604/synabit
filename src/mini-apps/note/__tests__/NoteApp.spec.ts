import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createTestingPinia } from '@pinia/testing';
import NoteApp from '../NoteApp.vue';
import * as core from '@tauri-apps/api/core';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn()
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn()
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn().mockResolvedValue(true)
}));

describe('NoteApp.vue', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('scans the vault on mount if vaultPath is provided', async () => {
    // Mock the backend scan_vault_path response
    const mockNotes = [
      { id: '1', title: 'Test Note 1', path: '/vault/note1.md', tags: ['work'], date: '2026-05-01', content: 'hello', pinned: false, full_width: false }
    ];
    
    vi.mocked(core.invoke).mockImplementation((cmd) => {
      if (cmd === 'scan_vault_path') return Promise.resolve(mockNotes);
      if (cmd === 'read_note') return Promise.resolve('Note Content');
      if (cmd === 'get_note_backlinks') return Promise.resolve([]);
      return Promise.resolve();
    });

    const wrapper = mount(NoteApp, {
      props: {
        vaultPath: '/mock/vault'
      },
      global: {
        plugins: [createTestingPinia({ createSpy: vi.fn })],
        stubs: {
          TiptapEditor: true,
          NoteGraph: true,
          'lucide-vue-next': true
        }
      }
    });

    // Wait for async operations to complete
    await new Promise(resolve => setTimeout(resolve, 100));

    // It should have called invoke('scan_vault_path')
    expect(core.invoke).toHaveBeenCalledWith('scan_vault_path', { vaultPath: '/mock/vault' });

    // Component exposes notes array
    const exposed = wrapper.vm as any;
    expect(exposed.notes.length).toBe(1);
    expect(exposed.notes[0].title).toBe('Test Note 1');
    
    // Automatically selects the first note if none is selected
    expect(exposed.currentNoteId).toBe('1');
  });
});
