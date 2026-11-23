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
        // GeckoView (moteur de Firefox) n'est publié que sur le dépôt de Mozilla.
        maven("https://maven.mozilla.org/maven2/")
    }
}

rootProject.name = "TrameAndroid"
include(":app")
