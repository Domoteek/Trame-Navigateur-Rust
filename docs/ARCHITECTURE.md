# Architecture

> Ce document décrit la structure réelle du code, les décisions qui ont des conséquences, et
> **les trois pièges qui ont coûté le plus de temps** — pour qu'ils ne soient pas repayés.

## 1. Vue d'ensemble

```
┌──────────────────────────────────────────────────────────────┐
│ fenêtre tao (une seule)                                      │
│ ┌──────────┬──────────────────────────────────────────────┐   │
│ │  rail    │  barre (la Couture)                          │   │  <- 2 webviews "interface"
│ │  292 px  ├──────────────────────────────────────────────┤   │
│ │          │                                              │   │
│ │  fils    │  contenu : webview de l'onglet actif         │   │  <- N webviews "page"
│ │  onglets │            ou vue Trame (Ctrl+Espace)        │   │
│ └──────────┴──────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
```

Quatre types de webviews. Elles ne se recouvraient jamais jusqu'au 2026-11-21 ; depuis, le menu
des extensions, la Couture et la fenêtre d'extension se posent **par-dessus** la page (voir 1.2) :

| Surface | Rôle | Taille |
|---|---|---|
| `rail` | fils, onglets, vitalité, budget, capsules | 292 × H, replié à 64 sous 1200 px de large |
| `barre` | navigation + Couture + **barre d'extensions** + panneau | (L−292) × 76 (46 + 30), **420 quand le panneau s'ouvre** |
| `vue` | la trame tissée, créée à la demande | zone de contenu |
| onglet | la page, une par onglet **réveillé** | zone de contenu |

La barre d'extensions occupe un second rang de 30 px (`shell::EXT_H`) : la barre haute est donc
de 76 px par défaut, 46 si la barre d'extensions est masquée dans les réglages. C'est la même
géométrie à trois zones, avec une hauteur de barre variable.

