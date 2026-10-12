package com.synabit.app

import android.app.Notification
import android.content.Context
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/**
 * The `android.notifications` bridge of Syn's device connectors.
 *
 * Keeps the notifications of the apps the person chose — and only those — so
 * a balance change from their bank or a message they were sent can be read
 * later, by them or by Syn when they ask. Nothing else is kept: every other
 * app's notification is seen by the system and dropped here unread.
 *
 * The list of apps is written by Rust (`connector::device::sync_packages`)
 * from the device connectors approved on this device, after the approval
 * card has said what is kept and why. The package name is the system's, so
 * a notification cannot claim to be from an app it is not.
 *
 * This end is dumb on purpose, like `CaptureStore`: one line of JSON per
 * notification, appended to a file in the app's private directory, which
 * `connector::device` reads. The Tauri process may not be running when a
 * notification arrives.
 */
class SynNotificationListener : NotificationListenerService() {

  override fun onNotificationPosted(sbn: StatusBarNotification) {
    try {
      val wanted = packages(this)
      if (!wanted.contains(sbn.packageName)) return
      val extras = sbn.notification.extras
      val title = extras.getCharSequence(Notification.EXTRA_TITLE)?.toString() ?: ""
      val text = (extras.getCharSequence(Notification.EXTRA_BIG_TEXT)
        ?: extras.getCharSequence(Notification.EXTRA_TEXT))?.toString() ?: ""
      if (title.isEmpty() && text.isEmpty()) return
      // A group summary repeats its children.
      if (sbn.notification.flags and Notification.FLAG_GROUP_SUMMARY != 0) return
      val line = JSONObject()
        .put("id", "${sbn.key}@${sbn.postTime}")
        .put("package", sbn.packageName)
        .put("title", title)
        .put("text", text)
        .put("posted_at", sbn.postTime)
      append(this, line.toString())
    } catch (e: Exception) {
      // A notification that could not be kept is not worth crashing the
      // listener over; the system would stop delivering the rest.
    }
  }

  companion object {
    /** Read from Rust as `<app data dir>/syn-notifications.jsonl`. */
    const val STORE = "syn-notifications.jsonl"
    /** Written by Rust: the packages whose notifications are kept. */
    const val PACKAGES = "syn-notification-packages.json"
    /** Past this, the oldest half is let go. */
    private const val MAX_BYTES = 4L * 1024 * 1024

    private var cached: Pair<Long, Set<String>>? = null

    fun packages(context: Context): Set<String> {
      val f = File(context.filesDir, PACKAGES)
      if (!f.exists()) return emptySet()
      val stamp = f.lastModified()
      cached?.let { if (it.first == stamp) return it.second }
      val list = JSONArray(f.readText())
      val out = (0 until list.length()).map { list.getString(it) }.toSet()
      cached = stamp to out
      return out
    }

    @Synchronized
    fun append(context: Context, line: String) {
      val f = File(context.filesDir, STORE)
      if (f.exists() && f.length() > MAX_BYTES) {
        val lines = f.readLines()
        f.writeText(lines.drop(lines.size / 2).joinToString("\n", postfix = "\n"))
      }
      f.appendText(line.replace("\n", " ") + "\n")
    }
  }
}
