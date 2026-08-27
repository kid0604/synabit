# ---------------------------------------------------------------------------
# SecureStore is reached only from Rust, by name, over JNI.
#
# R8 has no way to see that. The generated proguard-wry.pro carries
# `-keep class com.synabit.app.* { native <methods>; }`, which keeps the class
# itself but only its *native* members — and saveSecret/getSecret are ordinary
# static Java methods with zero callers on the Java side. R8 therefore removes
# or renames them, JNI's method lookup fails, and the app cannot read its own
# encryption key.
#
# Keep every member. The class is two methods; there is nothing to gain by
# being narrower, and being narrower is how this broke in the first place.
# ---------------------------------------------------------------------------
-keep class com.synabit.app.SecureStore { *; }

# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# ---------------------------------------------------------------------------
# Google Tink references JSR-305 annotations that are not on the classpath.
#
# Tink arrives through androidx.security:security-crypto, which is what backs
# EncryptedSharedPreferences and therefore SecureStore above. It is compiled
# against javax.annotation.Nullable and javax.annotation.concurrent.GuardedBy,
# neither of which ships with Android or with Tink itself — they are
# compile-time only and have no runtime effect.
#
# R8 treats a missing referenced class as an error, not a warning, so the
# release build failed at :app:minifyUniversalReleaseWithR8 before it ever
# reached packaging or signing. Nothing was wrong with the code; the annotation
# classes are simply absent by design.
#
# These are the two rules R8 itself generated into
# app/build/outputs/mapping/universalRelease/missing_rules.txt.
# ---------------------------------------------------------------------------
-dontwarn javax.annotation.Nullable
-dontwarn javax.annotation.concurrent.GuardedBy
