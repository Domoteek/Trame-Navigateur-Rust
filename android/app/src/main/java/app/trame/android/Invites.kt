package app.trame.android

import android.app.Activity
import android.app.AlertDialog
import android.widget.EditText
import org.mozilla.geckoview.GeckoResult
import org.mozilla.geckoview.GeckoSession
import org.mozilla.geckoview.GeckoSession.PromptDelegate
import org.mozilla.geckoview.GeckoSession.PromptDelegate.AlertPrompt
import org.mozilla.geckoview.GeckoSession.PromptDelegate.ButtonPrompt
import org.mozilla.geckoview.GeckoSession.PromptDelegate.ChoicePrompt
import org.mozilla.geckoview.GeckoSession.PromptDelegate.PromptResponse
import org.mozilla.geckoview.GeckoSession.PromptDelegate.TextPrompt

/**
 * Les invites de base des pages : alert, confirm, prompt et listes déroulantes (<select>).
 * Sans elles, GeckoView n'affiche rien : un <select> ne s'ouvrirait même pas.
 */
class Invites(private val activite: Activity) : PromptDelegate {

    private fun dialogue() = AlertDialog.Builder(activite, R.style.Theme_Trame_Dialogue)

    override fun onAlertPrompt(session: GeckoSession, prompt: AlertPrompt): GeckoResult<PromptResponse> {
        val res = GeckoResult<PromptResponse>()
        dialogue().setTitle(prompt.title).setMessage(prompt.message)
            .setPositiveButton("OK") { _, _ -> res.complete(prompt.dismiss()) }
            .setOnCancelListener { res.complete(prompt.dismiss()) }
            .show()
        return res
    }

    override fun onButtonPrompt(session: GeckoSession, prompt: ButtonPrompt): GeckoResult<PromptResponse> {
        val res = GeckoResult<PromptResponse>()
        dialogue().setTitle(prompt.title).setMessage(prompt.message)
            .setPositiveButton("OK") { _, _ -> res.complete(prompt.confirm(ButtonPrompt.Type.POSITIVE)) }
            .setNegativeButton("Annuler") { _, _ -> res.complete(prompt.confirm(ButtonPrompt.Type.NEGATIVE)) }
            .setOnCancelListener { res.complete(prompt.dismiss()) }
            .show()
        return res
    }

    override fun onTextPrompt(session: GeckoSession, prompt: TextPrompt): GeckoResult<PromptResponse> {
        val res = GeckoResult<PromptResponse>()
        val champ = EditText(activite).apply { setText(prompt.defaultValue ?: "") }
        dialogue().setTitle(prompt.title).setMessage(prompt.message).setView(champ)
            .setPositiveButton("OK") { _, _ -> res.complete(prompt.confirm(champ.text.toString())) }
            .setNegativeButton("Annuler") { _, _ -> res.complete(prompt.dismiss()) }
            .setOnCancelListener { res.complete(prompt.dismiss()) }
            .show()
        return res
    }

    override fun onChoicePrompt(session: GeckoSession, prompt: ChoicePrompt): GeckoResult<PromptResponse> {
        val res = GeckoResult<PromptResponse>()
        // Les groupes (<optgroup>) sont aplatis : leurs options suivent leur titre.
        val choix = prompt.choices.flatMap { c -> if (c.items != null) c.items!!.toList() else listOf(c) }
            .filter { !it.separator }
        val libelles = choix.map { it.label }.toTypedArray()
        if (prompt.type == ChoicePrompt.Type.MULTIPLE) {
            val coches = choix.map { it.selected }.toBooleanArray()
            dialogue().setTitle(prompt.title)
                .setMultiChoiceItems(libelles, coches) { _, i, v -> coches[i] = v }
                .setPositiveButton("OK") { _, _ ->
                    res.complete(prompt.confirm(choix.filterIndexed { i, _ -> coches[i] }.toTypedArray()))
                }
                .setOnCancelListener { res.complete(prompt.dismiss()) }
                .show()
        } else {
            val actuel = choix.indexOfFirst { it.selected }
            dialogue().setTitle(prompt.title)
                .setSingleChoiceItems(libelles, actuel) { d, i ->
                    res.complete(prompt.confirm(choix[i]))
                    d.dismiss()
                }
                .setOnCancelListener { res.complete(prompt.dismiss()) }
                .show()
        }
        return res
    }
}
