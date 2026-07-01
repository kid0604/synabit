import { ref, onMounted } from 'vue'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'

/**
 * Composable for managing app auto-updates via Tauri updater plugin.
 *
 * Flow:
 * 1. On mount, auto-checks for updates after 10s delay (silent mode)
 * 2. If update found, exposes state for UI to show notification
 * 3. User triggers downloadAndInstall() → downloads with progress → installs → relaunches
 *
 * Usage:
 *   const { updateAvailable, updateVersion, downloadAndInstall, ... } = useAppUpdate()
 */
export function useAppUpdate() {

  // --- Reactive State ---
  const updateAvailable = ref(false)
  const updateVersion = ref('')
  const updateNotes = ref('')
  const isChecking = ref(false)
  const isDownloading = ref(false)
  const downloadProgress = ref(0) // 0-100
  const error = ref<string | null>(null)

  // Internal reference to the Update object (not reactive — it's a class instance)
  let pendingUpdate: Update | null = null

  /**
   * Check if a new version is available.
   * @param silent - If true, don't emit errors for network failures (used for auto-check)
   * @returns true if an update is available
   */
  async function checkForUpdates(silent = true): Promise<boolean> {
    if (isChecking.value) return false

    isChecking.value = true
    error.value = null

    try {
      const update = await check()

      if (update) {
        pendingUpdate = update
        updateAvailable.value = true
        updateVersion.value = update.version
        updateNotes.value = update.body ?? ''
        console.log(`[Update] New version available: ${update.version}`)
        return true
      } else {
        updateAvailable.value = false
        console.log('[Update] Already up to date.')
        return false
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      console.error('[Update] Check failed:', message)

      if (!silent) {
        error.value = message
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

      // Relaunch app to apply the update
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
   * Dismiss the update notification (user chose "Later")
   */
  function dismissUpdate() {
    updateAvailable.value = false
    pendingUpdate = null
  }

  // Auto-check for updates 10 seconds after app starts
  // Uses silent mode — no error shown if offline or check fails
  onMounted(() => {
    const timer = setTimeout(() => {
      checkForUpdates(true)
    }, 10_000)

    // Cleanup if component unmounts before timer fires
    return () => clearTimeout(timer)
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

    // Actions
    checkForUpdates,
    downloadAndInstall,
    dismissUpdate,
  }
}
