# HIVE ATAK Plugin ProGuard Rules

# Keep HIVE FFI bindings
-keep class uniffi.hive_ffi.** { *; }

# Keep plugin entry points for ATAK
-keep class com.atakmap.android.hive.plugin.HivePluginLifecycle { *; }
-keep class com.atakmap.android.hive.plugin.HiveMapComponent { *; }
-keep class com.atakmap.android.hive.plugin.HiveDropDownReceiver { *; }
-keep class com.atakmap.android.hive.plugin.HiveNodeManager { *; }

# Keep data classes for serialization (kotlinx.serialization)
-keep class com.atakmap.android.hive.plugin.model.** { *; }
-keepclassmembers class com.atakmap.android.hive.plugin.model.** {
    public <init>(...);
}

# Keep Kotlin serialization
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.AnnotationsKt

-keepclassmembers class kotlinx.serialization.json.** {
    *** Companion;
}
-keepclasseswithmembers class kotlinx.serialization.json.** {
    kotlinx.serialization.KSerializer serializer(...);
}

-keep,includedescriptorclasses class com.atakmap.android.hive.plugin.**$$serializer { *; }
-keepclassmembers class com.atakmap.android.hive.plugin.** {
    *** Companion;
}
-keepclasseswithmembers class com.atakmap.android.hive.plugin.** {
    kotlinx.serialization.KSerializer serializer(...);
}

# Keep Compose classes
-keep class androidx.compose.** { *; }

# JNI methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# Keep enum values
-keepclassmembers enum * {
    public static **[] values();
    public static ** valueOf(java.lang.String);
}

# Keep JNA classes for UniFFI
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
