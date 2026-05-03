import { test, expect } from '@playwright/test';

test('App loads and shows Welcome Screen when no vault is selected', async ({ page }) => {
  // Add minimal mock for the logger and basic settings to prevent crashes
  await page.addInitScript(() => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      value: {
        invoke: async (cmd: string) => {
          if (cmd === 'get_settings') {
            return {
              vault_path: '',
              theme: 'system',
              font_size: 14,
              font_family: 'Inter',
              window_width: 1024,
              window_height: 768
            };
          }
          if (cmd === 'plugin:log|log') return null;
          return null;
        }
      }
    });

    Object.defineProperty(window, '__TAURI__', {
      value: {
        event: {
          listen: async () => () => {},
          emit: async () => {}
        }
      }
    });
  });

  // Wait for the app to load
  await page.goto('/');

  // Should show the welcome screen
  await expect(page.locator('text=Welcome to Synabit')).toBeVisible({ timeout: 10000 });
  await expect(page.locator('text=Choose how you want to store your vault.')).toBeVisible();

  // Should have the Local Folder and Google Drive buttons
  await expect(page.locator('text=Local Folder')).toBeVisible();
  await expect(page.locator('text=Google Drive')).toBeVisible();
});
