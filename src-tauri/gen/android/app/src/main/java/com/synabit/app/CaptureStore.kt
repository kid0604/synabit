package com.synabit.app

import android.content.Context
import org.json.JSONObject
import java.io.File

/**
 * Where a capture goes when the app is not running.
 *
 * Shared by every Android surface that takes text without the Tauri process
 * being alive — the no-window share activity, and the notification you can
 * type into. The capture queue itself lives in SQLite inside that process, so
 * reaching it from here would mean a JNI bridge and a second writer against
 * the same database file.
 *
 * This end is deliberately dumb instead: one small JSON file per capture, in
 * the app's own private directory. `commands::capture` on the Rust side moves
 * them into the real queue the next time the app runs.
 */
internal object CaptureStore {

  /** Read from Rust as `<app data dir>/pending-captures`. */
  const val HANDOFF_DIR = "pending-captures"

  private const val PREFS = "synabit_capture"
  private const val NEXT_SEQ = "next_seq"

  /**
   * Write one capture down. Returns false rather than throwing — every caller
   * is a surface that must finish cleanly whether or not this worked.
   */
  fun write(context: Context, text: String, source: String): Boolean = try {
    val dir = File(context.filesDir, HANDOFF_DIR).apply { mkdirs() }

    // Zero-padded so the filenames sort into arrival order, which is the
    // order the caps must be created in. A counter rather than a clock:
    // sharing several items lands them inside the same millisecond.
    val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    val seq = prefs.getLong(NEXT_SEQ, 0)
    prefs.edit().putLong(NEXT_SEQ, seq + 1).apply()

    val payload = JSONObject()
      .put("text", text)
      .put("source", source)
      .put("received_at", System.currentTimeMillis())

    // Written beside the target and renamed, so the reader never finds a file
    // holding half a capture.
    val target = File(dir, String.format("%012d.json", seq))
    val staging = File(dir, "${target.name}.part")
    staging.writeText(payload.toString())
    staging.renameTo(target)
  } catch (e: Exception) {
    false
  }
}
