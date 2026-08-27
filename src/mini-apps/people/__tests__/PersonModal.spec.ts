import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createTestingPinia } from '@pinia/testing';
import PersonModal from '../PersonModal.vue';
import * as core from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  convertFileSrc: (p: string) => p
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn()
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn().mockResolvedValue(true),
  message: vi.fn().mockResolvedValue(undefined)
}));
// See NoteApp.spec.ts: stubbing the whole module would take `createI18n` with
// it, and the suite fails at import time.
vi.mock('vue-i18n', async (importOriginal) => ({
  ...(await importOriginal<typeof import('vue-i18n')>()),
  useI18n: () => ({ t: (key: string) => key, locale: { value: 'en' } })
}));

/** A person as the list hands one over: saved once, with data in every field. */
const filledPerson = () => ({
  id: 'People/abc.md',
  title: 'Mai',
  content: 'Body typed in the note editor.',
  properties: {
    nickname: 'Mai Mai',
    display_name: 'fullname',
    relationship_type: 'Friend',
    birthday: '1994-03-02',
    tags: ['work'],
    details: [{ label: 'Email', value: 'mai@example.com', type: 'email' }],
    email: 'mai@example.com',
    important_dates: [{ label: 'Anniversary', date: '2020-06-01' }],
    experiences: [{ company: 'Acme', role: 'Dev', start: '2020', end: '', current: true }],
    // Owned by other screens, not by this form.
    interactions: [{ id: 'i1', type: 'coffee', date: '2026-08-01', note: 'cafe' }],
    last_contacted: '2026-08-01',
    connections: [{ person_id: 'People/xyz.md', name: 'Nam', relation_type: 'friend' }]
  }
});

const mountModal = (person: any) =>
  mount(PersonModal, {
    props: { vaultPath: '/mock/vault', person },
    global: {
      plugins: [createTestingPinia({ createSpy: vi.fn })],
      mocks: { $t: (key: string) => key }
    }
  });

/** The properties patch of the last `write_node_file` call. */
const lastWrite = () => {
  const calls = vi.mocked(core.invoke).mock.calls.filter(([cmd]) => cmd === 'write_node_file');
  expect(calls.length).toBeGreaterThan(0);
  return calls[calls.length - 1][1] as Record<string, any>;
};

const save = async (wrapper: any) => {
  const button = wrapper.findAll('button').find((b: any) => b.text().includes('Save Person'));
  expect(button).toBeTruthy();
  await button!.trigger('click');
  await new Promise((r) => setTimeout(r, 0));
};

describe('PersonModal — saving', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(core.invoke).mockResolvedValue(undefined);
  });

  it('names every cleared field with null so the removal reaches disk', async () => {
    const wrapper = mountModal(filledPerson());
    const vm = wrapper.vm as any;

    // Empty the form the way the user does: remove the last tag, the last
    // detail, the birthday, the dates, the jobs, the relationship.
    vm.form.tags = [];
    vm.form.details = [];
    vm.form.birthday = '';
    vm.form.important_dates = [];
    vm.form.experiences = [];
    vm.form.relationships = [];
    vm.form.nickname = '';
    await save(wrapper);

    const { properties } = lastWrite();
    // A write is a patch: `null` removes the key, a missing key keeps whatever
    // is on disk. Every one of these has to be named.
    for (const key of [
      'tags',
      'details',
      'birthday',
      'important_dates',
      'experiences',
      'relationship_type',
      'nickname'
    ]) {
      expect(properties, `${key} must be named`).toHaveProperty(key);
      expect(properties[key], `${key} must be null`).toBeNull();
    }
  });

  it('clears the search shortcuts along with the details they copy', async () => {
    const wrapper = mountModal(filledPerson());
    const vm = wrapper.vm as any;

    vm.form.details = [];
    await save(wrapper);

    // Search reads these. Leaving them behind keeps finding the person by an
    // address they deleted.
    const { properties } = lastWrite();
    expect(properties.email).toBeNull();
    expect(properties.phone).toBeNull();
    expect(properties.company).toBeNull();
  });

  it('round-trips the keep-in-touch cadence', async () => {
    const wrapper = mountModal(filledPerson());
    const vm = wrapper.vm as any;

    // Nothing set it before, which is what left every contact's relationship
    // health reading "unknown".
    expect(vm.form.contact_frequency).toBe('');

    vm.form.contact_frequency = 'monthly';
    await save(wrapper);
    expect(lastWrite().properties.contact_frequency).toBe('monthly');
  });

  it('reads an existing cadence back into the form', async () => {
    const person = filledPerson();
    person.properties = { ...person.properties, contact_frequency: 'quarterly' } as any;
    const wrapper = mountModal(person);
    expect((wrapper.vm as any).form.contact_frequency).toBe('quarterly');

    await save(wrapper);
    expect(lastWrite().properties.contact_frequency).toBe('quarterly');
  });

  it('turns the cadence off again when set back to no tracking', async () => {
    const person = filledPerson();
    person.properties = { ...person.properties, contact_frequency: 'weekly' } as any;
    const wrapper = mountModal(person);

    (wrapper.vm as any).form.contact_frequency = '';
    await save(wrapper);
    expect(lastWrite().properties.contact_frequency).toBeNull();
  });

  it('leaves fields owned by other screens out of the patch', async () => {
    const wrapper = mountModal(filledPerson());
    await save(wrapper);

    // `props.person` is as old as whenever the modal was opened. Sending these
    // back would revert an interaction logged in the meantime; omitting them
    // keeps what is on disk.
    const { properties } = lastWrite();
    expect(properties).not.toHaveProperty('interactions');
    expect(properties).not.toHaveProperty('last_contacted');
    expect(properties).not.toHaveProperty('connections');
  });

  it('sends no body, since this form does not edit one', async () => {
    const wrapper = mountModal(filledPerson());
    await save(wrapper);
    expect(lastWrite().content).toBeUndefined();
  });

  it('still writes the fields that do have values', async () => {
    const wrapper = mountModal(filledPerson());
    await save(wrapper);

    const { properties } = lastWrite();
    // A list now, so a relationship whose own name contains a comma stays one.
    expect(properties.relationship_type).toEqual(['Friend']);
    expect(properties.birthday).toBe('1994-03-02');
    expect(properties.tags).toEqual(['work']);
    expect(properties.details).toHaveLength(1);
    expect(properties.email).toBe('mai@example.com');
  });
});
