import java.io.FileInputStream
import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.serialization") version "1.9.20"
}

// Load local properties
val localProperties = Properties().apply {
    val localPropsFile = rootProject.file("local.properties")
    if (localPropsFile.exists()) {
        load(FileInputStream(localPropsFile))
    }
}

// Helper to get property value
fun getLocalProperty(key: String, defaultValue: String = ""): String {
    return localProperties.getProperty(key, defaultValue)
}

android {
    namespace = "com.atakmap.android.hive.plugin"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.atakmap.android.hive.plugin"
        minSdk = 26  // Android 8.0 (ATAK minimum)
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"

        // ATAK plugin configuration
        manifestPlaceholders["atakApiVersion"] = "5.5"

        ndk {
            // Build for all Android architectures supported by hive-ffi
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64", "x86")
        }
    }

    signingConfigs {
        create("debug") {
            val keyFile = getLocalProperty("takDebugKeyFile")
            if (keyFile.isNotEmpty() && file(keyFile).exists()) {
                storeFile = file(keyFile)
                storePassword = getLocalProperty("takDebugKeyFilePassword", "android")
                keyAlias = getLocalProperty("takDebugKeyAlias", "androiddebugkey")
                keyPassword = getLocalProperty("takDebugKeyPassword", "android")
            }
        }
        create("release") {
            val keyFile = getLocalProperty("takReleaseKeyFile")
            if (keyFile.isNotEmpty() && file(keyFile).exists()) {
                storeFile = file(keyFile)
                storePassword = getLocalProperty("takReleaseKeyFilePassword")
                keyAlias = getLocalProperty("takReleaseKeyAlias")
                keyPassword = getLocalProperty("takReleaseKeyPassword")
            }
        }
    }

    buildTypes {
        debug {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("debug")
        }
        release {
            isMinifyEnabled = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            signingConfig = signingConfigs.getByName("release")
        }
    }

    // ATAK flavor dimensions
    flavorDimensions += "tak"

    productFlavors {
        create("civ") {
            dimension = "tak"
            applicationIdSuffix = ".civ"
            manifestPlaceholders["atakVariant"] = "CIV"
        }
        create("mil") {
            dimension = "tak"
            applicationIdSuffix = ".mil"
            manifestPlaceholders["atakVariant"] = "MIL"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        viewBinding = true
        compose = true
    }

    composeOptions {
        kotlinCompilerExtensionVersion = "1.5.4"
    }

    // Location of pre-built native libraries from hive-ffi
    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("libs")
        }
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

dependencies {
    // ATAK SDK - provided at runtime by ATAK app
    // Note: These must be marked as 'compileOnly' since ATAK provides them
    // compileOnly(files("${getLocalProperty("atak.sdk.dir")}/main.jar"))

    // Kotlin standard library
    implementation("org.jetbrains.kotlin:kotlin-stdlib:1.9.20")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.0")

    // AndroidX (versions compatible with ATAK)
    compileOnly("androidx.core:core-ktx:1.12.0")
    compileOnly("androidx.appcompat:appcompat:1.6.1")
    compileOnly("androidx.fragment:fragment-ktx:1.6.2")
    compileOnly("androidx.lifecycle:lifecycle-runtime-ktx:2.6.2")
    compileOnly("androidx.lifecycle:lifecycle-viewmodel-ktx:2.6.2")

    // Jetpack Compose (for modern UI)
    implementation(platform("androidx.compose:compose-bom:2023.10.01"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.activity:activity-compose:1.8.1")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.6.2")

    // ATAK Compose helper library
    implementation("com.dittofederal:atak-compose:0.0.4")

    // HIVE FFI Kotlin bindings (copy from kotlin-test or generate)
    // The bindings and native .so files will be copied to this module
    // implementation(project(":hive-bindings"))

    // HIVE BLE mesh transport (for WearTAK sync)
    // 0.0.10 adds field-level delta sync for bandwidth efficiency
    implementation("com.revolveteam:hive:0.0.10")

    // HIVE-Lite for canned message encoding/decoding (Kotlin bindings copied directly)
    // Native libs in libs/arm64-v8a, libs/armeabi-v7a, libs/x86_64
    implementation("net.java.dev.jna:jna:5.14.0@aar")  // Required by UniFFI

    // Testing
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.5")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.1")
}

// Task to copy native libraries from hive-ffi build
tasks.register<Copy>("copyHiveNativeLibs") {
    description = "Copy hive-ffi native libraries to jniLibs"

    val hiveFfiDir = rootProject.file("../target")

    from("$hiveFfiDir/aarch64-linux-android/release/libhive_ffi.so") {
        into("arm64-v8a")
    }
    from("$hiveFfiDir/armv7-linux-androideabi/release/libhive_ffi.so") {
        into("armeabi-v7a")
    }
    from("$hiveFfiDir/x86_64-linux-android/release/libhive_ffi.so") {
        into("x86_64")
    }
    from("$hiveFfiDir/i686-linux-android/release/libhive_ffi.so") {
        into("x86")
    }

    into("$projectDir/libs")
}

// Task to copy Kotlin bindings from hive-ffi
tasks.register<Copy>("copyHiveBindings") {
    description = "Copy hive-ffi Kotlin bindings"

    from(rootProject.file("../bindings/kotlin/uniffi/hive_ffi"))
    into("$projectDir/src/main/java/uniffi/hive_ffi")
}
