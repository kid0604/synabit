import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createTestingPinia } from '@pinia/testing';
import PeopleApp from '../PeopleApp.vue';
import * as core from '@tauri-apps/api/core';
import * as dialog from '@tauri-apps/plugin-dialog';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  convertFileSrc: (p: string) => p,
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn(),
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn().mockResolvedValue(true),
  message: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('vue-router', () => ({
  useRoute: () => ({ query: {} }),
  useRouter: () => ({ replace: vi.fn(), push: vi.fn() }),
}));
vi.mock('vue-i18n', async (importOriginal) => ({
  ...(await importOriginal<typeof import('vue-i18n')>()),
  useI18n: () => ({ t: (key: string) => key, locale: { value: 'en' } }),
}));

const summary = (id: string, title: string, properties: any = {}) => ({
  id,
  node_type: 'person',
  title,
  preview: '',
  properties,
  created_at: '2026-01-01 00:00:00',
  updated_at: '2026-01-01 00:00:00',
  timestamp: 1767225600000,
});

/** Calls to one IPC command, newest last. */
const callsTo = (cmd: string) =>
  vi.mocked(core.invoke).mock.calls.filter(([c]) => c === cmd).map(([, args]) => args as any);

const mountApp = () =>
  mount(PeopleApp, {
    props: { vaultPath: '/mock/vault' },
    global: {
      plugins: [createTestingPinia({ createSpy: vi.fn })],
      stubs: {
        GraphTab: true,
        TimelineTab: true,
        NotesTab: true,
        OverviewTab: true,
        PersonModal: true,
        GiftModal: true,
        LinkPersonModal: true,
        NavButtons: true,
      },
      mocks: { $t: (key: string) => key },
    },
  });

