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
    // Mock the backend get_nodes response (Node Core architecture)
    const mockNodes = [
      {
        id: 'Notes/note1.md',
        node_type: 'note',
        title: 'Test Note 1',
        content: 'hello',
        properties: { tags: ['work'], pinned: false, full_width: false },
        created_at: '2026-05-01 00:00:00',
        updated_at: '2026-05-01 00:00:00',
        timestamp: 1746057600000
      }
    ];
    
    vi.mocked(core.invoke).mockImplementation((cmd) => {
      if (cmd === 'get_nodes') return Promise.resolve(mockNodes);
      if (cmd === 'get_linked_nodes') return Promise.resolve([]);
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

    // It should have called invoke('get_nodes') with nodeType filter
    expect(core.invoke).toHaveBeenCalledWith('get_nodes', { nodeType: 'note' });

    // Component exposes notes array
    const exposed = wrapper.vm as any;
    expect(exposed.notes.length).toBe(1);
    expect(exposed.notes[0].title).toBe('Test Note 1');
    
    // Automatically selects the first note if none is selected
    expect(exposed.currentNoteId).toBe('Notes/note1.md');
  });
});
