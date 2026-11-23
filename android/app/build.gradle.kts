import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
}

android {
    namespace = "app.trame.android"
    compileSdk = 37

    defaultConfig {
        applicationId = "app.trame.android"
        minSdk = 26
        targetSdk = 36
        versionCode = 2
        versionName = "0.1.1"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // En attendant une clé à nous : la clé de débogage, pour pouvoir installer l'APK.
            signingConfig = signingConfigs.getByName("debug")
            // Téléphones seulement (arm64) : l'APK passe d'environ 530 Mo (toutes architectures,
            // émulateur compris) à une taille raisonnable. L'émulateur x86_64 prend la version debug.
            ndk { abiFilters += "arm64-v8a" }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // Bibliothèques du moteur compressées dans l'APK (environ 200 Mo sinon) ; Android les
    // décompresse une fois à l'installation.
    packaging { jniLibs { useLegacyPackaging = true } }

    sourceSets["main"].assets.srcDir(layout.buildDirectory.dir("generated/extensions").get().asFile)
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

dependencies {
    // Moteur de Firefox, toutes architectures (téléphone arm64 et émulateur x86_64).
    implementation("org.mozilla.geckoview:geckoview:160.0.20261116130455")
}

// Le saute-pub reste écrit UNE fois, dans ../../extensions/saute-pub (celui du PC). On en copie
// les scripts dans l'APK, à côté du manifeste Firefox et des règles de blocage propres à Android
// (src/main/extensions/saute-pub). Seul ajustement : contenu.js lit l'état posé par debut.js dans
// la page ; sous Firefox, la page n'est visible d'un script de contenu que par wrappedJSObject.
val copierSautePub by tasks.registering(Copy::class) {
    val sortie = layout.buildDirectory.dir("generated/extensions/extensions/saute-pub")
    into(sortie)
    from(rootProject.file("../extensions/saute-pub")) {
        include("debut.js", "contenu.js", "style.css")
        filter { ligne -> ligne.replace("window.__trameSansPub", "(window.wrappedJSObject || window).__trameSansPub") }
    }
    from("src/main/extensions/saute-pub")
}
tasks.named("preBuild") { dependsOn(copierSautePub) }
