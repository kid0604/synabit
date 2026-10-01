import { describe, it, expect, beforeEach } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useAppStore } from '../../stores/useAppStore';
import { confirmDelete, pendingDeleteQuestion, answerDeleteQuestion } from '../useConfirmDelete';

beforeEach(() => {
  setActivePinia(createPinia());
  pendingDeleteQuestion.value = null;
});

describe('the one question a delete may ask', () => {
  it('asks nothing when "Ask before deleting" is off, which is the default', async () => {
    expect(useAppStore().confirmBeforeDelete).toBe(false);
    await expect(confirmDelete({ name: 'Milk' })).resolves.toBe(true);
    expect(pendingDeleteQuestion.value).toBeNull();
  });

  it('asks, and waits for the answer, when the setting is on', async () => {
    useAppStore().confirmBeforeDelete = true;
    const answer = confirmDelete({ name: 'Milk' });
    expect(pendingDeleteQuestion.value?.name).toBe('Milk');
    expect(pendingDeleteQuestion.value?.toTrash).toBe(true);
    answerDeleteQuestion(false);
    await expect(answer).resolves.toBe(false);
    expect(pendingDeleteQuestion.value).toBeNull();
  });

  it('answers an open question with no when a second one arrives', async () => {
    useAppStore().confirmBeforeDelete = true;
    const first = confirmDelete({ name: 'A' });
    const second = confirmDelete({ name: 'B' });
    await expect(first).resolves.toBe(false);
    answerDeleteQuestion(true);
    await expect(second).resolves.toBe(true);
  });
});
