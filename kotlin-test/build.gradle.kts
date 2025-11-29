plugins {
    kotlin("jvm") version "2.0.21"
    application
}

repositories {
    mavenCentral()
}


dependencies {
    implementation("net.java.dev.jna:jna:5.14.0")
}

application {
    mainClass.set("MainKt")
}

// Tell JVM where to find native library
tasks.named<JavaExec>("run") {
    systemProperty("jna.library.path", "${rootProject.projectDir}/../target/release")
}
