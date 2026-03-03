# PEAT ATAK Plugin ProGuard Rules

# Keep PEAT FFI bindings
-keep class uniffi.peat_ffi.** { *; }

# Keep plugin entry points for ATAK
-keep class com.defenseunicorns.atak.peat.HivePluginLifecycle { *; }
-keep class com.defenseunicorns.atak.peat.HiveMapComponent { *; }
-keep class com.defenseunicorns.atak.peat.HiveDropDownReceiver { *; }
-keep class com.defenseunicorns.atak.peat.HiveNodeManager { *; }

# Keep data classes for serialization (kotlinx.serialization)
-keep class com.defenseunicorns.atak.peat.model.** { *; }
-keepclassmembers class com.defenseunicorns.atak.peat.model.** {
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

-keep,includedescriptorclasses class com.defenseunicorns.atak.peat.**$$serializer { *; }
-keepclassmembers class com.defenseunicorns.atak.peat.** {
    *** Companion;
}
-keepclasseswithmembers class com.defenseunicorns.atak.peat.** {
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
