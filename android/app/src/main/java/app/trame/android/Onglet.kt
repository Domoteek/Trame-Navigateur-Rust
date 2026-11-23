package app.trame.android

import org.mozilla.geckoview.GeckoSession

/** Un onglet : sa session Gecko et ce que la barre du bas affiche pour lui. */
class Onglet(val session: GeckoSession) {
    var titre: String = ""
    var url: String = ""
    var peutReculer = false
    var peutAvancer = false
    var progression = 100
    var securise = false

    /** Onglet restauré au démarrage : sa page ne se charge qu'à sa première ouverture. */
    var aCharger = false

    /** Pubs retirées ou sautées sur la page en cours (relayé par journal.js du saute-pub). */
    var bouclier = 0
    var bouclierActif = false
}
