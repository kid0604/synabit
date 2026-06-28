package com.synabit.app

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.NetworkType
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import java.util.concurrent.TimeUnit

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    val constraints = Constraints.Builder()
        .setRequiredNetworkType(NetworkType.CONNECTED)
        .build()

    val syncWorkRequest = PeriodicWorkRequestBuilder<SyncWorker>(15, TimeUnit.MINUTES)
        .setConstraints(constraints)
        .build()

    WorkManager.getInstance(this).enqueueUniquePeriodicWork(
        "SynabitBackgroundSync",
        ExistingPeriodicWorkPolicy.KEEP,
        syncWorkRequest
    )
  }
}
