package com.synabit.app;

import android.content.Context;
import android.content.SharedPreferences;
import android.util.Log;
import androidx.security.crypto.EncryptedSharedPreferences;
import androidx.security.crypto.MasterKey;

public class SecureStore {
    private static final String TAG = "SynabitSecureStore";
    private static final String PREFS_FILENAME = "synabit_secure_secrets";

    private static SharedPreferences encryptedPrefsInstance = null;

    private static synchronized SharedPreferences getEncryptedPrefs(Context context) throws Exception {
        if (encryptedPrefsInstance == null) {
            try {
                MasterKey masterKey = new MasterKey.Builder(context)
                        .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
                        .build();

                encryptedPrefsInstance = EncryptedSharedPreferences.create(
                        context,
                        PREFS_FILENAME,
                        masterKey,
                        EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                        EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
                );
            } catch (Exception e) {
                Log.e(TAG, "EncryptedSharedPreferences corrupted, recreating...", e);
                // Xóa file bị lỗi
                context.deleteSharedPreferences(PREFS_FILENAME);
                
                // Thử tạo lại
                MasterKey masterKey = new MasterKey.Builder(context)
                        .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
                        .build();

                encryptedPrefsInstance = EncryptedSharedPreferences.create(
                        context,
                        PREFS_FILENAME,
                        masterKey,
                        EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                        EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
                );
            }
        }
        return encryptedPrefsInstance;
    }

    public static boolean saveSecret(Context context, String key, String value) {
        try {
            SharedPreferences prefs = getEncryptedPrefs(context);
            // Dùng commit() thay vì apply() để đảm bảo file được ghi xuống disk ngay lập tức
            return prefs.edit().putString(key, value).commit();
        } catch (Exception e) {
            Log.e(TAG, "Failed to save secure secret", e);
            return false;
        }
    }

    public static String getSecret(Context context, String key) {
        try {
            SharedPreferences prefs = getEncryptedPrefs(context);
            String value = prefs.getString(key, "");
            return value != null ? value : "";
        } catch (Exception e) {
            Log.e(TAG, "Failed to get secure secret", e);
            return "";
        }
    }
}
