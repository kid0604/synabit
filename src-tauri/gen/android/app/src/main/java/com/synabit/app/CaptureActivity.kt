package com.synabit.app

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.widget.Toast

/**
 * Capture without opening the app.
 *
 * Routing shares and text selections through MainActivity worked, but it
 * launched the whole app to save one sentence. For a share that is merely
 * jarring; for a text selection it defeats the purpose — the user was
 * reading, and wanted to keep reading.
 *
 * This activity has no window at all. It reads the intent, writes the text
 * down, shows a toast and finishes, so the user never leaves what they were
 * looking at.
 *
 * # Why files rather than the queue in `kv_store`
 *
 * The capture queue lives in SQLite, inside the Tauri process, which is not
 * running: this activity is started by another app's intent and the rest of
 * Synabit may be nowhere in memory. Reaching that database from here would
 * mean a JNI bridge and a second writer against the same file.
 *
 * So this end of the handoff is deliberately dumb — one small JSON file per
 * capture, in the app's own private directory. `commands::capture` reads
 * them and moves them into the real queue the next time the app runs. Two
 * steps, but each is a plain file write that cannot half-happen.
 */
class CaptureActivity : Activity() {

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)

    val text = readCapture(intent)
    val saved = text != null && writeHandoff(text, sourceOf(intent))

    Toast.makeText(
      this,
      if (saved) R.string.capture_saved else R.string.capture_empty,
      Toast.LENGTH_SHORT
    ).show()

    // Nothing is returned to the caller even for an editable selection: this
    // reads text, it does not edit it, and a result would replace whatever the
    // user had highlighted.
    finish()
  }

  private fun sourceOf(intent: Intent?): String =
    if (intent?.action == Intent.ACTION_PROCESS_TEXT) "selected-text" else "share-sheet"

  /** The words to keep, or null if this intent carries none. */
  private fun readCapture(intent: Intent?): String? {
    if (intent == null) return null

    val shared = when (intent.action) {
      Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)

      // Several items at once become one cap: a person sharing three things
      // in one gesture meant one thought.
      Intent.ACTION_SEND_MULTIPLE ->
        intent.getCharSequenceArrayListExtra(Intent.EXTRA_TEXT)
          ?.map { it.toString() }
          ?.filter { it.isNotBlank() }
          ?.joinToString("\n\n")

      Intent.ACTION_PROCESS_TEXT ->
        intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT)?.toString()

      else -> null
    }

    // A shared article puts its headline in EXTRA_SUBJECT and the link in
    // EXTRA_TEXT. Keeping both is the difference between a cap that reads
    // like a note and one that is a bare URL nobody will revisit.
    val subject = intent.getStringExtra(Intent.EXTRA_SUBJECT)
    val text = listOfNotNull(subject, shared)
      .filter { it.isNotBlank() }
      .distinct()
      .joinToString("\n\n")

    return text.ifBlank { null }
  }

  private fun writeHandoff(text: String, source: String): Boolean =
    CaptureStore.write(this, text, source)
}
