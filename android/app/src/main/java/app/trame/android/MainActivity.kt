package app.trame.android

import android.app.Activity
import android.app.AlertDialog
import android.app.Dialog
import android.content.Intent
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.util.TypedValue
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowManager
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.Switch
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject
import org.mozilla.geckoview.AllowOrDeny
import org.mozilla.geckoview.GeckoResult
import org.mozilla.geckoview.GeckoRuntime
import org.mozilla.geckoview.GeckoSession
import org.mozilla.geckoview.GeckoSession.PermissionDelegate
import org.mozilla.geckoview.GeckoView
import org.mozilla.geckoview.WebExtension
import org.mozilla.geckoview.WebExtensionController
import java.net.URLEncoder

/**
 * Écran 2 de la maquette (docs/maquettes/android.html) : une page, la barre d'adresse sous le
 * pouce, cinq boutons. Premier APK : il sert d'abord à vérifier que le saute-pub et Bitwarden
 * tournent dans GeckoView.
 */
class MainActivity : Activity() {

    companion object {
        const val JOURNAL = "Trame"
        const val SAUTE_PUB_ID = "saute-pub@trame.app"
        const val SAUTE_PUB_URI = "resource://android/assets/extensions/saute-pub/"
        // Identifiant Firefox de Bitwarden, et son adresse de téléchargement sur addons.mozilla.org.
        const val BITWARDEN_ID = "{446900e4-71c2-419f-a6a7-df9c091e268b}"
        const val BITWARDEN_XPI =
            "https://addons.mozilla.org/firefox/downloads/latest/bitwarden-password-manager/latest.xpi"
        const val ACCUEIL = "resource://android/assets/accueil.html"
        // Même moteur par défaut que Trame sur PC (src/urls.rs, MOTEUR_DEFAUT).
        const val RECHERCHE = "https://www.perplexity.ai/search?q="
    }

    private lateinit var runtime: GeckoRuntime
    private lateinit var vue: GeckoView
    private lateinit var barre: LinearLayout
    private lateinit var adresse: EditText
    private lateinit var cadenas: TextView
    private lateinit var bouclier: TextView
    private lateinit var progression: ProgressBar
    private lateinit var btRetour: ImageButton
    private lateinit var btAvance: ImageButton
    private lateinit var compteur: TextView

    private val onglets = mutableListOf<Onglet>()
    private var actif: Onglet? = null
    private val extensions = mutableMapOf<String, WebExtension>()
    private val actions = mutableMapOf<String, WebExtension.Action>()
    private var pleinEcran = false
    private var popupsOuverts = 0

    /** Page du popup des extensions connues (default_popup de leur manifeste). */
    private val POPUPS = mapOf(BITWARDEN_ID to "popup/index.html")

