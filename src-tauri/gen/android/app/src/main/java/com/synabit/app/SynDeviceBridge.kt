package com.synabit.app

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.provider.Settings

/**
 * What Rust asks of the device bridges through JNI (`connector::device`).
 * Reached by name, so `proguard-rules.pro` keeps it.
 */
object SynDeviceBridge {

  /** Whether the person has given Synabit notification access in Settings. */
  @JvmStatic
  fun notificationAccess(context: Context): Boolean {
    val enabled = Settings.Secure.getString(context.contentResolver, "enabled_notification_listeners") ?: return false
    val mine = ComponentName(context, SynNotificationListener::class.java).flattenToString()
    return enabled.split(':').any { it == mine }
  }

  /** Open the system screen where notification access is given. */
  @JvmStatic
  fun openNotificationAccess(context: Context): Boolean = try {
    val intent = Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    context.startActivity(intent)
    true
  } catch (e: Exception) {
    false
  }
}
