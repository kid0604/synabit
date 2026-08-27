package com.synabit.app

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // Must run before super.onCreate. This swaps the launcher's splash theme
    // for the app's real one; without it the activity keeps the splash theme
    // and the WebView renders against the wrong background.
    installSplashScreen()

    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    // Put the capture box back in the shade. Posted on every launch rather
    // than once: a notification does not survive a reboot, and this is the
    // only moment the app reliably runs.
    CaptureNotification.post(this)
  }
}
