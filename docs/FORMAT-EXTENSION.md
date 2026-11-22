# Format d'extension Trame

> **Deux moteurs coexistent.** Celui décrit ici est le moteur **maison** : manifeste simple,
> injection en fin de chargement, `chrome.storage.local`. Il ne sait pas exécuter une extension
> Chrome. Pour cela, Trame active le moteur de Chromium via WebView2 (`AddBrowserExtension`) et
> charge des extensions **Chrome v2/v3 décompressées** depuis `%LOCALAPPDATA%\Trame\extensions-chrome\` —
> voir [`../exemples-chrome/LISEZMOI.md`](../exemples-chrome/LISEZMOI.md). Les deux
> fonctionnent en parallèle sur les mêmes pages.

Une extension est **un dossier** contenant un `manifest.json` et ses fichiers. Elle est posée
dans `%LOCALAPPDATA%\Trame\extensions\<identifiant>\` — l'identifiant est le **nom du dossier**.

```powershell
# recharger sans redémarrer : Couture (Ctrl+K) → « Recharger les extensions » → ◇
```

## 1. Le manifeste

```jsonc
{
  "name": "Temps de lecture",            // obligatoire, affiché dans l'interface
  "version": "1.0.0",
  "description": "Estime la durée de lecture d'une page.",
  "author": "Trame",
  "matches": ["*://*/*"],                // où l'extension s'applique
  "js": ["contenu.js"],                  // scripts injectés, dans cet ordre
  "css": ["style.css"],                  // feuilles injectées avant les scripts
  "run_at": "document_end",              // seul document_end est honoré (voir § 5)
  "accent": "#5eead4"                    // couleur d'accent, purement décoratif
}
```

| Champ | Requis | Défaut | Remarque |
|---|---|---|---|
| `name` | oui | — | l'extension est ignorée si le manifeste est illisible |
| `version` | non | `""` | affichée à côté du nom |
| `description` | non | `""` | affichée dans le panneau des extensions |
| `author` | non | `""` | informatif |
| `matches` | non | `[]` | sans motif, l'extension ne s'applique **jamais** (signalé au journal) |
| `js` / `css` | non | `[]` | un fichier absent est signalé, les autres sont chargés |
| `run_at` | non | `document_end` | tout autre valeur est **consignée puis ignorée** |
| `accent` | non | — | couleur de la pastille dans l'interface |
| `enabled_by_default` | non | `true` | mettre `false` pour une extension qui **modifie l'apparence des pages** |

> **Une extension qui touche à l'apparence d'un site doit être livrée éteinte**
> (`enabled_by_default: false`). Constaté à l'usage : « Nuit douce » repeignait les conteneurs
> des pages (`article`, `main`, `section`, `pre`, champs) et créait des aplats sombres sur
> Wikipédia. Une feuille injectée ne doit toucher que la racine (`html`, `body`), les couleurs de
> texte, et doit se retirer d'elle-même si la page est déjà sombre.
>
> Le choix d'activer ou de désactiver une extension est **persisté** dans `reglages.json`
> (`extensions_etat`, une carte `identifiant → allumée ?`) : un interrupteur coupé **ou allumé**
> le reste au redémarrage. C'est ce choix qui prime, dans les deux sens — `enabled_by_default`
> ne donne que le défaut d'une extension jamais touchée.
> Les données d'une extension se remettent à zéro depuis **Réglages → Extensions → Réinitialiser**
> (efface `ext-storage.json` pour cette extension).

## 2. Les motifs d'URL

Forme : `<schéma>://<hôte><chemin>`

```
*://*/*                      tout, partout
*://*.exemple.fr/*           exemple.fr et tous ses sous-domaines
https://exemple.fr/wiki/*    https seulement, chemin sous /wiki/
https://exemple.fr/*.pdf     chemin terminant par .pdf (glob sur le chemin)
```

- `*` en schéma = n'importe lequel (`http`, `https`…).
- `*.exemple.fr` correspond à `exemple.fr` **et** `sous.exemple.fr` — mais **pas** à
  `exemple.fr.evil.com` (la comparaison se fait sur le suffixe de domaine, pas sur la chaîne).
