package app.trame.android

import android.app.Application
import android.content.pm.ApplicationInfo
import org.mozilla.geckoview.GeckoRuntime
import org.mozilla.geckoview.GeckoRuntimeSettings

/**
 * Un seul moteur Gecko par appli. Créé à la première demande (par l'activité) et pas dans
 * onCreate : Gecko lance ses propres processus avec cette même classe Application, et le
 * moteur ne doit exister que dans le processus principal.
 */
class TrameApp : Application() {
    val runtime: GeckoRuntime by lazy {
        val debogable = (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
        val reglages = GeckoRuntimeSettings.Builder()
            // console.log des pages et des extensions -> logcat (préfixe « [saute-pub] »).
            .consoleOutput(true)
            .remoteDebuggingEnabled(debogable)
            .preferredColorScheme(GeckoRuntimeSettings.COLOR_SCHEME_DARK)
            .build()
        GeckoRuntime.create(this, reglages)
    }
}
