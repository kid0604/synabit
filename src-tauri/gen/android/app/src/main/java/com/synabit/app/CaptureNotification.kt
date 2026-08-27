package com.synabit.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.RemoteInput

/**
 * A capture box that lives in the notification shade.
 *
 * Every other surface still ends with an app on screen: the share sheet and
 * the text-selection action need somewhere to have come from, the widget and
 * the launcher shortcut open the compose box. This one is the only place on
 * Android where a person can type a sentence and have it saved **without any
 * app being opened at all** — pull down the shade, tap, type, send.
 *
 * That makes it the shortest path there is between a thought and a note, and
 * the reason it is worth a persistent notification.
 *
 * # Why it is as quiet as a notification can be
 *
 * The channel is IMPORTANCE_MIN: no sound, no status-bar icon, collapsed at
 * the bottom of the shade. A permanent notification that announced itself
 * would be a tax on every other thing the user is notified about.
 */
internal object CaptureNotification {

  private const val CHANNEL_ID = "quickcap-capture"
  private const val NOTIFICATION_ID = 4201

  /** The key the typed text arrives under. */
  const val REPLY_KEY = "quickcap_reply"

  fun post(context: Context) {
    ensureChannel(context)

    val remoteInput = RemoteInput.Builder(REPLY_KEY)
      .setLabel(context.getString(R.string.notification_hint))
      .build()

    val replyIntent = Intent(context, CaptureReplyReceiver::class.java)
      .setPackage(context.packageName)

    val replyPending = PendingIntent.getBroadcast(
      context,
      0,
      replyIntent,
      // MUTABLE, not IMMUTABLE: the system has to write the typed text into
      // this intent before delivering it. An immutable one silently arrives
      // with no reply attached.
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE
    )

    val reply = NotificationCompat.Action.Builder(
      R.drawable.ic_shortcut_quickcap,
      context.getString(R.string.notification_action),
      replyPending
    )
      .addRemoteInput(remoteInput)
      // Nothing on screen changes when this is used, which is the point.
      .setShowsUserInterface(false)
      .build()

    // Tapping the body rather than the action opens the app's compose box —
    // for anything longer than a line, or when a picture is wanted.
    val openIntent = Intent(
      Intent.ACTION_VIEW,
      Uri.parse("com.synabit.app://quickcap/compose")
    ).setPackage(context.packageName)

    val openPending = PendingIntent.getActivity(
      context,
      1,
      openIntent,
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
    )

    val notification = NotificationCompat.Builder(context, CHANNEL_ID)
      .setSmallIcon(R.drawable.ic_shortcut_quickcap)
      .setContentTitle(context.getString(R.string.notification_title))
      .setContentText(context.getString(R.string.notification_hint))
      .setContentIntent(openPending)
      .addAction(reply)
      .setOngoing(true)
      .setSilent(true)
      .setPriority(NotificationCompat.PRIORITY_MIN)
      .setCategory(NotificationCompat.CATEGORY_REMINDER)
      // Visible on a locked screen without showing what was captured before.
      .setVisibility(NotificationCompat.VISIBILITY_SECRET)
      .build()

    try {
      NotificationManagerCompat.from(context).notify(NOTIFICATION_ID, notification)
    } catch (e: SecurityException) {
      // POST_NOTIFICATIONS not granted. The app asks for it elsewhere, at a
      // moment that makes sense; there is nothing useful to do here.
    }
  }

  private fun ensureChannel(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return

    val channel = NotificationChannel(
      CHANNEL_ID,
      context.getString(R.string.notification_channel),
      NotificationManager.IMPORTANCE_MIN
    ).apply {
      description = context.getString(R.string.notification_channel_desc)
      setShowBadge(false)
    }

    context.getSystemService(NotificationManager::class.java)
      ?.createNotificationChannel(channel)
  }
}
