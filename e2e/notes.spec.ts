import { test, expect } from '@playwright/test';
import { openApp } from './app';

/**
 * Faults the monkey test found, kept as plain steps so they stay found.
 */

test('a task ticked in a note that ends in a checklist is ticked, and nothing throws', async ({ page }) => {
  // The editor was made with the note in it, and TrailingNode's paragraph
  // after the final list waited for the first transaction: the tick's own
  // `focus()`. The tick's transaction no longer matched, threw "Applying a
  // mismatched transaction", and the tick was not saved.
  const seen = await openApp(page);
  await page.getByRole('button', { name: 'Notes' }).first().click();
  await page.getByText('Chào', { exact: true }).first().click();
  await page.waitForTimeout(600);
  await page.locator('input[type="checkbox"]').first().click();
  await page.waitForTimeout(300);
  const checked = await page.evaluate(
    () => (document.querySelector('.ProseMirror') as any).editor.getJSON().content.find((b: any) => b.type === 'taskList').content[0].attrs.checked,
  );
  expect(seen.thrown).toEqual([]);
  expect(checked).toBe(true);
});
