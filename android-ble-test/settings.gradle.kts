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
        flatDir {
            dirs("${rootDir}/../../peat-btle/android/build/outputs/aar")
        }
    }
}

rootProject.name = "peat-ble-test"
include(":app")
