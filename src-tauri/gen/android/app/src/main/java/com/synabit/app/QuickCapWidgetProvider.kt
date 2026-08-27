package com.synabit.app

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.widget.RemoteViews

/**
 * The home-screen widget: a capture box that is already on screen.
 *
 * Every other Android surface still asks the user to go somewhere first —
 * open the share sheet, long-press the launcher icon, select some text. This
 * one is simply there, and one tap in.
 *
 * A widget cannot hold a real text field, so tapping opens the app's compose
 * box instead. It fires the same `quickcap/compose` deep link as the launcher
 * shortcut and the desktop hotkey, which is why there is no code here beyond
 * wiring the tap: everything after the tap already exists.
 */
class QuickCapWidgetProvider : AppWidgetProvider() {

  override fun onUpdate(
    context: Context,
    appWidgetManager: AppWidgetManager,
    appWidgetIds: IntArray
  ) {
    for (widgetId in appWidgetIds) {
      val views = RemoteViews(context.packageName, R.layout.quickcap_widget)
      views.setOnClickPendingIntent(R.id.widget_root, composeIntent(context))
      appWidgetManager.updateAppWidget(widgetId, views)
    }
  }

  private fun composeIntent(context: Context): PendingIntent {
    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(COMPOSE_URL)).apply {
      // Named explicitly so the tap cannot be picked up by another app that
      // has registered the same scheme.
      setPackage(context.packageName)
    }

    return PendingIntent.getActivity(
      context,
      0,
      intent,
      // Immutable is required from API 31, and correct anyway: nothing should
      // be able to rewrite where this tap goes.
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
    )
  }

  companion object {
    private const val COMPOSE_URL = "com.synabit.app://quickcap/compose"
  }
}
