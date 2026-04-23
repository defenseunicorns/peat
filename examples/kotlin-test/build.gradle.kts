plugins {
    kotlin("jvm") version "2.0.21"
    application
}

group = "live.ditto.peat"
version = "0.1.0"

repositories {
    mavenCentral()
}

dependencies {
    // JNA for UniFFI native library loading
    implementation("net.java.dev.jna:jna:5.14.0")

    // Coroutines for async FFI support
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0")

    // Testing
    testImplementation(kotlin("test"))
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.0")
}

application {
    mainClass.set("MainKt")
}

tasks.test {
    useJUnitPlatform()
}

kotlin {
    jvmToolchain(21)
}

val nativeLibDir = layout.buildDirectory.dir("native")

// Copy native library to a location JNA can find
tasks.register<Copy>("copyNativeLib") {
    from("${rootProject.projectDir}/../target/debug")
    include("libpeat_ffi.dylib")  // macOS
    include("libpeat_ffi.so")     // Linux
    include("peat_ffi.dll")       // Windows
    into(nativeLibDir)
}

tasks.named<JavaExec>("run") {
    dependsOn("copyNativeLib")
    // Tell JNA where to find the native library
    jvmArgs("-Djna.library.path=${nativeLibDir.get().asFile.absolutePath}")
}

tasks.named<Test>("test") {
    dependsOn("copyNativeLib")
    jvmArgs("-Djna.library.path=${nativeLibDir.get().asFile.absolutePath}")
}