describe('PeopleApp', () => {
  let people: any[];

  beforeEach(() => {
    vi.clearAllMocks();
    people = [
      summary('People/owner.md', 'Me', { is_owner: true }),
      summary('People/an.md', 'An', {
        connections: [{ person_id: 'People/binh.md', relation_type: 'friend' }],
        relations: ['[Binh](synabit://person/People/binh.md)'],
      }),
      summary('People/binh.md', 'Binh', {
        connections: [{ person_id: 'People/an.md', relation_type: 'friend' }],
      }),
    ];

    vi.mocked(core.invoke).mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'get_node_summaries') return Promise.resolve(people);
      if (cmd === 'get_node') {
        const found = people.find(p => p.id === args.id);
        return Promise.resolve(found ? { ...found, content: `body of ${found.title}` } : null);
      }
      if (cmd === 'get_linked_nodes') return Promise.resolve([]);
      if (cmd === 'last_contact_dates') return Promise.resolve({ 'People/an.md': '2026-08-20' });
      return Promise.resolve(undefined);
    });
  });

  it('asks for summaries, not whole people, to draw the list', async () => {
    const wrapper = mountApp();
    await flushPromises();

    // People, and the saved views over them. Both are summaries; neither
    // sends a body the list would not show.
    expect(callsTo('get_node_summaries')).toEqual([
      { nodeType: 'person' },
      { nodeType: 'filter' },
    ]);
    // `get_nodes` sends every body; the list shows none of them.
    expect(callsTo('get_nodes')).toEqual([]);
    wrapper.unmount();
  });

  it('leaves the finance history alone until the Timeline asks for it', async () => {
    // Reading every month the vault has recorded, to pull out one person's
    // few transactions, used to happen on the way in.
    const wrapper = mountApp();
    await flushPromises();

    expect(callsTo('get_nodes')).toEqual([]);
    wrapper.unmount();
  });

  it('fetches the body when a person is opened', async () => {
    const wrapper = mountApp();
    await flushPromises();

    await (wrapper.vm as any).selectPerson(people[1]);
    await flushPromises();

    expect(callsTo('get_node')).toEqual([{ id: 'People/an.md' }]);
    // The Notes tab renders this; the summary in the list does not carry it.
    expect((wrapper.vm as any).selectedPerson.content).toBe('body of An');
    wrapper.unmount();
  });

  it('strips a deleted person out of everybody else who linked to them', async () => {
    const wrapper = mountApp();
    await flushPromises();

    await (wrapper.vm as any).deletePerson(people[2]); // Binh
    await flushPromises();

    // No question, and nothing written until the undo window closes.
    expect(vi.mocked(dialog.ask)).not.toHaveBeenCalled();
    expect(callsTo('delete_node_file')).toEqual([]);
    expect((wrapper.vm as any).people.map((p: any) => p.id)).not.toContain('People/binh.md');

    await (wrapper.vm as any).personUndo.commit();
    await flushPromises();

    // The person goes to the trash first — the step most likely to fail,
    // and the one the tidying up after it depends on.
    const order = vi.mocked(core.invoke).mock.calls
      .map(([c]) => c)
      .filter(c => c === 'trash_node_file' || c === 'write_node_file' || c === 'delete_node_file');
    expect(order[0]).toBe('trash_node_file');
    expect(callsTo('trash_node_file').map(a => a.relPath)).toEqual(['People/binh.md']);

    // An held the other end of the link.
    const writes = callsTo('write_node_file');
    expect(writes).toHaveLength(1);
    expect(writes[0].relPath).toBe('People/an.md');
    expect(writes[0].properties.connections).toBeNull();
    expect(writes[0].properties.relations).toBeNull();

    // Their birthday entry on the calendar goes with them. Nothing used to
    // clear it, so a deleted person's birthday came round every year forever.
    expect(callsTo('delete_node_file').map(a => a.relPath)).toEqual([
      'Events/birthday-people-binh.md',
    ]);
    wrapper.unmount();
  });

  it('touches nobody else, and puts them back whole, when the trash fails', async () => {
    const base = vi.mocked(core.invoke).getMockImplementation()!;
    vi.mocked(core.invoke).mockImplementation((cmd: string, args?: any) =>
      cmd === 'trash_node_file' ? Promise.reject(new Error('disk full')) : base(cmd, args));
    const wrapper = mountApp();
    await flushPromises();

    await (wrapper.vm as any).deletePerson(people[2]);
    await (wrapper.vm as any).personUndo.commit();
    await flushPromises();

    // Links and birthday were left alone, so the person who came back still
    // has both.
    expect(callsTo('write_node_file')).toEqual([]);
    expect(callsTo('delete_node_file')).toEqual([]);
    expect((wrapper.vm as any).people.map((p: any) => p.id)).toContain('People/binh.md');
    wrapper.unmount();
  });

  it('reads the same last-contact dates the reminder engine works from', async () => {
    // The dot beside somebody's name and the notification about them have to
    // count from the same day, or they disagree about the same person.
    const wrapper = mountApp();
    await flushPromises();

    expect(callsTo('last_contact_dates')).toHaveLength(1);
    wrapper.unmount();
  });

  it('deletes nothing, and puts them back, when Undo is pressed', async () => {
    const wrapper = mountApp();
    await flushPromises();

    await (wrapper.vm as any).deletePerson(people[2]);
    await flushPromises();
    (wrapper.vm as any).personUndo.undo();
    await (wrapper.vm as any).personUndo.commit();
    await flushPromises();

    expect((wrapper.vm as any).people.map((p: any) => p.id)).toContain('People/binh.md');
    expect(callsTo('delete_node_file')).toEqual([]);
    expect(callsTo('write_node_file')).toEqual([]);
    wrapper.unmount();
  });

  it('refuses to delete the vault owner', async () => {
    const wrapper = mountApp();
    await flushPromises();

    await (wrapper.vm as any).deletePerson(people[0]);
    await (wrapper.vm as any).personUndo.commit();
    await flushPromises();

    expect(callsTo('delete_node_file')).toEqual([]);
    wrapper.unmount();
  });

  it('stops listening for resizes once it is gone', async () => {
    const added = vi.spyOn(window, 'addEventListener');
    const removed = vi.spyOn(window, 'removeEventListener');

    const wrapper = mountApp();
    await flushPromises();
    wrapper.unmount();

    const resizeHandler = added.mock.calls.find(([type]) => type === 'resize')?.[1];
    expect(resizeHandler, 'a resize listener is registered').toBeTruthy();
    expect(removed.mock.calls.some(([type, fn]) => type === 'resize' && fn === resizeHandler))
      .toBe(true);

    added.mockRestore();
    removed.mockRestore();
  });
});