- Le chemin est un glob simple (`*` seulement : pas de `?` ni de classes).

## 3. Le code injecté

Chaque script est exécuté dans une fonction qui reçoit **son propre objet `chrome`** :

```js
// contenu.js — le corps du fichier est enveloppé par Trame
const texte = document.body.innerText;
await chrome.storage.local.set({ dernierTitre: document.title });
```

L'injection a lieu **une fois par chargement de page**, après le chargement du document. Un
garde-fou (`window.__trame_ext[<id>]`) empêche une double exécution si l'événement de fin de
chargement se produit deux fois.

Le CSS est inséré dans une balise `<style data-trame-ext="<id>">`, donc retirable par script.

## 4. L'API `chrome` réellement disponible

```ts
chrome.runtime.id: string
chrome.runtime.getManifest(): { name, version, description, author }
chrome.runtime.lastError: null            // toujours null : présent pour la compatibilité

chrome.storage.local.get(clés?, rappel?): Promise<objet>
chrome.storage.local.set(objet, rappel?): Promise<void>
chrome.storage.local.remove(clés, rappel?): Promise<void>
chrome.storage.local.clear(rappel?): Promise<void>

chrome.dom.style(css): HTMLStyleElement   // raccourci propre à Trame
```

`clés` accepte les formes habituelles : `null` (tout), chaîne, tableau, ou objet de valeurs par
défaut. Le stockage est **cloisonné par extension** et **persisté** dans `ext-storage.json` du
profil : il survit aux redémarrages.

### Ce qui n'existe pas — volontairement

| Absent | Pourquoi |
|---|---|
| page d'arrière-plan, `runtime.onMessage` | demanderait un cycle de vie de service worker : non implémenté, donc non annoncé |
| `chrome.tabs.*`, `chrome.webRequest.*`, `chrome.cookies.*` | nécessitent un modèle de permissions et une API privilégiée ; une imitation partielle serait un piège |
| `document_start` | l'injection se fait en fin de chargement ; le déclarer lève un avertissement au journal au lieu d'un faux-semblant |
| popup d'action, icônes, badges | l'interface d'extension se limite au panneau ◇ de la barre |
| permissions déclaratives | pas encore de modèle de permissions : **toute extension installée peut lire toutes les pages qu'elle cible** — à garder en tête avant d'en installer une tierce |

## 5. Cycle de vie

1. Au démarrage, Trame lit `%LOCALAPPDATA%\Trame\extensions\*\manifest.json`.
2. Si le dossier du profil est vide, les extensions livrées avec l'application y sont **copiées une fois**.
3. À la fin de chaque chargement de page, pour chaque extension active dont un motif correspond,
   le CSS puis les scripts sont injectés.
4. Désactiver une extension dans l'interface prend effet **au chargement suivant** (les pages
   déjà ouvertes ne sont pas modifiées).

## 6. Gabarit minimal

```
mon-extension/
  manifest.json
  contenu.js
  style.css
```

`manifest.json` :

```json
{
  "name": "Mon extension",
  "version": "0.1.0",
  "description": "Ce qu'elle fait, en une phrase.",
  "matches": ["*://*/*"],
  "js": ["contenu.js"],
  "css": ["style.css"],
  "run_at": "document_end"
}
```

`contenu.js` :

```js
console.log("Bonjour depuis", chrome.runtime.getManifest().name, "sur", location.hostname);
const { compteur } = await chrome.storage.local.get(["compteur"]);
await chrome.storage.local.set({ compteur: (compteur || 0) + 1 });
```

## 7. Déboguer

- **Journal de l'application** : `%LOCALAPPDATA%\Trame\trame.log` — les manifestes invalides, fichiers
  manquants, motifs impossibles et `run_at` non honorés y sont écrits.
- **Console de la page** : `F12` (build debug) sur l'onglet concerné ; les erreurs d'injection
  sont préfixées `[trame] <id>`.
- Une extension cassée n'empêche pas les autres de se charger : c'est voulu.
