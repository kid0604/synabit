package com.synabit.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.RemoteInput

/**
 * The text typed into the notification.
 *
 * A receiver rather than an activity, which is the whole point: nothing is
 * launched, nothing appears, and the user stays wherever they were. It writes
 * the capture down and puts the notification back with an empty field.
 */
class CaptureReplyReceiver : BroadcastReceiver() {

  override fun onReceive(context: Context, intent: Intent) {
    val typed = RemoteInput.getResultsFromIntent(intent)
      ?.getCharSequence(CaptureNotification.REPLY_KEY)
      ?.toString()
      ?.trim()

    if (!typed.isNullOrBlank()) {
      CaptureStore.write(context, typed, "notification")
    }

    // Re-posting is not optional. A notification that has been replied to sits
    // in a spinner until it is updated, so without this the capture box would
    // work exactly once.
    CaptureNotification.post(context)
  }
}