**Pages internes** : `trame://localhost/newtab.html` (accueil), `trame://localhost/reglages.html`
(panneau de contrôle), `trame://localhost/trame.html` (vue tissée). Elles sont reconnues par
`urls::nom_page_interne` (nom d'onglet) et par la règle d'autorisation IPC : une page `trame:`
peut envoyer des commandes, une page distante non.

Un **garde-fou de taille** tourne à chaque battement : si `window.inner_size()` diffère de la
dernière taille appliquée (événement de redimensionnement perdu, changement d'écran ou de DPI),
la géométrie est réappliquée. Sans lui, une webview garde son ancienne taille et la page est
rognée.

Le panneau de la Couture **agrandit la barre** (jusqu'à 560 px), mais la page **ne bouge plus** :
elle commence sous la barre de base, et la barre agrandie se pose dessus (voir 1.2).

### 1.1 Le panneau de droite a deux métiers

Le même webview (`self.zap`, 330 px de large, 40 % de la fenêtre au plus) sert à deux choses,
et `panneau_extension: Option<String>` dit laquelle :

| Mode | Contenu | Ouverture | Fermeture |
|---|---|---|---|
| zappeur | `trame://localhost/zap.html` — masquer pubs et Shorts | bouton ⚡ de la barre | bouton ⚡ |
| extension épinglée | `chrome-extension://<id>/<popup>` | puce de l'extension dans la barre | la puce, ou ⚡ |

En mode extension, le panneau prend **la taille du contenu** de la fenêtre d'action (mesurée à
chaque battement, bornes de Chrome : 800 × 600 au plus), accrochée sous la barre à droite, par-dessus
la page ; un clic dans la page la referme. Dans un fil cloisonné, le panneau est recréé dans le
profil de la cloison.

### 1.2 Superposition : `WS_CLIPSIBLINGS`, ordre d'empilement, région

Ce qui empêchait toute superposition n'était pas WebView2 mais le style des fenêtres filles : wry
les crée en `WS_CHILD | WS_CLIPCHILDREN`, sans `WS_CLIPSIBLINGS`, et deux sœurs qui se chevauchent
se dessinent alors dans un ordre quelconque. `App::superposer`, appelé après chaque changement de
géométrie et chaque création de webview (wry crée chaque webview au premier plan) :

1. donne `WS_CLIPSIBLINGS` à toutes les webviews (`fenetres::clip_soeurs`) ;
2. met au premier plan le panneau de droite s'il est ouvert, puis la barre ;
3. découpe la barre agrandie (`fenetres::region`) : la ligne du haut, plus la colonne du menu
   (position envoyée par l'interface, coins bas arrondis), ou toute la largeur pour les résultats
   de la Couture. Hors région, la fenêtre n'existe pas : la page se voit et reçoit les clics.

La géométrie (`shell::geometrie`) ne retranche plus le panneau de droite de la page et place la
page sous la barre **de base**, pas sous la barre agrandie.

4. dessine les **ombres** (`ombre::Ombre`, deux instances : menu et fenêtre d'extension) : une
   fenêtre popup `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE`, possédée par la fenêtre de
   Trame, mise à jour par `UpdateLayeredWindow` avec un bitmap 32 bits dont l'alpha est un anneau
   autour de la forme (nul dedans, nul au-dessus du bord haut, décroissant sur 18 px). Recalée à
   chaque `superposer` et sur `WindowEvent::Moved` ; cachée quand la fenêtre est réduite. Ne se
   redessine que si la forme ou sa position à l'écran a changé.

Épingler une extension = `Reglages::extensions_epinglees` (identifiants, persistés). La puce
n'apparaît que si son manifeste a un `action.default_popup` ; elle porte l'initiale de
l'extension et son nom, et son état « ouvert » (`.on`) montre que le panneau lui appartient.

Deux pièges corrigés en chemin, tous deux visibles à l'usage :

- les puces de la barre étaient reconstruites à chaque poussée d'état (`textContent = ""`),
  donc un clic dont le relâchement tombait pendant la reconstruction était **perdu** : l'action
  part désormais sur `pointerdown`, et une signature d'affichage évite la reconstruction inutile ;
- une extension localisée n'a pas de nom lisible dans son manifeste (`__MSG_appName__`,
  `__MSG_extDesc__`) : `localiser_champ` résout `_locales/<langue>/messages.json`, insensible à
  la casse comme le fait Chromium.

Sonde de diagnostic : `TRAME_PANNEAU=<id>` ouvre la popup d'une extension au premier battement.
Elle existe parce que vérifier ce panneau à la souris au pixel près s'est révélé trompeur trois
fois de suite ; une variable d'environnement donne une preuve reproductible.

## 2. Modules

| Fichier | Responsabilité | Ne connaît pas |
|---|---|---|
| `main.rs` | boucle d'événements, fenêtre, fils de battement (1 s) et de mesure (4 s) | la logique d'onglets |
| `app.rs` | l'état, les surfaces, les commandes, l'IPC, l'injection | la géométrie exacte |
| `shell.rs` | calcul des trois rectangles, **en pixels physiques** | les webviews |
| `model.rs` | types persistés (onglet, fil, capsule, session, manifeste) | l'interface |
| `vitality.rs` | décroissance, recharge, seuils, paliers | la persistance |
| `weave.rs` | score de rattachement, nommage, suggestion de fusion | le rendu |
| `search.rs` | pertinence floue, classement, commandes | le DOM |
| `extensions.rs` | lecture des manifestes, motifs, injection, amorce `chrome` | les webviews |
| `protocol.rs` | protocole `trame:` (fichiers de `ui/`) | l'application |
| `profile.rs` | session, historique, capsules, stockage, **écritures atomiques** | le rendu |
| `probe.rs` | mémoire réelle du processus + descendants (sysinfo) | le reste |
| `urls.rs` | saisie → URL, domaine enregistrable, affichage court | le reste |
| `log.rs` | journal fichier + stderr | le reste |

Règle de dépendance : `model`, `vitality`, `weave`, `search`, `urls` sont **purs** (aucune
dépendance à `wry`/`tao`) et couverts par 23 tests unitaires. Tout ce qui touche à WebView2 est
concentré dans `app.rs`.

## 3. Cycle de vie d'un onglet

```
ouvrir_onglet(url)  →  tissage (weave::evaluer)  →  Onglet { fil, parent, declencheur }
                    →  reveiller(id) : création de la webview si absente
                    →  activer(id)   : set_visible(true) sur celle-ci, false sur les autres
```

- **Endormir** = `self.webviews.remove(&id)` : la `WebView` est détruite, le processus de rendu
  disparaît. L'onglet garde `url`, `titre`, `defilement`.
- **Réveiller** = recréer la webview sur `url`, puis restaurer `scrollTo(0, defilement)` à la fin
  du chargement.
- Le drapeau `eveille` est un **état d'exécution** (`#[serde(skip)]`) : il n'est jamais persisté.
  La sauvegarde travaille sur une **copie** de la session — écrire sur l'état vivant effaçait les
  drapeaux et faisait afficher « 1 éveillé · 1 en veille » pour un seul onglet (bug corrigé).

## 4. Protocole IPC

Deux sens, un seul format : des objets JSON à clé `m`.

**Interface → hôte** (`window.ipc.postMessage`), avec contrôle d'autorisation :

| Message | Effet |
|---|---|
| `pret`, `hello` | l'hôte pousse l'état complet |
| `naviguer {url}`, `nouvelOnglet {url?}` | normalisation (`urls::normaliser`) puis navigation |
| `activer/endormir/reveiller/fermer {id}` | cycle de vie |
| `ancrer`, `alire` | marqueurs durables |
| `filRenommer`, `filReplier`, `tisser`, `fusionner`, `filFermer` | tissage |
| `requete {rid, q}` | la Couture ; réponse par `window.trame.resultats({rid, …})` |
| `panneau {open}` | agrandit/réduit la barre |
| `sceller`, `capsuleOuvrir {chemin}` | capsules |
| `extCall {rid, ext, op, payload}` | stockage des extensions (réponse ciblée) |
| `raccourci {action}`, `defilement {y}` | événements de page |

**Hôte → interface** : uniquement trois points d'entrée, définis par la page.

```js
window.trame.etat(étatComplet)   // poussé à chaque changement structurel
window.trame.budget(...)         // toutes les 4 s si la mesure bouge de plus de 8 Mo
window.trame.resultats({rid, …}) // réponse à une requête de la Couture
window.trame.toast(texte), window.trame.focusCouture()
```

**Sécurité.** Une page distante peut appeler `window.ipc.postMessage` (wry injecte `window.ipc`
sur toutes les pages). Le cœur applique donc une **liste blanche par source** : depuis un onglet
(`Source::Onglet`), seuls `extCall`, `defilement`, `raccourci`, `pret`, `hello` passent. Les
commandes ne sont acceptées que depuis les surfaces d'interface, ou depuis nos pages internes
(`trame:`). Côté interface, **aucun titre de page n'est injecté par `innerHTML`** : un titre est
une chaîne hostile, et il s'exécuterait dans l'origine qui a le droit de commander le navigateur.

## 5. Le protocole `trame:` — et pourquoi pas `file://`

L'interface est servie par `with_custom_protocol("trame", …)` depuis `ui/`. Sous Windows, wry
réécrit `trame://localhost/x` en `http://trame.localhost/x` ; `protocol::chemin_demande` lit
donc le **chemin** de l'URI, et `retranscrire_interne` refait le chemin inverse pour que les
pages internes restent reconnaissables.

**Piège n° 1, mesuré le 2026-11-20.** Une page chargée en `file://` reçoit bien `window.ipc`,
`postMessage` ne lève **aucune** erreur… et le message n'arrive **jamais** au gestionnaire.
Origine opaque. La même page servie par une origine http fonctionne. Diagnostic :

```
[spike] IPC CONTENU recu, uri=https://example.com/     <- https : OK
(rien)                                                  <- file:// : perdu, silencieusement
```

Toute l'interface est donc servie par `trame:`, jamais par `file://`.

## 6. Persistance

`%LOCALAPPDATA%\Trame\` (ou `TRAME_PROFIL`) :

| Fichier | Contenu | Écriture |
|---|---|---|
| `session.json` | onglets, fils, actif, rail replié | atomique, différée de 3 s max |
| `history.jsonl` | une entrée par page chargée (5 000 max en mémoire) | ajout en fin de fichier |
| `ext-storage.json` | stockage par extension | atomique, à chaque écriture |
| `capsules\*.json` | fils scellés | atomique |
| `extensions\` | extensions installées | copiées du dépôt au premier lancement |
| `trame.log` | journal | ajout |

« Atomique » = fichier `.tmp`, `sync_all`, puis `rename`. Une coupure de courant ne doit jamais
laisser un `session.json` tronqué : c'est précisément le moment où l'on a besoin de la session.

### 6 bis. Sauvegarde et restauration : pourquoi au démarrage

`src/sauvegarde.rs` exporte le profil dans un fichier `.trame` (du JSON lisible) et le réapplique.
Trois points d'architecture, chacun imposé par une contrainte réelle :

1. **La restauration ne s'applique pas depuis la fenêtre ouverte.** Trame réécrit
   `session.json` à son arrêt (`App::sauver`, appelé sur `CloseRequested` et `LoopDestroyed`) :
   restaurer à chaud serait donc écrasé à la fermeture. La commande `sauvegardeRestaurerConfirmer`
   écrit `restauration-en-attente.json`, met `App::quitter` à vrai ; la boucle d'événements
   (`main.rs`) pose `ControlFlow::Exit` ; et le **démarrage suivant** appelle
   `sauvegarde::appliquer_restauration_en_attente` juste après la prise du verrou et **avant**
   `App::nouveau` — donc avant la lecture de la session et avant la création du contexte WebView2.
2. **Le disque n'est jamais écrasé sans copie.** `archiver_avant` copie les fichiers concernés
   dans `avant-restauration-<date>\` avant d'écrire. Même règle que `.migre` et
   `extensions-chrome-archives\` : on archive, on ne détruit pas. Si la copie échoue, la
   restauration est annulée — rien n'a été touché.
3. **Une archive venue d'ailleurs ne peut écrire que des contenus connus.** `appliquer` filtre les
   clés contre `CONTENUS`, ignore `webview2\`, et refuse un nom de capsule contenant `/`, `\` ou
   `..`. Une archive est un fichier qu'on peut recevoir de quelqu'un.

L'empreinte (`empreinte` de l'en-tête) est le SHA-256 du bloc `donnees` **sérialisé par
`serde_json`**, dont la `Map` est une `BTreeMap` : les clés sont donc triées à tous les niveaux.
Un recalcul qui ne trie pas les clés imbriquées trouve une empreinte différente — c'est un
avertissement au journal, pas un refus (mesuré le 2026-11-22 : le banc d'essai `z` a échoué
exactement là avant correction du calcul côté test).

Le dialogue de fichier de Windows **bloque** le fil appelant : enregistrement, choix de l'archive
et échange des favoris passent donc par un fil annexe, comme l'installation d'une extension
(`installation.rs`), et le résultat revient par `UserEvent::Sauvegarde`.

## 7. Géométrie

Les tailles de conception sont **logiques** (292, 46, 420) et converties en **physique** par le
facteur d'échelle. `wry::Rect` accepte les deux ; passer du logique sans conversion décale toute
l'interface à 125 % ou 150 %. `shell::geometrie` garantit aussi la partition : pas de
recouvrement, et il reste toujours du contenu visible quand le panneau est ouvert.

## 8. Piège n° 2 : un `const ipc` tue tout le script

wry définit `window.ipc` via `Object.defineProperty(window, 'ipc', { value: … })` **sans
`configurable: true`**. Une déclaration lexicale de premier niveau du même nom jette donc une
`SyntaxError` — et **tout le fichier ne s'exécute pas** :

```
Uncaught SyntaxError: Identifier 'ipc' has already been declared  (chrome.js:1)
```

Résultat observé : écran noir, aucune erreur visible, et une interface muette. Les trois scripts
d'interface sont donc enfermés dans une fonction (`(function(){ … })()`). Corollaire : une erreur
JavaScript dans l'interface doit être **remontée à l'hôte** — c'est ce que fait `AMORCE_UI`
(`window.onerror` → IPC `erreurUi` → journal).

## 9. Piège n° 3 : une adresse interne de Chromium tue la webview

`chrome-search://local-ntp/local-ntp.html` est la page d'accueil interne du moteur, celle que
Chromium fabrique lui-même. WebView2 **refuse de la servir** :

```
Désolé, impossible d'accéder à cette page.        ERR_INVALID_URL
chrome-search://local-ntp/local-ntp.html
```

Et la webview ne s'en remet pas : elle n'affiche plus rien, et il n'y a pas de bouton retour. Le
2026-11-21, le panneau de droite (popup Bitwarden) s'est retrouvé là — un `window.open` (le bouton
« détacher » de la popup) a laissé WebView2 suivre sa gestion par défaut au lieu d'ouvrir un
onglet.

Trois garde-fous, chacun à un étage différent :

| Étage | Ce qui est fait | Effet |
|---|---|---|
| `urls::interne_chromium` | reconnaît `chrome-search:`, `chrome://`, `chrome-untrusted:`, `devtools:`, `edge:`, `view-source:` (mais **pas** `chrome-extension://`) | décision unique, testée |
| `App::garde_navigation` | `with_navigation_handler` **refuse** la navigation, sur les quatre surfaces et sur les onglets | la page reste en place, le refus part au journal |
| popup d'extension dans le panneau | `with_new_window_req_handler` → `NouvelleFenetre` → onglet Trame, `Deny` | `window.open` ouvre un onglet |
| `ouvrir_onglet` / `restaurer` | toute adresse interne devient l'accueil | rien de mort ne se restaure |

Preuve mesurée : `TRAME_NAV=chrome-search://local-ntp/local-ntp.html` →
`navigation refusee (adresse interne de Chromium)`, et l'onglet garde sa page.

## 10. L'icône

Trois pièces, et les trois sont nécessaires :

| Pièce | Rôle |
|---|---|
| `ressources/trame.ico` | 10 tailles (16 → 256) ; `build.rs` l'intègre à l'exécutable via `winresource` (explorateur, barre des tâches, alt-tab) |
| `ressources/trame-64.rgba` | pixels bruts 64×64 ; `main.rs` les passe à `tao::window::Icon::from_rgba` pour la fenêtre elle-même |
| `outils/faire-ico.mjs` | assemble le `.ico` à partir des PNG et **relit le fichier** pour vérifier que chaque entrée est bien un PNG |

Sans le `.ico` embarqué, l'exécutable garde l'icône générique de Rust. Sans l'icône de fenêtre,
la barre de titre garde celle du runtime même si l'exécutable est correct. Les deux sont donc
posées.

Deux détails qui coûtent du temps si on les ignore : un dégradé SVG sur une ligne **horizontale**
a une boîte englobante de hauteur nulle et ne s'affiche pas (utiliser `gradientUnits="userSpaceOnUse"`) ;
et à 16 px, trois fils colorés de 2 px se confondent — la variante petite taille est monochrome
et sans fioritures.

## 12. Deux moteurs d'extensions
| | Moteur maison (`extensions.rs`) | **Moteur Chromium (WebView2)** |
|---|---|---|
| Dossier | `profil/extensions/` | `profil/extensions-chrome/` |
| Activation | always | `with_browser_extensions_enabled(true)` |
| Installation | lue à chaque démarrage | `with_extensions_path(...)` → `ICoreWebView2Profile7::AddBrowserExtension` **une fois par profil** |
| Exécution | `evaluate_script` en fin de chargement | le moteur d'extensions de Chromium |

Trois points de vigilance, appris dans la source de wry 0.57 :

1. **Le chemin ne doit être transmis qu'à la première webview.** `load_extensions` installe
   *chaque sous-dossier* du chemin au moment de la création ; le redonner à chaque onglet
   réinstallerait toutes les extensions à chaque ouverture. D'où le drapeau
   `extensions_chrome_amorcees` et la méthode `habiller()`.
2. **Toutes les webviews doivent avoir le même réglage.** WebView2 refuse la connexion à un
   environnement déjà lancé si `AreBrowserExtensionsEnabled` diffère (`ERROR_INVALID_STATE`) :
   `habiller()` est donc appliqué partout, surfaces d'interface comprises.
3. **Les surfaces d'interface sont des pages web.** Un `content_scripts` en `<all_urls>` s'y
   injecte comme dans n'importe quelle page : c'est pourquoi une extension de test a affiché sa
   bannière au-dessus du rail. Aucun filtrage n'est possible côté WebView2 — c'est une limite à
   documenter, pas un bug à corriger.

## 13. Méthode de diagnostic qui a fonctionné

- **Une webview ne se devine pas, elle se mesure** : `EnumChildWindows` sur la fenêtre donne les
  classes et les rectangles réels (`WRY_WEBVIEW`, `Chrome_RenderWidgetHostHWND`), et
  `WindowFromPoint` dit quelle fenêtre recevrait un clic. Deux fois, un « bug de rendu » était en
  réalité une **autre application** au premier plan ou une fenêtre déplacée.
- **Le journal d'abord** : `trame.log` et stderr. Le canal IPC a été trouvé cassé en journalisant
  chaque requête servie par le protocole (uri, type MIME, taille).
- **Sonder depuis l'hôte** : `evaluate_script_with_callback` permet de lire `typeof window.trame`,
  `document.scripts.length`, ou de poser une sonde dans la page — sans dépendre de l'interface,
  qui est justement ce qui est cassé.

## 14. Installer une extension : ce que fait vraiment le magasin de Chrome

Longtemps écrit ici et dans la fiche : « l'installation en un clic est impossible, WebView2 ne
charge que des extensions décompressées ». La première moitié est vraie, la seconde conclusion
était **fausse** : un `.crx` du magasin **est** une archive décompressable, il suffit de retirer
son en-tête de signature. Mesuré le 2026-11-21.

Trois modules, chacun à son étage :

| Module | Rôle | Testé par |
|---|---|---|
| `crx.rs` | format CRX2/CRX3/ZIP, identifiant Chromium (SHA-256 de la clé publique → 32 lettres `a`–`p`), extraction de l'identifiant depuis un lien | tests unitaires, **recoupés avec le `crypto` de Node** |
| `systeme.rs` | trois ponts Windows : `HttpClient` (téléchargement), `Expand-Archive` (décompression), `OpenFileDialog` (choix d'un fichier) | scénarios réels seulement |
| `extensions_chrome.rs` | trois dossiers, activation par déplacement, archives, permissions traduites, provenance | tests unitaires + scénario réel |

**Pourquoi PowerShell et pas des crates.** Trame n'a ni pile TLS ni décompresseur ZIP : les
ajouter pour trois opérations ponctuelles changerait la nature du binaire. Windows fournit les
deux, et le projet appelle déjà `explorer` et `taskkill`. Le coût est dit : ces fonctions ne sont
pas couvertes par les tests unitaires, seulement par les scénarios de `outils/verifier-app.mjs`.

**Le point qui a failli être manqué** : l'identifiant. WebView2 nomme une extension d'après la
clé publique de son manifeste, pas d'après son dossier. Sans le SHA-256 de cette clé, la popup
d'une extension installée depuis un fichier ne s'ouvrirait pas (`chrome-extension://<id>/popup`
répondrait 404). `crx::identifiant_depuis_cle` implémente donc la règle exacte de Chromium, avec
les vecteurs FIPS 180-4 en test.

**Ce qui n'est pas vérifié** : la signature. Le magasin signe ses paquets, WebView2 ne contrôle
rien. L'interface le dit (« la signature n'est pas vérifiée ») au lieu de laisser croire à une
installation validée.

**Les trois dossiers**, et pourquoi des dossiers plutôt qu'un état en mémoire :

```
extensions-chrome\             chargée au démarrage
extensions-chrome-desactivees\ coupée (dossier déplacé)
extensions-chrome-archives\    retirée ou remplacée, jamais effacée
```

Il n'existe aucune API WebView2 pour désactiver, retirer ou mettre à jour une extension — wry
n'expose même pas le résultat de `AddBrowserExtension`. Déplacer un dossier est le seul mécanisme
honnête, et il est réversible à la main.

## 15. Tout ce dont le navigateur a besoin vit dans le profil

Deux dépendances traînaient hors de `%LOCALAPPDATA%\Trame\`, et les deux étaient des défauts :

| Ce qui traînait | Où | Pourquoi c'était grave |
|---|---|---|
| l'interface (`ui/`) et les extensions livrées | à côté de l'exécutable, ou dans le dossier du projet | binaire déplacé → plus d'interface ; `cargo clean` → plus rien |
| le profil WebView2 (`trame.exe.WebView2`) | **dans `target\release`** | un nettoyage de build effaçait les cookies, les connexions et le stockage des extensions — la connexion Bitwarden disparaissait |

Le profil vivait aussi dans **Roaming** (`%APPDATA%`), par héritage : 355 Mo de caches et de
profils WebView2 dans un dossier itinérant. Il est désormais dans **`%LOCALAPPDATA%`** — donnée de
machine, pas de compte — et le déplacement Roaming → Local se fait au premier démarrage
(`profile::rapatrier_racine` : renommage du dossier entier, copie en repli, et **rien n'est écrasé**
si la destination contient déjà une session).

Désormais :

| Mécanisme | Module | Détail |
|---|---|---|
| installation de l'interface | `ressources.rs` | **miroir** `ui/` → profil : copie, mise à jour, suppression des fichiers retirés de la livraison ; comparaison octet par octet, donc un démarrage normal n'écrit rien |
| profil WebView2 | `profile.rs` | `dossier_webview2()` = `<profil>\webview2` ; `rapatrier_webview2()` déplace (même volume) ou copie l'ancien dossier puis le renomme `.migre` — en cas d'échec il **garde l'ancien emplacement** plutôt que de perdre une session |
| contexte partagé | `app.rs` | `static CONTEXTE_WEBVIEW: OnceLock<Mutex<WebContext>>` créé une fois sur ce dossier ; toutes les webviews passent par `WebViewBuilder::new_with_web_context` (WebView2 exige un seul environnement par dossier de données) |

L'interface est un miroir, **les extensions Trame non** : elles sont amorcées une seule fois, car
l'utilisateur a le droit de les modifier. Les extensions Chrome, elles, sont gérées par déplacement
(§ 14).

Ce que cela coûte : `WebContext` doit être emprunté pour construire chaque webview, donc chaque
site de création fait `let mut contexte = verrou_contexte();` et le garde jusqu'au `build`. Le
verrou n'est jamais tenu au-delà : la boucle d'événements est sur un seul fil.
