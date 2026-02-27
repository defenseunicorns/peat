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

rootProject.name = "PeatBtleDemo"
include(":app")

// Include peat-btle Android library from external repo
include(":peat-btle")
project(":peat-btle").projectDir = file("../../../peat-btle/android")
