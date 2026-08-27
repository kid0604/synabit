package com.synabit.app

import android.app.PendingIntent
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

/**
 * A capture tile in the quick settings panel.
 *
 * Two swipes from anywhere — including over a full-screen app, where the
 * launcher shortcut and the home-screen widget are both out of reach. It is
 * the only surface that does not require getting back to a home screen first.
 *
 * A tile cannot take text, so this opens the compose box. Like every other
 * surface, it does that through the same `quickcap/compose` deep link, so
 * there is nothing here but the tap.
 */
class CaptureTileService : TileService() {

  override fun onStartListening() {
    super.onStartListening()
    // Stateless: this is a button, not a switch. Leaving it INACTIVE keeps it
    // from rendering as something that is currently turned on.
    qsTile?.apply {
      state = Tile.STATE_INACTIVE
      label = getString(R.string.tile_label)
      updateTile()
    }
  }

  override fun onClick() {
    super.onClick()

    val pending = PendingIntent.getActivity(
      this,
      0,
      Intent(Intent.ACTION_VIEW, Uri.parse(COMPOSE_URL)).setPackage(packageName),
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
    )

    // The panel has to be closed for the compose box to be visible, and how
    // that is asked for changed in Android 14: the Intent overload was
    // removed in favour of a PendingIntent. Building for API 36 while
    // supporting 24 means carrying both.
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
      startActivityAndCollapse(pending)
    } else {
      @Suppress("DEPRECATION")
      startActivityAndCollapse(
        Intent(Intent.ACTION_VIEW, Uri.parse(COMPOSE_URL)).setPackage(packageName)
      )
    }
  }

  companion object {
    private const val COMPOSE_URL = "com.synabit.app://quickcap/compose"
  }
}