    private fun dp(v: Float) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, resources.displayMetrics)
    private fun dpi(v: Int) = dp(v.toFloat()).toInt()
    private fun couleur(id: Int) = getColor(id)

    // ------------------------------------------------------------------ interface

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        runtime = (application as TrameApp).runtime

        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(false)
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = (View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION)
        }
        setContentView(construireEcran())
        brancherExtensions()

        val lien = intent?.dataString
        val restaures = restaurer()
        if (lien != null) ouvrir(lien, nouvelOnglet = true)
        else if (!restaures) ouvrir(ACCUEIL, nouvelOnglet = true)
    }

    private fun construireEcran(): View {
        val racine = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(couleur(R.color.fond))
        }
        vue = GeckoView(this).apply { coverUntilFirstPaint(couleur(R.color.fond)) }
        racine.addView(vue, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))

        barre = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(couleur(R.color.fond2))
            setPadding(dpi(10), 0, dpi(10), dpi(4))
        }
        progression = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = 100
            progressTintList = android.content.res.ColorStateList.valueOf(couleur(R.color.accent))
            progressBackgroundTintList = android.content.res.ColorStateList.valueOf(Color.TRANSPARENT)
            visibility = View.INVISIBLE
        }
        barre.addView(progression, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dpi(3)).apply {
            bottomMargin = dpi(5)
        })

        // Ligne d'adresse : cadenas, champ, pastille du saute-pub.
        val ligne = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = GradientDrawable().apply {
                cornerRadius = dp(20f)
                setColor(couleur(R.color.surface2))
                setStroke(dpi(1), couleur(R.color.bord))
            }
            setPadding(dpi(12), 0, dpi(6), 0)
        }
        cadenas = TextView(this).apply {
            textSize = 12f
            setTextColor(couleur(R.color.vif))
            setPadding(0, 0, dpi(6), 0)
        }
        adresse = EditText(this).apply {
            background = null
            textSize = 14f
            setTextColor(couleur(R.color.texte))
            setHintTextColor(couleur(R.color.texte3))
            hint = "Rechercher ou saisir une adresse"
            isSingleLine = true
            imeOptions = EditorInfo.IME_ACTION_GO
            inputType = android.text.InputType.TYPE_TEXT_VARIATION_URI or android.text.InputType.TYPE_CLASS_TEXT
            setOnEditorActionListener { _, id, ev ->
                if (id == EditorInfo.IME_ACTION_GO || (ev?.keyCode == KeyEvent.KEYCODE_ENTER)) {
                    ouvrir(normaliser(text.toString()), nouvelOnglet = false)
                    cacherClavier()
                    true
                } else false
            }
            // Au toucher : l'adresse complète, entièrement sélectionnée (taper la remplace).
            // setText après coup annulait la sélection : on la refait ensuite.
            setOnFocusChangeListener { _, focus ->
                if (!focus) majBarre()
                else {
                    setText(actif?.url?.let { u -> if (u == ACCUEIL) "" else u } ?: "")
                    post { selectAll() }
                }
            }
        }
        bouclier = TextView(this).apply {
            textSize = 11f
            setTextColor(couleur(R.color.vif))
            background = GradientDrawable().apply {
                cornerRadius = dp(12f)
                setColor(Color.argb(26, 94, 234, 212))
            }
            setPadding(dpi(8), dpi(3), dpi(8), dpi(3))
            visibility = View.GONE
        }
        ligne.addView(cadenas)
        ligne.addView(adresse, LinearLayout.LayoutParams(0, dpi(40), 1f))
        ligne.addView(bouclier)
        barre.addView(ligne, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dpi(40)))

        // Cinq boutons.
        val boutons = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        fun bouton(icone: Int, desc: String, action: () -> Unit) = ImageButton(this).apply {
            setImageResource(icone)
            contentDescription = desc
            background = null
            setOnClickListener { action() }
        }
        btRetour = bouton(R.drawable.ic_retour, "Précédent") { actif?.session?.goBack() }
        btAvance = bouton(R.drawable.ic_avance, "Suivant") { actif?.session?.goForward() }
        val btNouvel = bouton(R.drawable.ic_plus, "Nouvel onglet") {
            ouvrir(ACCUEIL, nouvelOnglet = true)
            adresse.requestFocus()
        }
        compteur = TextView(this).apply {
            gravity = Gravity.CENTER
            textSize = 11f
            setTextColor(couleur(R.color.texte2))
            typeface = android.graphics.Typeface.DEFAULT_BOLD
            background = GradientDrawable().apply {
                cornerRadius = dp(6f)
                setStroke(dpi(2), couleur(R.color.texte2))
            }
            contentDescription = "Onglets"
        }
        val cadreCompteur = FrameLayout(this).apply {
            addView(compteur, FrameLayout.LayoutParams(dpi(22), dpi(22), Gravity.CENTER))
            setOnClickListener { montrerOnglets() }
        }
        val btMenu = bouton(R.drawable.ic_menu, "Menu") { montrerMenu() }
        for (v in listOf(btRetour, btAvance, btNouvel, cadreCompteur, btMenu)) {
            boutons.addView(v, LinearLayout.LayoutParams(0, dpi(42), 1f))
        }
        barre.addView(boutons, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dpi(42)).apply {
            topMargin = dpi(4)
        })
        racine.addView(barre, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))

        // Bords de l'écran : barre d'état en haut, barre de gestes (ou clavier) en bas.
        racine.setOnApplyWindowInsetsListener { v, ins ->
            if (pleinEcran) {
                v.setPadding(0, 0, 0, 0)
            } else if (Build.VERSION.SDK_INT >= 30) {
                val b = ins.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.ime() or WindowInsets.Type.displayCutout())
                v.setPadding(b.left, b.top, b.right, b.bottom)
            } else {
                @Suppress("DEPRECATION")
                v.setPadding(ins.systemWindowInsetLeft, ins.systemWindowInsetTop, ins.systemWindowInsetRight, ins.systemWindowInsetBottom)
            }
            ins
        }
        return racine
    }

    private fun cacherClavier() {
        adresse.clearFocus()
        (getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager).hideSoftInputFromWindow(adresse.windowToken, 0)
    }

    private fun majBarre() {
        val o = actif ?: return
        btRetour.isEnabled = o.peutReculer
        btRetour.alpha = if (o.peutReculer) 1f else 0.35f
        btAvance.isEnabled = o.peutAvancer
        btAvance.alpha = if (o.peutAvancer) 1f else 0.35f
        compteur.text = onglets.size.toString()
        if (!adresse.hasFocus()) adresse.setText(affichage(o.url))
        cadenas.text = if (o.url == ACCUEIL || o.url.isEmpty()) "⌕" else if (o.securise) "🔒" else "⚠"
        progression.progress = o.progression
        progression.visibility = if (o.progression in 1..99) View.VISIBLE else View.INVISIBLE
        if (o.bouclierActif && o.bouclier > 0) {
            bouclier.text = "🛡 ${o.bouclier}"
            bouclier.visibility = View.VISIBLE
        } else if (o.bouclierActif) {
            bouclier.text = "🛡"
            bouclier.visibility = View.VISIBLE
        } else {
            bouclier.visibility = View.GONE
        }
    }

    /** L'adresse telle qu'on la lit : sans « https:// » ni « www. ». */
    private fun affichage(url: String): String {
        if (url == ACCUEIL || url.isEmpty() || url == "about:blank") return ""
        return url.removePrefix("https://").removePrefix("http://").removePrefix("www.")
    }

    private fun normaliser(saisie: String): String {
        val t = saisie.trim()
        if (t.isEmpty()) return ACCUEIL
        if (Regex("^[a-zA-Z][a-zA-Z0-9+.-]*://").containsMatchIn(t) || t.startsWith("about:")) return t
        if (!t.contains(' ') && Regex("^[^\\s/]+\\.[a-zA-Z]{2,}(:\\d+)?(/.*)?$").matches(t)) return "https://$t"
        if (Regex("^(localhost|\\d{1,3}(\\.\\d{1,3}){3})(:\\d+)?(/.*)?$").matches(t)) return "http://$t"
        return RECHERCHE + URLEncoder.encode(t, "UTF-8")
    }

    // ------------------------------------------------------------------ onglets

    private fun creerOnglet(): Onglet {
        val session = GeckoSession()
        val o = Onglet(session)
        session.navigationDelegate = object : GeckoSession.NavigationDelegate {
            override fun onLocationChange(
                session: GeckoSession, url: String?,
                perms: MutableList<PermissionDelegate.ContentPermission>, hasUserGesture: Boolean,
            ) {
                val nouvelle = url ?: ""
                if (hote(nouvelle) != hote(o.url)) { o.bouclier = 0; o.bouclierActif = false }
                o.url = nouvelle
                if (o === actif) majBarre()
            }
            override fun onCanGoBack(session: GeckoSession, canGoBack: Boolean) { o.peutReculer = canGoBack; if (o === actif) majBarre() }
            override fun onCanGoForward(session: GeckoSession, canGoForward: Boolean) { o.peutAvancer = canGoForward; if (o === actif) majBarre() }
            override fun onNewSession(session: GeckoSession, uri: String): GeckoResult<GeckoSession> {
                // target=_blank, window.open : un nouvel onglet, montré tout de suite.
                val n = creerOnglet()
                onglets.add(onglets.indexOf(o) + 1, n)
                montrer(n)
                return GeckoResult.fromValue(n.session)
            }
        }
        session.progressDelegate = object : GeckoSession.ProgressDelegate {
            override fun onProgressChange(session: GeckoSession, progress: Int) { o.progression = progress; if (o === actif) majBarre() }
            override fun onSecurityChange(session: GeckoSession, securityInfo: GeckoSession.ProgressDelegate.SecurityInformation) {
                o.securise = securityInfo.isSecure; if (o === actif) majBarre()
            }
        }
        session.contentDelegate = object : GeckoSession.ContentDelegate {
            override fun onTitleChange(session: GeckoSession, title: String?) { o.titre = title ?: "" }
            override fun onFullScreen(session: GeckoSession, fullScreen: Boolean) { basculerPleinEcran(fullScreen) }
            override fun onCloseRequest(session: GeckoSession) { fermer(o) }
        }
        session.permissionDelegate = object : PermissionDelegate {
            override fun onContentPermissionRequest(session: GeckoSession, perm: PermissionDelegate.ContentPermission): GeckoResult<Int> {
                // Lecture automatique des vidéos (YouTube) : permise. Le reste : refusé pour l'instant.
                val ok = perm.permission == PermissionDelegate.PERMISSION_AUTOPLAY_AUDIBLE ||
                    perm.permission == PermissionDelegate.PERMISSION_AUTOPLAY_INAUDIBLE
                return GeckoResult.fromValue(
                    if (ok) PermissionDelegate.ContentPermission.VALUE_ALLOW else PermissionDelegate.ContentPermission.VALUE_DENY
                )
            }
        }
        session.promptDelegate = Invites(this)
        for (ext in extensions.values) brancher(o, ext)
        return o
    }

    private fun hote(url: String) = try { Uri.parse(url).host ?: "" } catch (e: Exception) { "" }

    private fun ouvrir(url: String, nouvelOnglet: Boolean) {
        val o = if (nouvelOnglet || actif == null) {
            creerOnglet().also {
                it.session.open(runtime)
                onglets.add(it)
            }
        } else actif!!
        o.url = url
        o.session.loadUri(url)
        montrer(o)
    }

    private fun montrer(o: Onglet) {
        if (actif === o && vue.session === o.session) { majBarre(); return }
        actif?.session?.setActive(false)
        vue.releaseSession()
        actif = o
        // Onglet restauré : sa session n'a jamais été ouverte. GeckoView.setSession ne l'ouvre PAS
        // (constaté le 23/11 sur le téléphone : page noire, rien ne répondait). Les sessions
        // rendues à Gecko (onNewSession, onNewTab) sont ouvertes par Gecko lui-même : pas ici.
        if (o.aCharger && !o.session.isOpen) o.session.open(runtime)
        vue.setSession(o.session)
        o.session.setActive(true)
        if (o.aCharger) {
            o.aCharger = false
            o.session.loadUri(o.url)
        }
        majBarre()
    }

    private fun fermer(o: Onglet) {
        val i = onglets.indexOf(o)
        if (i < 0) return
        onglets.removeAt(i)
        if (actif === o) {
            vue.releaseSession()
            actif = null
        }
        o.session.close()
        if (onglets.isEmpty()) ouvrir(ACCUEIL, nouvelOnglet = true)
        else if (actif == null) montrer(onglets[minOf(i, onglets.size - 1)])
        majBarre()
    }

    private fun montrerOnglets() {
        val titres = onglets.map { o ->
            val t = o.titre.ifBlank { affichage(o.url).ifBlank { "Nouvel onglet" } }
            if (o === actif) "● $t" else t
        }.toTypedArray()
        AlertDialog.Builder(this, R.style.Theme_Trame_Dialogue)
            .setTitle("${onglets.size} onglet" + if (onglets.size > 1) "s" else "")
            .setItems(titres) { _, i -> montrer(onglets[i]) }
            .setNeutralButton("Fermer celui-ci") { _, _ -> actif?.let { fermer(it) } }
            .setPositiveButton("Nouvel onglet") { _, _ -> ouvrir(ACCUEIL, nouvelOnglet = true) }
            .show()
    }

    private fun basculerPleinEcran(oui: Boolean) {
        pleinEcran = oui
        barre.visibility = if (oui) View.GONE else View.VISIBLE
        if (Build.VERSION.SDK_INT >= 30) {
            val c = window.insetsController
            if (oui) c?.hide(WindowInsets.Type.systemBars()) else c?.show(WindowInsets.Type.systemBars())
        } else {
            @Suppress("DEPRECATION")
            if (oui) window.addFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN)
            else window.clearFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN)
        }
        window.decorView.requestApplyInsets()
    }

    // ------------------------------------------------------------------ extensions

    private fun brancherExtensions() {
        val controleur = runtime.webExtensionController
        controleur.promptDelegate = object : WebExtensionController.PromptDelegate {
            override fun onInstallPromptRequest(
                extension: WebExtension, permissions: Array<String>, origins: Array<String>,
                dataCollectionPermissions: Array<String>,
            ): GeckoResult<WebExtension.PermissionPromptResponse> {
                // Bitwarden : c'est l'appli qui l'installe, on accepte. Toute autre : on demande.
                if (extension.id == BITWARDEN_ID) {
                    return GeckoResult.fromValue(WebExtension.PermissionPromptResponse(true, false, false))
                }
                val res = GeckoResult<WebExtension.PermissionPromptResponse>()
                val nom = extension.metaData.name ?: extension.id
                AlertDialog.Builder(this@MainActivity, R.style.Theme_Trame_Dialogue)
                    .setTitle("Installer « $nom » ?")
                    .setMessage("Elle demande : " + (permissions + origins).joinToString(", "))
                    .setPositiveButton("Installer") { _, _ -> res.complete(WebExtension.PermissionPromptResponse(true, false, false)) }
                    .setNegativeButton("Non") { _, _ -> res.complete(WebExtension.PermissionPromptResponse(false, false, false)) }
                    .setOnCancelListener { res.complete(WebExtension.PermissionPromptResponse(false, false, false)) }
                    .show()
                return res
            }
        }

        controleur.ensureBuiltIn(SAUTE_PUB_URI, SAUTE_PUB_ID).accept(
            { ext -> if (ext != null) adopter(ext) },
            { err -> Log.e(JOURNAL, "saute-pub non chargé", err) },
        )
        controleur.list().accept({ liste ->
            val installees = liste ?: emptyList()
            for (ext in installees) if (ext.id != SAUTE_PUB_ID) adopter(ext)
            if (installees.none { it.id == BITWARDEN_ID }) {
                Log.i(JOURNAL, "Bitwarden absent : installation depuis addons.mozilla.org")
                controleur.install(BITWARDEN_XPI).accept(
                    { ext -> if (ext != null) { Log.i(JOURNAL, "Bitwarden installé ${ext.metaData.version}"); adopter(ext) } },
                    { err -> Log.e(JOURNAL, "Bitwarden non installé", err) },
                )
            }
        }, { err -> Log.e(JOURNAL, "liste des extensions illisible", err) })
    }

    /** Une extension connue de l'appli : ses onglets, son bouton, et ses messages par onglet. */
    private fun adopter(ext: WebExtension) {
        extensions[ext.id] = ext
        Log.i(JOURNAL, "extension prête : ${ext.id} ${ext.metaData.version} active=${ext.metaData.enabled}")
        ext.tabDelegate = object : WebExtension.TabDelegate {
            override fun onNewTab(source: WebExtension, createDetails: WebExtension.CreateTabDetails): GeckoResult<GeckoSession> {
                val n = creerOnglet()
                onglets.add(n)
                if (createDetails.active != false) runOnUiThread { montrer(n) }
                return GeckoResult.fromValue(n.session)
            }
        }
        ext.setActionDelegate(object : WebExtension.ActionDelegate {
            override fun onBrowserAction(extension: WebExtension, session: GeckoSession?, action: WebExtension.Action) {
                Log.i(JOURNAL, "action reçue de ${extension.id} (onglet=${session != null}, même exemplaire=${extension === ext})")
                if (session == null) actions[extension.id] = action
                // Action.click() cherche le délégué sur l'exemplaire de l'extension que GeckoView
                // garde, pas forcément celui que list() nous a donné : on s'y accroche aussi.
                if (extension !== ext) extension.setActionDelegate(this)
            }
            override fun onOpenPopup(extension: WebExtension, action: WebExtension.Action): GeckoResult<GeckoSession> {
                Log.i(JOURNAL, "popup demandé par ${extension.id}")
                return GeckoResult.fromValue(montrerPopup())
            }
            // Action.click() (notre menu) passe encore par celle-ci, dépréciée mais seule appelée.
            @Deprecated("Appelée par Action.click() dans GeckoView 160")
            override fun onTogglePopup(extension: WebExtension, action: WebExtension.Action): GeckoResult<GeckoSession> {
                Log.i(JOURNAL, "popup demandé (bouton) par ${extension.id}")
                return GeckoResult.fromValue(montrerPopup())
            }
        })
        for (o in onglets) brancher(o, ext)
    }

    private fun brancher(o: Onglet, ext: WebExtension) {
        val ctl = o.session.webExtensionController
        if (ext.id == SAUTE_PUB_ID) {
            ctl.setMessageDelegate(ext, object : WebExtension.MessageDelegate {
                override fun onMessage(nativeApp: String, message: Any, sender: WebExtension.MessageSender): GeckoResult<Any>? {
                    val m = message as? JSONObject ?: return null
                    o.bouclierActif = m.optBoolean("debutActif") || m.optBoolean("lecteur")
                    o.bouclier = m.optInt("retirees") + m.optInt("pubs") + m.optInt("sponsors")
                    Log.i(JOURNAL, "saute-pub ${hote(o.url)} : $m")
                    if (o === actif) majBarre()
                    return null
                }
            }, "trame")
        }
        ctl.setTabDelegate(ext, object : WebExtension.SessionTabDelegate {
            override fun onCloseTab(source: WebExtension?, session: GeckoSession): GeckoResult<AllowOrDeny> {
                fermer(o); return GeckoResult.allow()
            }
            override fun onUpdateTab(extension: WebExtension, session: GeckoSession, details: WebExtension.UpdateTabDetails): GeckoResult<AllowOrDeny> {
                if (details.active == true) montrer(o)
                return GeckoResult.allow()
            }
        })
    }

    /** La fenêtre d'une extension (Bitwarden) : une feuille en bas de l'écran, 80 % de haut. */
    private fun montrerPopup(): GeckoSession {
        popupsOuverts++
        val session = GeckoSession()
        session.promptDelegate = Invites(this)
        val cadre = GeckoView(this)
        val d = Dialog(this, android.R.style.Theme_Material_NoActionBar)
        d.setContentView(cadre)
        d.window?.apply {
            setLayout(ViewGroup.LayoutParams.MATCH_PARENT, (resources.displayMetrics.heightPixels * 0.8).toInt())
            setGravity(Gravity.BOTTOM)
            setBackgroundDrawable(GradientDrawable().apply {
                cornerRadii = floatArrayOf(dp(20f), dp(20f), dp(20f), dp(20f), 0f, 0f, 0f, 0f)
                setColor(couleur(R.color.surface))
            })
            setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
        }
        session.contentDelegate = object : GeckoSession.ContentDelegate {
            override fun onCloseRequest(session: GeckoSession) { Log.i(JOURNAL, "popup : fermeture demandée"); d.dismiss() }
        }
        session.navigationDelegate = object : GeckoSession.NavigationDelegate {
            override fun onLocationChange(
                session: GeckoSession, url: String?,
                perms: MutableList<PermissionDelegate.ContentPermission>, hasUserGesture: Boolean,
            ) { Log.i(JOURNAL, "popup : $url") }
        }
        d.setOnDismissListener { Log.i(JOURNAL, "popup : fermé"); cadre.releaseSession(); session.close() }
        // GeckoView charge l'adresse du popup dans la session renvoyée dès qu'il la reçoit
        // (Action.openPopup) : elle doit être déjà ouverte, sinon la page reste blanche.
        session.open(runtime)
        cadre.setSession(session)
        d.show()
        return session
    }

    private fun montrerMenu() {
        val d = Dialog(this, android.R.style.Theme_Material_NoActionBar)
        val feuille = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dpi(16), dpi(10), dpi(16), dpi(16))
        }
        fun titre(t: String) = TextView(this).apply {
            text = t.uppercase()
            textSize = 11.5f
            letterSpacing = 0.06f
            setTextColor(couleur(R.color.texte3))
            setPadding(0, dpi(14), 0, dpi(6))
        }
        // Actions rapides.
        val rangee = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        fun action(t: String, f: () -> Unit) = TextView(this).apply {
            text = t
            gravity = Gravity.CENTER
            textSize = 12f
            setTextColor(couleur(R.color.texte))
            background = GradientDrawable().apply { cornerRadius = dp(12f); setColor(couleur(R.color.surface2)) }
            setPadding(0, dpi(12), 0, dpi(12))
            setOnClickListener { d.dismiss(); f() }
        }
        val actionsMenu = listOf(
            action("⟳  Recharger") { actif?.session?.reload() },
            action("↗  Partager") {
                actif?.let { o ->
                    startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply {
                        type = "text/plain"; putExtra(Intent.EXTRA_TEXT, o.url)
                    }, null))
                }
            },
        )
        actionsMenu.forEachIndexed { i, v ->
            rangee.addView(v, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply {
                if (i > 0) leftMargin = dpi(8)
            })
        }
        feuille.addView(rangee)

        feuille.addView(titre("Extensions"))
        for (ext in extensions.values.sortedBy { it.id != SAUTE_PUB_ID }) {
            val nom = ext.metaData.name ?: ext.id
            val detail = when {
                ext.id == SAUTE_PUB_ID -> actif?.let { if (it.bouclier > 0) "${it.bouclier} pubs retirées ou sautées ici" else "Actif sur YouTube" } ?: ""
                actions[ext.id] != null -> "Toucher pour ouvrir"
                else -> ext.metaData.version
            }
            val ligne = LinearLayout(this).apply {
                orientation = LinearLayout.HORIZONTAL
                gravity = Gravity.CENTER_VERTICAL
                setPadding(0, dpi(8), 0, dpi(8))
            }
            val textes = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
            textes.addView(TextView(this).apply { text = nom; textSize = 14f; setTextColor(couleur(R.color.texte)) })
            textes.addView(TextView(this).apply { text = detail; textSize = 11.5f; setTextColor(couleur(R.color.texte3)) })
            ligne.addView(textes, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
            val inter = Switch(this).apply {
                isChecked = ext.metaData.enabled
                setOnCheckedChangeListener { _, oui ->
                    val ctl = runtime.webExtensionController
                    val r = if (oui) ctl.enable(ext, WebExtensionController.EnableSource.USER)
                    else ctl.disable(ext, WebExtensionController.EnableSource.USER)
                    r.accept({ e -> if (e != null) extensions[e.id] = e }, { err -> Log.e(JOURNAL, "bascule ${ext.id}", err) })
                }
            }
            ligne.addView(inter)
            actions[ext.id]?.let { a ->
                // Gecko ne rend le popup que pour l'onglet actif ET focalisé : le menu ouvert lui
                // prend le focus. On clique donc une fois le menu refermé et le focus revenu.
                ligne.setOnClickListener {
                    d.dismiss()
                    vue.postDelayed({
                        Log.i(JOURNAL, "clic sur l'action de ${ext.id}")
                        val avant = popupsOuverts
                        actif?.session?.setFocused(true)
                        a.click()
                        // Constaté le 23/11 (GeckoView 160) : le clic ne rend souvent aucun popup
                        // (Gecko ne trouve pas d'onglet actif). Repli : ouvrir la page du popup
                        // nous-mêmes, à l'adresse de l'extension.
                        vue.postDelayed({
                            val chemin = POPUPS[ext.id]
                            if (popupsOuverts == avant && chemin != null) {
                                Log.i(JOURNAL, "popup ouvert directement : ${ext.id}")
                                montrerPopup().loadUri(ext.metaData.baseUrl + chemin)
                            }
                        }, 1200)
                    }, 350)
                }
            }
            feuille.addView(ligne)
        }
        if (extensions.isEmpty()) {
            feuille.addView(TextView(this).apply { text = "Chargement…"; setTextColor(couleur(R.color.texte3)) })
        }

        d.setContentView(feuille)
        d.window?.apply {
            setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
            setGravity(Gravity.BOTTOM)
            setBackgroundDrawable(GradientDrawable().apply {
                cornerRadii = floatArrayOf(dp(20f), dp(20f), dp(20f), dp(20f), 0f, 0f, 0f, 0f)
                setColor(couleur(R.color.surface))
            })
        }
        d.show()
    }

    // ------------------------------------------------------------------ cycle de vie

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        intent.dataString?.let { ouvrir(it, nouvelOnglet = true) }
    }

    @Deprecated("Retour classique : suffisant tant qu'on vise aussi Android 8")
    override fun onBackPressed() {
        val o = actif
        when {
            pleinEcran && o != null -> o.session.exitFullScreen()
            adresse.hasFocus() -> cacherClavier()
            o != null && o.peutReculer -> o.session.goBack()
            else -> @Suppress("DEPRECATION") super.onBackPressed()
        }
    }

    override fun onPause() {
        super.onPause()
        sauvegarder()
    }

    /** Les onglets ouverts (adresses seulement) survivent à la fermeture de l'appli. */
    private fun sauvegarder() {
        val liste = JSONArray()
        for (o in onglets) if (o.url.isNotEmpty() && o.url != ACCUEIL) liste.put(o.url)
        getSharedPreferences("onglets", MODE_PRIVATE).edit()
            .putString("liste", liste.toString())
            .putInt("actif", maxOf(0, onglets.filter { it.url.isNotEmpty() && it.url != ACCUEIL }.indexOf(actif)))
            .apply()
    }

    private fun restaurer(): Boolean {
        val p = getSharedPreferences("onglets", MODE_PRIVATE)
        val liste = try { JSONArray(p.getString("liste", "[]")) } catch (e: Exception) { JSONArray() }
        if (liste.length() == 0) return false
        // Seul l'onglet montré se charge ; les autres attendent qu'on les ouvre (mémoire).
        for (i in 0 until liste.length()) {
            val o = creerOnglet()
            o.url = liste.getString(i)
            o.aCharger = true
            onglets.add(o)
        }
        montrer(onglets[p.getInt("actif", 0).coerceIn(0, onglets.size - 1)])
        return true
    }
}
