# Consumer ProGuard rules for hive-btle
# These rules are applied to apps that use this library

# Keep JNI native methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# Keep callback proxies
-keep class com.hive.btle.ScanCallbackProxy { *; }
-keep class com.hive.btle.GattCallbackProxy { *; }
-keep class com.hive.btle.AdvertiseCallbackProxy { *; }
