pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "HiveBtleDemo"
include(":app")

// Include hive-btle Android library from external repo
include(":hive-btle")
project(":hive-btle").projectDir = file("../../../hive-btle/android")
