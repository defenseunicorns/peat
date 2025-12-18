# ProGuard rules for hive-btle Android library

# Keep JNI native methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# Keep all classes in the hive.btle package
-keep class com.hive.btle.** { *; }

# Keep callback proxies (called from native code)
-keep class com.hive.btle.ScanCallbackProxy { *; }
-keep class com.hive.btle.GattCallbackProxy { *; }
-keep class com.hive.btle.AdvertiseCallbackProxy { *; }

# Keep HiveBtle main class
-keep class com.hive.btle.HiveBtle { *; }
-keep class com.hive.btle.HiveConnection { *; }
-keep class com.hive.btle.DiscoveredDevice { *; }
