import { ref, onUnmounted } from 'vue'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { load, type Store } from '@tauri-apps/plugin-store'

/**
 * Composable for managing app auto-updates via Tauri updater plugin.
 *
 * Uses SINGLETON pattern — all callers share the same reactive state.
 * This prevents duplicate auto-checks and ensures consistent state across
 * App.vue (banner) and SettingsModal.vue (check button).
 *
 * Flow:
 * 1. First caller triggers auto-check after 10s delay (silent mode)
 * 2. If update found, exposes state for UI to show notification
 * 3. User triggers downloadAndInstall() → downloads with progress → installs → relaunches
 * 4. "Later" saves skipped version to persistent store — won't auto-nag again
 */

// ─── Module-level Singleton State ──────────────────────────
const updateAvailable = ref(false)
const updateVersion = ref('')
const updateNotes = ref('')
const isChecking = ref(false)
const isDownloading = ref(false)
const downloadProgress = ref(0)
const error = ref<string | null>(null)
const lastCheckResult = ref<'up-to-date' | 'error' | null>(null)

let pendingUpdate: Update | null = null
let resultClearTimer: ReturnType<typeof setTimeout> | null = null
let autoCheckTimer: ReturnType<typeof setTimeout> | null = null
let initialized = false
let settingsStore: Store | null = null

const SKIPPED_VERSION_KEY = 'update_skipped_version'

// ─── Internal Helpers ──────────────────────────────────────

async function getStore(): Promise<Store> {
  if (!settingsStore) {
    settingsStore = await load('settings.json')
  }
  return settingsStore
}

async function getSkippedVersion(): Promise<string | null> {
  try {
    const store = await getStore()
    return await store.get<string>(SKIPPED_VERSION_KEY) ?? null
  } catch {
    return null
  }
}

async function setSkippedVersion(version: string): Promise<void> {
  try {
    const store = await getStore()
    await store.set(SKIPPED_VERSION_KEY, version)
    await store.save()
  } catch (e) {
    console.warn('[Update] Could not save skipped version:', e)
  }
}

async function clearSkippedVersion(): Promise<void> {
  try {
    const store = await getStore()
    await store.delete(SKIPPED_VERSION_KEY)
    await store.save()
  } catch {
    // ignore
  }
}

function autoCleanResult() {
  if (resultClearTimer) clearTimeout(resultClearTimer)
  resultClearTimer = setTimeout(() => {
    lastCheckResult.value = null
  }, 5000)
}

// ─── Exported Composable ───────────────────────────────────

export function useAppUpdate() {

  /**
   * Check if a new version is available.
   * @param silent - If true, don't emit errors for network failures (auto-check)
   * @param ignoreSkipped - If true, show update even if user previously skipped this version
   */
  async function checkForUpdates(silent = true, ignoreSkipped = false): Promise<boolean> {
    if (isChecking.value) return false

    isChecking.value = true
    error.value = null

    try {
      const update = await check()

      if (update) {
        // Check if user previously skipped this version (only for auto-checks)
        if (!ignoreSkipped && silent) {
          const skipped = await getSkippedVersion()
          if (skipped === update.version) {
            console.log(`[Update] Version ${update.version} was skipped by user.`)
            return false
          }
        }

        pendingUpdate = update
        updateAvailable.value = true
        updateVersion.value = update.version
        updateNotes.value = update.body ?? ''
        lastCheckResult.value = null
        console.log(`[Update] New version available: ${update.version}`)
        return true
      } else {
        updateAvailable.value = false
        if (!silent) {
          lastCheckResult.value = 'up-to-date'
          autoCleanResult()
        }
        console.log('[Update] Already up to date.')
        return false
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      console.error('[Update] Check failed:', message)

      if (!silent) {
        error.value = message
        lastCheckResult.value = 'error'
        autoCleanResult()
      }
      return false
    } finally {
      isChecking.value = false
    }
  }

  /**
   * Download and install the pending update, then relaunch the app.
   * This function will NOT return if successful (app restarts).
   */
  async function downloadAndInstall() {
    if (!pendingUpdate) {
      error.value = 'No pending update'
      return
    }

    isDownloading.value = true
    downloadProgress.value = 0
    error.value = null

    // Clear skipped version — user actively chose to install
    await clearSkippedVersion()

    try {
      let totalBytes = 0
      let downloadedBytes = 0

      await pendingUpdate.downloadAndInstall((event) => {
        if (event.event === 'Started' && event.data.contentLength) {
          totalBytes = event.data.contentLength
          console.log(`[Update] Downloading ${(totalBytes / 1024 / 1024).toFixed(1)} MB...`)
        } else if (event.event === 'Progress') {
          downloadedBytes += event.data.chunkLength
          if (totalBytes > 0) {
            downloadProgress.value = Math.round((downloadedBytes / totalBytes) * 100)
          }
        } else if (event.event === 'Finished') {
          downloadProgress.value = 100
          console.log('[Update] Download complete. Installing...')
        }
      })

      console.log('[Update] Relaunching app...')
      await relaunch()
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      console.error('[Update] Download/install failed:', message)
      error.value = message
      isDownloading.value = false
    }
  }

  /**
   * Dismiss the update notification (user chose "Later").
   * Saves the skipped version so auto-check won't nag again.
   */
  async function dismissUpdate() {
    if (updateVersion.value) {
      await setSkippedVersion(updateVersion.value)
    }
    updateAvailable.value = false
    pendingUpdate = null
  }

  // Auto-check 10s after first mount — only once (singleton guard)
  if (!initialized) {
    initialized = true
    autoCheckTimer = setTimeout(() => {
      checkForUpdates(true)
    }, 10_000)
  }

  // Clean up timer if the component using this composable unmounts
  onUnmounted(() => {
    if (autoCheckTimer) {
      clearTimeout(autoCheckTimer)
      autoCheckTimer = null
    }
    if (resultClearTimer) {
      clearTimeout(resultClearTimer)
      resultClearTimer = null
    }
  })

  return {
    // State (readonly to consumers)
    updateAvailable,
    updateVersion,
    updateNotes,
    isChecking,
    isDownloading,
    downloadProgress,
    error,
    lastCheckResult,

    // Actions
    checkForUpdates,
    downloadAndInstall,
    dismissUpdate,
  }
}
