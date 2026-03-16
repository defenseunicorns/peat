plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
    id("signing")
}

group = "com.defenseunicorns"
version = "0.1.0"

android {
    namespace = "com.defenseunicorns.peat.ffi"
    compileSdk = 34

    defaultConfig {
        minSdk = 26
        targetSdk = 34
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    publishing {
        singleVariant("release") {
            withSourcesJar()
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.annotation:annotation:1.7.1")
    implementation("net.java.dev.jna:jna:5.14.0@aar")
}

// Build native libraries using cargo from the workspace root
tasks.register<Exec>("buildNativeLibs") {
    description = "Build native Rust libraries for Android"
    group = "build"

    // peat workspace root is two levels up from peat-ffi/android/
    val workspaceRoot = rootProject.projectDir.parentFile.parentFile
    workingDir = workspaceRoot

    val ndkPath = System.getenv("ANDROID_NDK_HOME")
        ?: System.getenv("NDK_HOME")
        ?: "${System.getenv("ANDROID_HOME")}/ndk/27.0.12077973"

    val ndkToolchain = "$ndkPath/toolchains/llvm/prebuilt/linux-x86_64/bin"
    environment("ANDROID_NDK_HOME", ndkPath)
    environment("PATH", "$ndkToolchain:${System.getenv("PATH")}")
    environment("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER", "$ndkToolchain/aarch64-linux-android26-clang")
    environment("CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER", "$ndkToolchain/armv7a-linux-androideabi26-clang")
    environment("CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER", "$ndkToolchain/x86_64-linux-android26-clang")

    commandLine("bash", "-c", """
        set -e
        echo "Building peat-ffi native libraries from: ${'$'}(pwd)"

        # Build for arm64-v8a (modern Android devices)
        echo "Building for aarch64-linux-android (arm64-v8a)..."
        cargo build --release --lib -p peat-ffi --target aarch64-linux-android         mkdir -p peat-ffi/android/src/main/jniLibs/arm64-v8a
        cp target/aarch64-linux-android/release/libpeat_ffi.so peat-ffi/android/src/main/jniLibs/arm64-v8a/

        # Build for armeabi-v7a (older devices)
        echo "Building for armv7-linux-androideabi (armeabi-v7a)..."
        cargo build --release --lib -p peat-ffi --target armv7-linux-androideabi         mkdir -p peat-ffi/android/src/main/jniLibs/armeabi-v7a
        cp target/armv7-linux-androideabi/release/libpeat_ffi.so peat-ffi/android/src/main/jniLibs/armeabi-v7a/

        # Build for x86_64 (emulators)
        echo "Building for x86_64-linux-android (x86_64)..."
        cargo build --release --lib -p peat-ffi --target x86_64-linux-android         mkdir -p peat-ffi/android/src/main/jniLibs/x86_64
        cp target/x86_64-linux-android/release/libpeat_ffi.so peat-ffi/android/src/main/jniLibs/x86_64/

        echo "Native libraries built successfully!"
    """.trimIndent())
}

// Generate Kotlin bindings from UniFFI
tasks.register<Exec>("generateBindings") {
    description = "Generate Kotlin bindings from UniFFI"
    group = "build"
    dependsOn("buildNativeLibs")

    val workspaceRoot = rootProject.projectDir.parentFile.parentFile
    workingDir = workspaceRoot

    commandLine("bash", "-c", """
        set -e
        echo "Generating Kotlin bindings..."
        cargo run -p peat-ffi --bin uniffi-bindgen generate \
            --library target/aarch64-linux-android/release/libpeat_ffi.so \
            --language kotlin \
            --out-dir peat-ffi/android/src/main/java
        echo "Kotlin bindings generated."
    """.trimIndent())
}

// Combined: build native + assemble AAR
tasks.register("buildAar") {
    description = "Build native libraries and assemble AAR"
    group = "build"
    dependsOn("buildNativeLibs")
    finalizedBy("assembleRelease")
}

// Publishing
afterEvaluate {
    publishing {
        publications {
            register<MavenPublication>("release") {
                groupId = "com.defenseunicorns"
                artifactId = "peat-ffi"
                version = project.version.toString()
                from(components["release"])

                pom {
                    name.set("Peat FFI Android")
                    description.set("Android bindings for Peat mesh protocol via UniFFI")
                    url.set("https://github.com/defenseunicorns/peat")

                    licenses {
                        license {
                            name.set("Apache License 2.0")
                            url.set("https://www.apache.org/licenses/LICENSE-2.0")
                        }
                    }

                    developers {
                        developer {
                            id.set("defenseunicorns")
                            name.set("Defense Unicorns")
                            email.set("oss@defenseunicorns.com")
                        }
                    }

                    scm {
                        connection.set("scm:git:git://github.com/defenseunicorns/peat.git")
                        developerConnection.set("scm:git:ssh://github.com/defenseunicorns/peat.git")
                        url.set("https://github.com/defenseunicorns/peat")
                    }
                }
            }
        }

        repositories {
            maven {
                name = "local"
                url = uri(layout.buildDirectory.dir("repo"))
            }
        }
    }

    signing {
        val signingKey = findProperty("signingInMemoryKey") as String? ?: System.getenv("ORG_GRADLE_PROJECT_signingInMemoryKey")
        val signingPassword = findProperty("signingInMemoryKeyPassword") as String? ?: System.getenv("ORG_GRADLE_PROJECT_signingInMemoryKeyPassword")
        if (signingKey != null && signingPassword != null) {
            useInMemoryPgpKeys(signingKey, signingPassword)
        } else {
            useGpgCmd()
        }
        sign(publishing.publications["release"])
    }
}

// Bundle for Maven Central upload
tasks.register<Zip>("createMavenCentralBundle") {
    description = "Create Maven Central bundle ZIP"
    group = "publishing"
    dependsOn("publishReleasePublicationToLocalRepository")

    from(layout.buildDirectory.dir("repo"))
    archiveFileName.set("peat-ffi-${project.version}-bundle.zip")
    destinationDirectory.set(layout.buildDirectory.dir("bundle"))
}

// Upload to Maven Central
tasks.register<Exec>("publishToMavenCentral") {
    description = "Upload bundle to Maven Central via Sonatype Central Portal"
    group = "publishing"
    dependsOn("createMavenCentralBundle")

    val bundleFile = layout.buildDirectory.file("bundle/peat-ffi-${project.version}-bundle.zip")
    val username = project.findProperty("sonatypeUsername") as String? ?: System.getenv("SONATYPE_USERNAME") ?: ""
    val password = project.findProperty("sonatypePassword") as String? ?: System.getenv("SONATYPE_PASSWORD") ?: ""

    doFirst {
        if (username.isEmpty() || password.isEmpty()) {
            throw GradleException("Sonatype credentials not configured")
        }
    }

    commandLine("bash", "-c", """
        curl --fail-with-body \
            -u "$username:$password" \
            -F "bundle=@${bundleFile.get().asFile.absolutePath}" \
            "https://central.sonatype.com/api/v1/publisher/upload?publishingType=AUTOMATIC"
    """.trimIndent())
}
