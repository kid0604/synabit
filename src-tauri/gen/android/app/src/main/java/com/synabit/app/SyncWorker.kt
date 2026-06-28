package com.synabit.app

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import android.util.Log

class SyncWorker(
    private val context: Context,
    workerParams: WorkerParameters
) : CoroutineWorker(context, workerParams) {

    companion object {
        private const val TAG = "SynabitSyncWorker"

        init {
            // Load the Rust library compiled by Tauri
            try {
                System.loadLibrary("synabit")
                Log.i(TAG, "Successfully loaded libsynabit.so")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load libsynabit.so", e)
            }
        }
    }

    // Declare the JNI method
    private external fun runHeadlessSync(vaultPath: String, serverAddr: String, serverId: String)

    override suspend fun doWork(): Result {
        Log.i(TAG, "Starting background sync work...")

        val sharedPrefs = context.getSharedPreferences("SynabitPrefs", Context.MODE_PRIVATE)
        val vaultPath = sharedPrefs.getString("vaultPath", null)
        val serverAddr = sharedPrefs.getString("p2pServerAddr", null)
        val serverId = sharedPrefs.getString("p2pServerIdHex", null)

        if (vaultPath.isNullOrEmpty() || serverAddr.isNullOrEmpty() || serverId.isNullOrEmpty()) {
            Log.w(TAG, "Vault path or server config is missing. Skipping background sync.")
            return Result.success()
        }

        return try {
            // Invoke the Rust logic headless
            runHeadlessSync(vaultPath, serverAddr, serverId)
            Log.i(TAG, "Background sync completed successfully")
            Result.success()
        } catch (e: Exception) {
            Log.e(TAG, "Error executing background sync", e)
            Result.retry()
        }
    }
}
