// AGP 9.1 (Kotlin intégré) : exigé par GeckoView 160, qui tire androidx.core 1.19 (compileSdk 37).
// GeckoView tire aussi kotlin-stdlib 2.4 : le compilateur Kotlin fourni par AGP (2.2) ne sait pas
// le lire, d'où le plugin Kotlin à la même version, posé sur le classpath.
buildscript {
    dependencies {
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.4.21")
    }
}

plugins {
    id("com.android.application") version "9.1.0" apply false
}
