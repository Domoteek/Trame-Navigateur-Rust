# Trame — le cours des évolutions

Ce document raconte **dans l'ordre** ce qui a été construit, corrigé et vérifié, avec pour chaque
étape : ce qui a changé, pourquoi, et la preuve. Il ne remplace pas les autres documents — il les
met en perspective.

| Document | Rôle |
|---|---|
| `docs/CONCEPT.md` | l'idée (le fil d'attention, la vitalité, la veille) |
| `docs/ARCHITECTURE.md` | le code réel, les décisions et les pièges |
| `docs/EVOLUTIONS.md` | **ce document** : la chronologie |
| `00-pilotage/fiches/trame.md` | la fiche du projet (état, mesures, tests) |

Les horodatages viennent des fichiers et de `%LOCALAPPDATA%\Trame\trame.log`.

---

## 1. 2026-11-20, 22 h 30 → 23 h 05 — la naissance

Le socle, en une soirée : une coquille `tao` + `wry` sur WebView2, et une interface entièrement
locale servie par un protocole maison.

| Évolution | Pourquoi |
|---|---|
| **Trois surfaces** (rail 292 px, barre 46 px, contenu) | deux webviews Win32 ne se composent pas : la seule façon de tenir plusieurs zones est de ne jamais les superposer |
| **Protocole `trame:`** | mesuré : en `file://`, `window.ipc` existe, `postMessage` ne lève rien, et le message n'arrive **jamais**. Toute l'interface passe donc par une origine http locale |
| **Fils, vitalité, veille** | un onglet n'est pas un document ouvert mais un fil d'attention : il se refroidit (demi-vie 8 min), s'endort (webview détruite, position retenue), et **ne se ferme jamais** |
| **La Couture** (`Ctrl+K`) | une seule saisie : onglets, fils, historique, commandes, adresse |
| **La trame** (`Ctrl+Espace`) | la vue tissée : chaque fil est une bande, le temps va de gauche à droite |
| **Capsules** | un fil scellé en JSON, déplaçable d'une machine à l'autre |
| **Moteur d'extensions maison** | manifeste à nous, motifs d'URL, injection CSS/JS, `chrome.storage.local` persisté |
| **Icône** | `.ico` multi-tailles intégré à l'exécutable, icône de fenêtre en RGBA |
| **Mesure de mémoire réelle** | 660,7 Mo / 10 processus avec 1 onglet, 1 218,8 Mo / 15 avec 6 → ≈ 112 Mo par onglet réveillé. C'est le chiffre qui justifie la veille |

Preuve : trois extensions injectées, Couture qui navigue, vue trame dessinée, redimensionnement
propre.

## 2. 2026-11-21, 06 h 30 → 07 h 50 — les retours d'usage

Chaque demande de Clément était un vrai défaut, pas un caprice d'interface.

| Demande | Ce qui a été fait |
|---|---|
| « Là tu m'as mis DuckDuckGo, peut-être intégrer Perplexity » | table de **8 moteurs** (`src/urls.rs`), **Perplexity par défaut**, langue française forcée dans l'URL quand le moteur l'accepte |
| « Il me faut un vrai panneau de contrôle » | page interne `trame://reglages` (`Ctrl+,`) : moteur, extensions, veille, budget, historique, chemins |
| « C'est en anglais » (titre de fenêtre) | titres et libellés en français, y compris le nom des pages internes |
| « Si je réduis, ça rogne le site » | géométrie recalculée à chaque battement (garde-fou si l'événement de redimensionnement est perdu), rail replié sous 1200 px |
| « Je voudrais voir une barre d'extension » | second rang de 30 px sous la barre haute : une pastille par extension |
| « Des carrés sombres sur Wikipédia » | l'extension « Nuit douce » ne touche plus que la racine et le texte, avec un garde de luminance — et elle se livre **désactivée** |
| « La pastille de lecture disparaît et je ne peux pas la remettre » | seul le ✕ la coupe, et chaque extension a son bouton **Réinitialiser** |
| « Les commandes ne fonctionnent pas » (Couture) | les lignes étaient reconstruites au survol : le clic n'arrivait jamais. Action sur `pointerdown`, survol sans reconstruction |
| « Le panneau reste ouvert » | un clic dans la page ou le rail le referme (le `blur` n'existe pas entre deux webviews voisines) |
| « L'extension Chrome ne s'active pas » | **second moteur** : WebView2 charge de vraies extensions Chrome (`AddBrowserExtension`) ; import depuis Chrome/Edge ; Bitwarden s'installe, se connecte, affiche 355 entrées |

## 3. 2026-11-21, 07 h 50 → 08 h 30 — la campagne de tests

Avant : 33 tests sur des fonctions pures, **aucun** sur l'interface (700 lignes de JS), **aucun**
sur l'application réelle. Trois étages construits, plus cinq défauts trouvés.

| Étage | Commande | Ce qu'il prouve |
|---|---|---|
| Logique | `cargo test --release` | **102 tests** : URL, géométrie (invariant sur 108 combinaisons), vitalité, tissage, protocole, persistance, modèle, CRX, gestion des extensions |
| Interface | `node outils/tester-ui.mjs` | **33 cas** : le DOM rendu depuis un état poussé, les messages envoyés à l'hôte, un titre hostile qui reste du texte, les 17 raccourcis par vraies frappes, et le contrat « tout message de l'UI est traité par l'hôte » |
| Application | `node outils/verifier-app.mjs` | **13 scénarios** sur le binaire et un profil jetable, journal à l'appui |

Défauts trouvés **par** ces tests, et corrigés :

1. **Profil corrompu = perte silencieuse** : `session.json` illisible démarrait à vide, puis le
   fichier cassé était écrasé à la sortie. Désormais écarté en `*.casse`, avec le chemin au journal.
2. **Un sélecteur du zappeur cassait toutes les règles** : une accolade refermait le bloc CSS.
   Sélecteur tronqué et filtré, `<`/`>` retirés.
3. **Géométrie : 1 pixel sous la fenêtre** : position et taille arrondies séparément. Arrondi
   avant conversion, compléments calculés en entiers, partition exacte.
4. **Vue trame : liens de provenance perdus** selon l'ordre de dessin. Positions calculées en une
   passe avant le dessin.
5. **Deux instances, une session** : la seconde écrasait la première. Verrou `trame.lock` (PID +
   nom de processus, verrou périmé repris).

## 4. 2026-11-21, 08 h 05 → 08 h 30 — quatre fonctionnalités manquantes

| Ajout | Pourquoi | Preuve |
|---|---|---|
| **Zoom par onglet** (`Ctrl+ +` / `-` / `0`) | `SetZoomFactor` existe : inutile de bricoler un `zoom` CSS qui casse les mises en page | journal `zoom onglet 17 : 120 %`, facteur réappliqué au réveil |
| **Recherche dans la page** (`Ctrl+F`) | relire une longue page est l'usage premier d'un navigateur « à lire » | barre injectée (`ui/recherche.js`), `window.find`, compteur, `Échap` rend le clavier |
| **Téléchargements** | sans gestionnaire, WebView2 ouvrait un panneau invisible : le clic ne produisait **rien** | scénario `j` : un vrai fichier en pièce jointe arrive dans `<profil>\telechargements\`, jamais écrasé |
| **Verrou de profil** | empêche la perte de session entre deux instances | scénario `h` |

## 5. 2026-11-21, 08 h 30 → 09 h 00 — les extensions vraiment gérables

Clément : « fais en sorte que les extensions puissent totalement gérables, installables ». Cela a
commencé par **corriger une affirmation fausse** de ma part.

> J'avais écrit que « l'installation en un clic depuis le Chrome Web Store est impossible ». Faux :
> un `.crx` est un **ZIP précédé d'un en-tête de signature**. Le magasin le sert très bien — à
> condition de se présenter comme un client récent (`204` en `prodversion=131`, `200` en `141`).

| Évolution | Détail |
|---|---|
| **`src/crx.rs`** | CRX2, CRX3 et ZIP nu ; extraction de l'identifiant depuis un lien ; **identifiant Chromium** = SHA-256 de la clé publique → 32 lettres `a`–`p`, avec SHA-256 et base64 écrits dans le projet et testés (vecteurs FIPS, recoupés avec le `crypto` de Node) |
| **`src/systeme.rs`** | trois ponts Windows : `HttpClient` (téléchargement), `Expand-Archive` (décompression), `OpenFileDialog` (choix d'un fichier) |
| **`src/extensions_chrome.rs`** | trois dossiers (`extensions-chrome`, `-desactivees`, `-archives`), activation par **déplacement**, permissions traduites, provenance, poids |
| **Interface** | champ « identifiant ou lien » + **Installer** ; **Installer un fichier .crx / .zip…** ; par extension : Interface, Panneau, Épingler, Mettre à jour, Désactiver/Activer, **Retirer** — et ce qu'elle demande |

Rien n'est effacé : désactiver déplace, retirer archive, une mise à jour archive l'ancienne version.

Preuves : scénario `l` (CRX3 fabriqué par le test, installé sous l'identifiant recalculé) et
scénario `m` (**uBlock Origin Lite installée depuis le magasin**, dans un profil jetable).

## 6. 2026-11-21, 9 h — tout dans le dossier utilisateur

Dernière demande, et dernier vrai défaut : **deux choses vivaient hors du profil**.

1. L'interface (`ui/`) et les extensions livrées étaient lues à côté de l'exécutable — donc
   introuvables après un déplacement du binaire, et absentes après un `cargo clean`.
2. Le profil WebView2 (`trame.exe.WebView2`, **273 Mo** : cookies, connexions, service workers,
   stockage des extensions) était créé **dans `target\release`**. Un nettoyage de build effaçait
   la connexion Bitwarden.

Le profil vivait en plus dans **Roaming** : 355 Mo de caches dans un dossier itinérant. Il est
passé dans **Local** (donnée de machine), déplacé au premier démarrage — pas recopié. Désormais
tout est sous `%LOCALAPPDATA%\Trame\` : `ui\` (miroir mis à jour au démarrage),
`extensions\`, `extensions-chrome\` (+ désactivées et archives), `webview2\EBWebView\`,
`telechargements\`, `capsules\` et les fichiers d'état. Le profil WebView2 existant a été
**rapatrié** (copie puis renommage de l'ancien en `.migre`, jamais supprimé) : la session
Bitwarden a survécu, vérifié sur le vrai profil et par le scénario `n`.

## 7. 2026-11-21, 11 h — signature Azure et installation

Clément : « signe comme mes autres projets avec Azure », puis « fais-moi un setup ».

- **Signature** : abandon d'un certificat auto-signe (qui affichait « éditeur inconnu » et ne
  valait que sur ce poste) pour l'**outil partagé de l'écosystème**, `_tools\signature\signer.ps1`
  — compte Azure `domoteek`, profil `MesApps`, identifiants dans `50-secrets\` (jamais lus par
  le script de build). Résultat : `Valid`, signataire `CN=CAHAGNE CLEMENT`, horodatage DigiCert.
  L'ordre est respecté : exe signé **avant** la compilation du setup.
- **Installation** : `outils\trame.iss` (Inno Setup 6), installation **par utilisateur** sous
  `%LOCALAPPDATA%\Programs\Trame` (aucun droit administrateur), raccourcis, entrée « Applications »,
  désinstalleur. Plus une archive portable et un `SHA256SUMS.txt`.
- **Ce que l'installation ne touche pas** : le profil `%LOCALAPPDATA%\Trame`. Désinstaller ne
  supprime ni onglets, ni réglages, ni connexions.
- **Vérifié** : `outils\verifier-setup.mjs` installe en silence dans un dossier jetable, contrôle
  fichiers, signature de l'exe **installé**, raccourci, registre, lancement réel, désinstallation,
  et l'intégrité du profil réel — 24 contrôles.

## 8. 2026-11-21, 15 h 45 — l'état des extensions se mémorise vraiment

Clément : « vérifie : il faut que les extensions puissent être activées et se mémorisent dans le
navigateur. » Le mécanisme existait **à moitié**, et la moitié manquante était celle qu'on
rencontre en premier.

| Ce qui marchait | Ce qui ne marchait pas |
|---|---|
| Couper une extension livrée **allumée** : son identifiant entrait dans `extensions_desactivees`, et elle restait coupée | **Allumer une extension livrée éteinte** : rien n'était écrit. « Nuit douce » (`enabled_by_default: false`) se rallumait éteinte à chaque démarrage |

La cause : le choix persisté n'était qu'une **liste de coupures**. Comme le manifeste ne connaît que
son défaut, l'absence dans la liste voulait dire « défaut du manifeste » — donc « éteinte » pour une
extension livrée éteinte. Le choix d'activation n'avait aucun endroit où exister.

| Évolution | Détail |
|---|---|
| **`Reglages.extensions_etat`** | une carte `identifiant -> allumé ?`, qui remplace la liste : le choix de l'utilisateur prime sur le manifeste **dans les deux sens** |
| **Migration** | `extensions_desactivees` reste lu une fois au chargement (`Reglages::migrer_extensions`) : un profil existant ne rallume pas ce qu'on avait coupé ; le champ n'est plus réécrit |
| **Un seul chemin** | `basculer_extension` sert à l'interface **et** à la sonde de test : écrire le choix sur le disque, journaliser, pousser l'état — jamais deux implémentations qui divergent |
| **Journal** | au démarrage, `extension <id> : activee (choix memorise)` et `extensions : N active(s) sur M (K choix memorise(s))` ; l'interface annonce « rechargez la page (Ctrl+R) », l'injection ayant lieu en fin de chargement |

Preuve, scénario `o` de `outils/verifier-app.mjs` : une extension livrée **éteinte** est allumée
(`TRAME_EXT_ETAT=essai`), le test lit `reglages.json`, **redémarre** le binaire, et exige une preuve
d'injection réelle — le script de contenu écrit dans `chrome.storage.local` l'adresse de la page, et
c'est `ext-storage.json` qui est relu. Puis l'extension est coupée, et le silence est vérifié au
redémarrage suivant. Sans le correctif, le scénario échoue au premier contrôle
(`reglages.json` : `{}`).

## 9. 2026-11-21, 16 h — « Ajouter à Chrome » installe vraiment

Clément : « je veux pouvoir installer une extension chrome depuis le store sans problème sur la
version installée du programme. » Le journal du profil réel a donné la réponse en une ligne :

```
[14:13:45Z] telechargement : https://permanently-removed.invalid/?…&x=id%3Dnngceckbapebfimnlniiiahkandclblb%26… -> telechargements\….crx
[14:13:45Z] !! telechargement echoue : https://permanently-removed.invalid/?…
```

Autrement dit : la fiche Bitwarden du magasin était ouverte **dans Trame**, et le clic sur
« Ajouter à Chrome » a fini en **téléchargement** (WebView2 ne sait pas installer depuis une page).
Le magasin a en plus redirigé vers `permanently-removed.invalid` — en gardant l'identifiant dans
l'URL. Le fichier `.crx` ne servait à rien, et un `.crx` de 33 Mo restait dans le dossier.

| Évolution | Détail |
|---|---|
| **`app::suite_telechargement`** | une décision pure, testée hors réseau : *paquet du magasin* → installer ; *magasin + identifiant dans l'URL* → reprendre par notre point d'entrée ; *tout le reste* → téléchargement ordinaire |
| **`crx::du_magasin`** | reconnaît le magasin et son CDN (`clients2.google.com`, `*.googleusercontent.com`) **et** `permanently-removed.invalid` : c'est le cas de refus, sans lequel le clic échouait |
| **`crx::identite`** | lit l'en-tête du CRX : `crx_id` (CRX3) ou clé publique (CRX2), **et la preuve dont l'empreinte donne ce `crx_id`** — donc la clé de l'éditeur |
| **`extensions_chrome::inscrire_cle`** | inscrit cette clé dans le manifeste installé : c'est elle qui fixe l'identifiant que Chromium attribue à une extension chargée depuis un dossier |
| **Garde-fou** | un `.crx` téléchargé depuis un **autre** site n'est pas installé en silence : le scénario `p` le vérifie |

**Ce que la mesure a montré, et qui n'était pas écrit.** Un dossier `mon-dossier` chargé par
`AddBrowserExtension` sans clé `key` reçoit `chrome.runtime.id` = `cfmnfkohahigcphgcoimlckammkneabk`
— l'empreinte du **chemin**. Le même dossier avec la clé du magasin reçoit l'identifiant du magasin
(`eimadpbcbfnmbkopoojfekhnkhdbieeh`). Conséquence : une extension installée depuis un `.crx`
s'appelait `local-dark-reader`, et la fenêtre d'action demandée par l'interface
(`chrome-extension://<id>/…`) ne pouvait pas répondre. La clé corrige les deux à la fois.

Vérifié : `cargo test` (décision, en-tête CRX3 avec preuves, inscription de la clé), scénario `p`
(offline : un `.crx` étranger reste un fichier), scénario `q` (réseau : Dark Reader, 860 Ko,
téléchargée par le magasin, installée sous son identifiant, clé inscrite, provenance « magasin »).

Vérifié **sur la version installée** (`%LOCALAPPDATA%\Programs\Trame\trame.exe`, même empreinte
SHA-256 que `target\release\trame.exe`), dans un profil jetable : le champ **Installer** des
réglages installe Dark Reader en 8 s ; et le chemin « Ajouter à Chrome » en 3 s, le paquet restant
dans `telechargements\`. La fenêtre d'action s'ouvre alors sur `chrome-extension://<identifiant du
magasin>/`, ce que le dossier `local-dark-reader` d'avant ne permettait pas.

Réserve, dite franchement : les fichiers de `dist\` ont été fabriqués avec `--sans-signature`,
l'outil Azure (`D:\LIFE\_tools\signature\signer.ps1`) n'existant pas sur cette machine. La
signature doit être rejouée là où il est présent, avec la même commande.

## 10. 2026-11-21, 17 h 50 → 18 h 20 — les extensions comme dans Chrome, sans redémarrer

Clément : « vérifie que les installations des extensions Chrome fonctionnent sans aucun problème,
que ça se rafraîchisse automatiquement, que je puisse tout voir comme dans Chrome, épingler le petit
icône, l'activation ». L'essai sur le binaire a montré ce qui manquait :

| Constaté | Corrigé |
|---|---|
| Installer uBlock Origin Lite **figeait la fenêtre 24 s** (téléchargement dans la boucle d'événements) | `src/installation.rs` : téléchargement, décompression et sélecteur de fichier dans un fil annexe ; une installation à la fois ; le puzzle porte un badge pendant ce temps |
| Après installation : « **Redémarre Trame** » — l'affirmation que WebView2 ne sait pas charger à chaud était fausse | `src/moteur_ext.rs` : `AddBrowserExtension`, `Enable`, `Remove` et `GetBrowserExtensions` appelés en direct sur le profil. Mesure : Dark Reader installée **et active en 3 s**, son service worker a ouvert sa page d'accueil aussitôt |
| Rien ne se rafraîchissait | après chaque installation, activation ou extinction (Chrome **et** Trame), l'onglet actif est rechargé s'il affiche une page web |
| Le message nommait l'extension `ddkjiahejlhfcafbddmgiahcphecmpfh` | le nom lu dans `_locales` (« uBlock Origin Lite ») |
| Une extension installée n'apparaissait **nulle part** dans la barre ; une épinglée était une lettre dans une pastille | bouton **puzzle** et menu façon Chrome (icône, état réel, épingle, interrupteur) ; les épingles sont **les vraies icônes**, servies par `trame://localhost/ext-icone/…` (images seulement, confinées au dossier de l'extension) |
| Le même message s'affichait deux fois, dont une tassée par-dessus les boutons du rail | un seul endroit : la barre |
| La fenêtre passait sous la barre des tâches (pied du rail caché) sur un écran de 1 080 px | taille bornée à l'écran et fenêtre centrée dans la hauteur utile |
| `reglages.json` enregistré avec un BOM (Bloc-notes, PowerShell 5.1) était jugé illisible et remplacé par les défauts | lecture tolérante au BOM pour tous les fichiers du profil |
| La liste des extensions Chrome (avec le poids de chaque dossier) était recalculée **sur le disque à chaque poussée d'état** | en cache, invalidée à chaque changement |

Un cas limite trouvé en écrivant le test : une extension déposée **sans clé** reçoit du moteur
l'empreinte de son chemin comme identifiant. Les opérations à chaud la retrouvent alors par son
nom ; le scénario `r` exige que le moteur la liste réellement « eteinte » puis « active ».

### 10 bis. 18 h 40 — la mémoire basse

Clément : « qu'elle soit légère en mémoire ». Un palier entre « ouvert » et « en veille » : un
onglet caché depuis 20 s passe en `MemoryUsageTargetLevel = Low` et revient à la normale au clic.
Mesure A/B sur le même binaire : **−94 à −192 Mo de RAM** avec 5 onglets cachés ; la mémoire
réservée, elle, ne baisse pas (détail et limites : `docs/OPTIMISATIONS.md` § 1.4). Réglage
débrayable, allumé par défaut ; scénario `s`.

### 10 ter. 19 h — les fils cloisonnés

Clément a choisi, parmi trois conceptions, des **extensions par fil**. Une vérification a décidé de
tout : WebView2 n'active une extension que pour un **profil** entier, jamais pour un onglet. Un fil
qui veut ses propres extensions doit donc avoir son propre profil — et ses propres connexions.

| Évolution | Détail |
|---|---|
| `Fil.cloison` | nom de profil (`cloison-<id>`) + extensions voulues ; absent = profil principal, les anciennes sessions se relisent |
| Onglets du fil | créés avec `with_profile_name` ; à la création, `moteur_ext::reconcilier` aligne le profil sur les extensions voulues (ajoute, retire, rallume) |
| Rail | bouclier dans l'en-tête du fil (ambré quand il est cloisonné, avec un filet ambré autour du fil) ; cloisonner endort les onglets cachés du fil et rouvre l'onglet affiché dans le nouveau profil |
| Puzzle et épingles | dans un fil cloisonné, l'interrupteur dit « dans ce fil » ; une extension coupée partout peut tourner dans sa cloison ; la popup s'ouvre dans un panneau du **même profil** |
| Cohérence | installée depuis un fil cloisonné → ajoutée à ce fil ; retirée → quitte toutes les cloisons ; dossier déplacé → rechargé dans les cloisons qui l'utilisent |

Défaut trouvé par le scénario `t` et corrigé : recharger un onglet **avant la fin de sa première
navigation** laissait la page vide. Le rechargement est désormais différé à la fin du chargement,
et l'alignement d'une cloison n'est annoncé qu'une fois **tous** les ajouts terminés.

Preuve, scénario `t` : même origine locale, trois fils. Le fil cloisonné ne voit pas le cookie posé
par le profil principal (le fil témoin le voit) ; l'extension de preuve tourne dans le principal et
pas dans la cloison qui ne l'a pas ; coupée partout mais voulue dans la cloison, elle ne tourne
que là. Contrôlé aussi à la main dans la fenêtre.

### 10 quater. 19 h 30 — la mémoire libre et le coût des extensions

Clément : « fais toutes les optimisations que tu as indiquées ». Les deux qui restaient :
le nombre d'onglets ouverts suit la mémoire libre du PC (scénario `u`), et le coût réel de chaque
extension s'affiche dans le puzzle et les réglages (scénario `v`). Détail, mesures et limites :
`docs/OPTIMISATIONS.md` § 1.5 et 1.6. Au passage, la sonde ne lit la ligne de commande d'un
processus qu'une fois (`UpdateKind::OnlyIfNotSet`) : la reconnaissance des processus
d'extension ne coûte pas un balayage de plus.

### 10 quinquies. 19 h 50 — les menus par-dessus la page

Clément : « fais passer les menus d'extension par-dessus les pages ». La règle « deux webviews ne
se composent pas » était fausse dans sa forme : ce qui manquait, c'est `WS_CLIPSIBLINGS` sur les
fenêtres filles créées par wry. Avec ce style, l'ordre d'empilement et une région de fenêtre
(`src/fenetres.rs`), le menu puzzle, les résultats de la Couture et la fenêtre d'une extension se
posent sur la page, qui ne bouge plus. La fenêtre d'une extension prend la taille de son contenu,
et un clic dans la page la referme. Vérifié à la main dans la fenêtre (captures
`docs/captures/menu-par-dessus.png` et `popup-par-dessus.png`) ; tests de géométrie réécrits pour
le nouvel invariant (la page garde sa place, le panneau se pose dessus sans en déborder).

Au passage : la Couture montrait cinq fois la même adresse de l'historique ; une adresse n'y
apparaît plus qu'une fois (test `une_adresse_visitee_plusieurs_fois_n_apparait_qu_une_fois`).

### 10 sexies. 20 h 15 — les ombres

Clément : ajouter des ombres aux menus posés sur la page. Une fenêtre découpée par une région ne
peut rien dessiner hors d'elle : l'ombre vient d'une **fenêtre popup layered** à part
(`src/ombre.rs`), possédée par la fenêtre de Trame, transparente aux clics et sans activation, dont
le bitmap est un anneau d'ombre (alpha par pixel) **vide à l'intérieur** — posée au-dessus du menu,
elle ne le cache pas. L'ombre tombe de 4 px vers le bas, jamais sur la barre, suit les coins bas
arrondis du menu ; elle suit les déplacements de la fenêtre et disparaît quand elle est réduite.
Trois tests sur le calcul de l'opacité (un a trouvé que les coins **hauts** du menu, droits, auraient
été ombrés). Vérifié à l'écran sur une page claire, menu et fenêtre de Dark Reader ouverts par
sonde (`TRAME_MENU`, `TRAME_PANNEAU`), capture sans voler le focus.

Leçon de méthode, notée parce qu'elle a coûté : les essais à la souris prenaient le premier plan
pendant que Clément jouait sur le même PC ; un clic est tombé dans une autre application. Les
essais visuels passent désormais par les sondes, et la capture ne prend plus le focus.

### 10 septies. 21 h 50 — la page Amazon noire

Clément : une page Amazon noire, revenue seulement après rechargement. Le journal et l'historique
du profil réel disent ce qui s'est passé : l'onglet Amazon, en veille, a été recréé à la fermeture
d'un voisin (21:30:53), et **aucun chargement n'a abouti** (l'historique n'a rien avant le
rechargement manuel de 21:31:32). La cause n'est pas reproduite (l'écran était occupé par un jeu,
aucun essai visuel n'a été lancé). Deux mesures :

- **filet de sécurité** (`App::secours_chargement`) : un onglet rouvert et affiché dont le premier
  chargement n'aboutit pas est rechargé une fois — après 12 s s'il n'a pas commencé, 30 s s'il a
  commencé (une page lente n'est pas une page bloquée) ;
- **trace** : « premier chargement termine en N ms », et en cas de secours, s'il avait commencé ou
  non — la prochaine occurrence dira laquelle des deux pistes est la bonne.

Au passage : la barre d'adresse restait sur l'adresse de connexion après la connexion — le champ
restait « actif » dans le document de la barre après un clic dans la page. Corrigé (le champ suit la
page dès que la barre perd le focus), test d'interface à l'appui.

### 10 octies. 2026-11-22 — plus léger, plus discret, et le coup d'œil

Clément a retenu quatre propositions sur huit.

| Ajout | Détail |
|---|---|
| **Adresses nettoyées** | `urls::nettoyer` retire les paramètres de pistage (`utm_*`, `fbclid`, `gclid`, campagnes Amazon `hv*`/`adgrpid`…, traces AliExpress) ; liste prudente, testée sur l'adresse Amazon réelle du profil. La garde de navigation d'un onglet refuse l'adresse sale et relance la propre — le serveur ne reçoit jamais le pistage. Garde anti-boucle (3 s) si un site remet ses paramètres. Identifiants d'affiliation (`tag=`) gardés par défaut, au choix |
| **Anti-pistage strict** | `ICoreWebView2Profile3::PreferredTrackingPreventionLevel` = Strict sur le profil principal et chaque cloison ; réglable (stricte, équilibrée, désactivée) |
| **Pause des onglets cachés** | `ICoreWebView2_3::TrySuspend` après 5 min caché ; jamais un onglet qui joue du son (`IsDocumentPlayingAudio`), qui charge, ou une page interne ; `Resume` avant de l'afficher |
| **Coup d'œil** | Alt + clic sur un lien : la page s'ouvre dans un aperçu flottant (84 × 88 % de la page, coins arrondis, ombre), dans le profil du fil ; petite barre en Shadow DOM (« Ouvrir en onglet », ×, Échap, Ctrl+Entrée) ; un clic dans la page dessous ou un changement d'onglet le referme. L'aperçu est une page web quelconque : il n'a droit qu'à ses deux messages (`Source::Apercu`) |

Corrigé au passage, sur question de Clément (« masquer la publicité me fait voir les
extensions ? ») : le bouton du zappeur réaffichait la fenêtre de la dernière extension ouverte le
temps de recharger ses règles. Le panneau est désormais recréé neuf, dans le profil principal.

### 10 nonies. 2026-11-22, 2 h — « F5 ça marche, mais le premier lancement non »

Même schéma que la page Amazon noire : la page d'un onglet **tout juste créé** (au démarrage, ou
au réveil) reste inerte — YouTube sur ses gabarits gris — et un rechargement la répare toujours.
Écartés par mesure, en fenêtres d'essai cachées ou visibles et profils neufs : l'anti-pistage
strict, l'interception réseau, Nuit douce, `?themeRefresh=1`, et la visibilité de la page
(`document.visibilityState` vaut bien `visible`). Non reproduit hors du profil réel.

Changement d'ordre : une page web n'est plus donnée à l'onglet **pendant** sa création ; il naît
vide (`about:blank`), il est entièrement mis en place (style, empilement, interception, cloison),
puis la page est chargée — dans un onglet déjà établi, comme le fait F5. `about:blank` est ignoré
par le suivi de chargement. **Ce changement n'a pas suffi** (« toujours gris »).

**La vraie cause**, trouvée sur le profil réel avec le port de débogage local de WebView2
(`--remote-debugging-port`, lecture seule, refermé ensuite) : la page YouTube était servie par son
**service worker** (`sw.js`, comptes connectés) et restait en `document.readyState = "loading"` —
réponse commencée à 154 ms, jamais terminée, document coupé à 983 Ko au milieu d'un script.
Variantes relancées sur le profil réel, sans toucher à ses réglages : normal, sans interception,
sans anti-pistage, sans protocole interne, sans scripts injectés → **toujours bloqué** ; **sans le
moteur d'extensions** → chargé (0,7 s). Puis extensions actives mais **sans les réinstaller au
démarrage** → chargé (0,86 s), et Bitwarden toujours actif : WebView2 garde ses extensions.

Trame réinstallait toutes les extensions du dossier à **chaque** démarrage
(`with_extensions_path` de wry) ; cela rechargeait Bitwarden pendant la première page et coupait
le flux du service worker. Correction : plus de réinstallation ; au démarrage, Trame interroge le
moteur, n'ajoute **que** les extensions qui lui manquent (en silence), puis ouvre les onglets
(3 s au plus). Vérifié deux fois sur le profil réel (0,82 s et 0,64 s), 23/23 scénarios.

Sondes de diagnostic ajoutées : `TRAME_CACHEE` (fenêtre jamais affichée, aucun focus pris),
`TRAME_SONDE_JS` (script exécuté dans l'onglet actif à 15 s, résultat au journal),
`TRAME_SANS_INTERCEPTION`, `TRAME_SANS_CLIP`.

### 10 decies. 2026-11-22, matin — navigateur par défaut, menu repliable, outils

**Navigateur par défaut.** L'installateur inscrit Trame auprès de Windows (sous `HKCU`, sans droits
administrateur) : protocoles `http`/`https`, fichiers `.htm`/`.html`, entrée dans « Applications
par défaut ». Windows interdit à un programme de **se choisir** navigateur par défaut : c'est
l'utilisateur qui le fait, en un clic depuis les réglages (« Choisir dans Windows »). Un lien
ouvert ailleurs alors que Trame tourne est **transmis** au Trame ouvert (boîte `a-ouvrir` du
profil) et arrive dans un nouvel onglet ; Trame fermé, il s'ouvre au démarrage (scénario `x`).
La désinstallation retire toute l'inscription (contrôlé par `verifier-setup.mjs`).

**Le menu de gauche se replie à toute largeur** : bouton en haut du rail (ou `Ctrl+B`), choix
gardé dans la session.

**Les outils** — demandés comme « des fonctionnalités de vrai navigateur, et des choses qu'on ne
trouve pas ailleurs », sans perdre en fluidité :

- **Favoris** (`Ctrl+D`, étoile), **impression / PDF** (`Ctrl+P`, boîte de Chromium) ;
- **`Alt+Entrée`** : l'adresse tapée s'ouvre dans un nouvel onglet **du fil courant**, juste sous la
  page d'où l'on vient — demandé par Clément sur une page AliExpress : taper « amazon » devait
  « se caler dans ce fil » sans remplacer la page ; `Entrée` garde son comportement ;
- **Côte à côte** (`Maj+Entrée`, `Maj+clic`) : la séparation est une bande de la fenêtre
  elle-même (les pages sont des fenêtres enfants) ; la souris n'y arrive que là, ce qui donne un
  glisser sans calque ni hook ;
- **Rendez-vous** et **fermeture programmée** ; l'heure locale est calculée par l'interface, le
  cœur ne reçoit qu'une échéance en millisecondes (bornée à 60 jours) ;
- **Guetteur** : une page cachée, muette, sans téléchargement ni fenêtre, une à la fois, détruite
  après lecture ; attente de 3 s après le chargement (sites en JavaScript) et deux reprises ;
  espacement des lectures après des échecs ; jamais sous la réserve mémoire ;
- **Mémoire de lecture** : texte lu **par Trame** (jamais envoyé par la page), 8 s d'affichage,
  pas de page à mot de passe, sites exclus, recherche sans accents ni casse dans un fil à part
  abandonné dès la frappe suivante ; extrait tiré du texte d'origine.

Code : `src/carnet.rs` (données et recherche, 10 tests) et `src/app/outils.rs` (comportement).
Sonde ajoutée : `TRAME_IPC` (messages de la barre rejoués après l'ouverture des onglets) ; le
scénario `y` prouve sur l'application réelle, fenêtre cachée : retour d'un rendez-vous, fermeture
programmée, guet relu et changement détecté (prix affiché en JavaScript), page lue gardée puis
retrouvée par une phrase, `Alt+Entrée` dans le fil, `Maj+Entrée` à côté.

Le scénario `h` (deux instances) attend désormais « demande transmise » : la seconde instance ne
refuse plus sèchement, elle passe son lien — ou un simple retour au premier plan — à la première.
Le fond de la fenêtre est passé au sombre de Trame : il ne se voit qu'entre les deux pages du côté
à côté, où il faisait une bande blanche (vu à la capture).

**Pas prouvé sur l'application réelle** : le glisser de la séparation et l'impression (gestes à
la souris dans une boîte de dialogue), le choix d'un élément à guetter dans la page (même
sélecteur que le zappeur, éprouvé au scénario `k`).

## 10 bis. 2026-11-22, 10 h 30 → 11 h 25 — la passkey de Bitwarden, et le chien de garde

Au « oui » d'une passkey Google, la fenêtre de Bitwarden restait blanche, puis vide, et Google
répondait « Impossible de créer votre clé d'accès ». Trois causes en chaîne, lues par le port de
débogage sur le profil réel :

| Évolution | Pourquoi, et la preuve |
|---|---|
| **Fenêtre d'extension laissée à WebView2** (`fenetre_extension`, `NewWindowResponse::Allow`) | une page `chrome-extension://` marquée `uilocation=popout` (ou avec une taille) était rouverte en onglet : Bitwarden suit la fenêtre qu'il crée. WebView2 ne transmet pas la taille, d'où la marque |
| **Adaptateur WebView2** en tête du service worker (`extensions_chrome::adapter`) | WebView2 ouvre la fenêtre de `chrome.windows.create` mais répond `undefined` : Bitwarden plantait sur `.id` (« Cannot read properties of undefined »). L'adaptateur retrouve la fenêtre apparue. Mesuré : `{id: 1365163734, onglets: 1}` au lieu de `undefined` |
| **Rechargement unique** de l'extension adaptée (témoin `extensions-chrome-adaptees/<id>`) | Chromium garde le service worker en cache : fichier adapté, mais `chrome.windows.create.__trame` absent du worker vivant tant que l'extension n'est pas rechargée |
| **Chien de garde** (`src/chien_de_garde.rs`) | après la passkey enregistrée, la boucle s'est figée : vidage lu dans WinDbg, réveil d'un onglet bloqué dans `build_as_child`, en attente de `AddScriptToExecuteOnDocumentCreated` qui n'a jamais répondu. Non reproduit (4 essais : fenêtre ouverte, fermée, focus rendu). Si le battement n'est plus traité depuis 30 s, le journal note l'événement en cours, Trame se relance (session sauvée à chaque changement) et l'ancien processus s'arrête. Éprouvé par `TRAME_SONDE_BLOCAGE` sur un profil d'essai : figé à 11:23:11, relancé à 11:23:41, verrou repris |

**Pas prouvé** : une deuxième passkey de bout en bout avec le chien de garde en place ; la cause
réelle du blocage reste inconnue (la ligne « chien de garde : … pendant : » la donnera).

**Consoles en rafale aux réglages** (vu par Clément à 11:49) : `est_par_defaut` lançait `reg query`
sans `CREATE_NO_WINDOW`. Chaque console volait le focus, et le retour du focus (`refocaliser`)
relançait la vérification. Correction : `systeme::SANS_CONSOLE` sur `reg` et `powershell`, et
une lecture toutes les 5 s au plus. Essai automatique non concluant (les réglages ne s'ouvrent
pas par un message injecté) : à confirmer à l'œil (confirmé par Clément : plus de fenêtres).

## 10 ter. 2026-11-22, 12 h → 12 h 25 — le zappeur devient un atelier

Demande : « j'enlève les Shorts une fois, et la prochaine fois je ne les vois plus ».

| Évolution | Pourquoi, et la preuve |
|---|---|
| **Paquet « YouTube sans Shorts »** (`model::paquets_zap`, 12 règles) | relevé sur YouTube : la recherche montre une grille `grid-shelf-view-model` de 40 Shorts que les anciennes règles laissaient passer, plus l'entrée du menu et la puce de filtre. Installé une seule fois (`reglages.paquets_zap`) : une règle supprimée ne revient pas ; interrupteur d'ensemble dans le panneau |
| **Règles à condition de texte** (`RegleZap::texte`) | la puce « Shorts » n'a aucun attribut distinctif ; le CSS ne lit pas un texte, le script vivant s'en charge |
| **Script vivant** (`ui/zap-vivant.js`) | compte ce que chaque règle attrape, y compris dans ce qui se charge en défilant (MutationObserver, 700 ms) ; « vu le » par règle, alerte après 30 jours sans rien attraper |
| **Sélecteur intelligent** (`ui/zap-choisir.js`) | molette ou flèches = bloc parent/enfant, semblables en pointillés, menu « celui-ci / les N semblables / ceux qui mènent à /rubrique/ / seulement le texte « … » ». Garde-fou : un bloc qui couvre la page entière est refusé (l'essai avait proposé `ytd-app`, tout YouTube). Les classes de portée des composants (`ytd-…` sur tous les enfants) sont écartées |

Preuves (Trame d'essai caché, profil isolé, vrai YouTube, vrais événements souris par CDP) :
3 grilles de Shorts sur 3 masquées, puce « Shorts » masquée et les autres intactes ; du titre au
bloc vidéo en quelques crans, « Masquer les 19 semblables (ytd-video-renderer) », règle écrite
avec le libellé « La recette des crêpes ! et semblables ». 160 tests Rust, 53 tests d'interface.

### Suite, 12 h 30 → 12 h 50 — modifier, pas seulement masquer

Une règle porte désormais une **action** (`RegleZap::action`, `valeur`) : masquer, **élargir**,
**replier** (un clic déplie), **garder seulement ceci** (`body :not(:has(S)):not(S):not(S *)`,
refusé si le sélecteur contient déjà `:has`, qui ne s'imbrique pas), **réécrire le texte**
(script vivant, original gardé), **restyler** (texte plus grand ou plus petit, atténuer,
couleur). Les styles passent par `style_sur` : huit propriétés d'apparence seulement, ni
`url(`, ni accolade, ni chevron, ni guillemet. Le sélecteur demande d'abord *quoi* (celui-ci,
semblables…), puis *que faire*. Panneau : nom de l'action sur chaque règle, **Annuler la
dernière**, **Voir la page d'origine** (tant que le bouton est appuyé). Un identifiant n'est
utilisé que s'il est unique (YouTube répète `#video-title` sur chaque vidéo).

Preuves (Trame d'essai caché, vrai YouTube) : titre réécrit puis rendu à l'appui sur « page
d'origine » et réécrit au relâché ; « garder seulement ceci » : 1 vidéo visible sur 16, barre
du haut cachée ; « annuler » : règle retirée, page rechargée, 16 vidéos. 162 tests Rust, 53
tests d'interface.

### Suite, 12 h 55 → 13 h 15 — déplacer, copier, échanger

| Évolution | Pourquoi, et la preuve |
|---|---|
| **Remonter en premier / descendre en dernier** (`action` `haut`, `bas`) | par `order`, sans toucher au document (déplacer un nœud casse les sites construits en composants) ; proposé seulement si le bloc parent est en flex ou grille. Preuve : miniature d'un résultat YouTube passée de x = 96 à x = 595, à droite du texte. Les puces de filtre ne sont pas en flex : rien n'est proposé, à raison |
| **Copier vers…** (`copier_regles_zap`, `domaine_saisi`) | une adresse collée entière (`https://www.youtube-nocookie.com/embed/x`) est ramenée au site ; copies sans paquet ni « vu le ». Preuve : 19 règles copiées |
| **Exporter / importer** en texte (`regles_importees`) | pour garder ou partager ses règles ; chaque règle importée est contrôlée (action connue, sélecteur non vide, domaine valide, champs bornés, 300 au plus). Le test a trouvé qu'une règle sans `domaine` était rejetée : elle prend désormais le site affiché |

165 tests Rust, 53 tests d'interface.

### Suite, 15 h → 15 h 40 — icône, saute-pub, son par onglet

| Évolution | Pourquoi, et la preuve |
|---|---|
| **Icône « T tissé »** (`ressources/trame.svg`, `outils/faire-icones.mjs`) | choix de Clément parmi quatre pistes ; une seule source vectorielle, toutes les tailles et le `.ico` en sortent par Chromium ; planche de contrôle `ressources/icones/apercu.png` (16 → 128 px, fonds sombre et clair). Le glyphe du rail suit |
| **Extension « Saute-pub YouTube »** (`extensions/saute-pub`) | une pub vidéo passe dans le lecteur, hors de portée du zappeur. Tant que le lecteur porte `ad-showing` : son coupé, ×16, fin de la vidéo de pub, clic sur « Passer » ; ensuite le son et la vitesse d'avant reviennent. Preuve (pub simulée sur une vraie page) : muet + ×16 + 6 clics pendant, `muted:false, vitesse:1.25` après. Les CGU de YouTube interdisent les bloqueurs : signalé à Clément |
| **Extensions livrées proposées une fois** (`extensions-livrees.json`) | la copie n'avait lieu qu'au premier lancement : le saute-pub n'arrivait pas dans un profil existant. Une nouvelle arrive, une supprimée ne revient pas |
| **Haut-parleur par onglet** (`son`, `muet`, `moteur_ext::regler_muet`) | relevé chaque seconde (`IsDocumentPlayingAudio`) ; clic = couper/rendre le son de l'onglet, sans arrêter la vidéo ; gardé à travers la veille. Preuve : icône apparue sur la vidéo qui joue, coupée puis rendue |

| **Téléchargements** (`src/telechargements.rs`, `ui/telechargements.html`, bouton de la barre, Ctrl+J) | wry ne donnait que « commencé / fini ». Branché sur `ICoreWebView2DownloadOperation` : progression, pause, reprise, annulation ; dossier « Téléchargements » de Windows par défaut (ou choisi) ; liste gardée (`telechargements.json`) : ouvrir, afficher dans le dossier, recommencer, copier le lien, retirer. Preuve (fichiers de test OVH) : 10 Go en cours 423 Mo / 4,1 %, 134,9 Mo/s, pause puis reprise, annulation (« Annulé », fichier partiel supprimé), 10 Mo terminé (10 485 760 octets sur le disque). Corrigés en route : vitesse absurde sur deux états rapprochés, site « localhost », annulation affichée en échec |

**Installation** : à partir d'ici, toujours par l'Explorateur (`.cmd` hors conteneur) — voir la
mémoire « installeurs hors du conteneur de Claude » : les setups lancés depuis Claude écrivaient
le registre dans un conteneur privé, et Trame n'était jamais proposé comme navigateur.

**Pas fait, volontairement** : déplacer un élément n'importe où dans la page (il faudrait
déplacer des nœuds, ce qui casse les sites en composants comme YouTube).

## 10 quater. 2026-11-22, soir — favoris et accès rapides séparés, 10 places sous le logo

Demande de Clément : un **favori** est une page précise (une vidéo, un article, un chemin) ; un
**accès rapide** est un bouton qui mène à un site. Jusque-là, c'était une seule liste (`favoris.json`).

- `carnet::Acces` dans `acces.json`, à part de `carnet::Favori` (`favoris.json`). Au premier
  lancement sans `acces.json`, l'ancienne liste devient les accès rapides et reste les favoris.
- Ctrl+D / l'étoile ne touchent plus qu'aux favoris. Le Nouvel onglet a une section **Favoris**
  (liste, clic, Ctrl+clic, ×, clic droit « Créer un accès rapide vers ce site »).
- Un accès créé depuis un onglet (dépôt, « + » du rail) mène à la **racine du site**
  (`racine_du_site`) et prend son nom (`nom_du_site` : youtube.com → « Youtube »).
- Rail : la bande sous le logo est toujours là, **10 places** (`RAIL_MAX`, 5 × 2 déplié, une colonne
  replié) ; « + » ajoute le site de la page affichée ; glisser une icône sur une autre la range ;
  clic droit : ouvrir, modifier nom/adresse, retirer du rail, supprimer. L'hôte refuse une 11e.
- Preuves : `cargo test --release` (171), `node outils/tester-ui.mjs` (58, dont les deux nouveaux cas
  « 10 places » et « favoris à part »), captures du rail déplié, du menu et du rail replié.

### Suite, 18 h 25 — icônes gardées en image, favoris en nouvel onglet

- L'icône d'Amazon ne s'affichait pas dans le rail : son adresse, rechargée depuis l'interface
  de Trame, est un tiers (anti-pistage). Trame lit maintenant l'image que WebView2 a déjà pour
  l'onglet (`ICoreWebView2_15::GetFavicon`, `moteur_ext::lire_icone`) et la garde en `data:` dans
  `acces.json` / `favoris.json`, dès qu'un onglet du site charge son icône ou après un ajout.
- Un favori s'ouvre dans un **nouvel onglet**, ou va sur l'onglet où cette page exacte est déjà
  ouverte (`favoriOuvrir`). Un accès rapide garde son comportement : l'onglet déjà ouvert sur le site.
- Vérifié chez Clément à 18 h 25 : YouTube, Wikipédia et les 2 favoris ont leur image ; Amazon
  suivra à l'ouverture de son onglet (en veille).

## 10 quinquies. 2026-11-22, soir — la barre de la fenêtre dans l'interface (0.4.0)

Demande de Clément (et de son fils) : comme Opera GX, plus de barre de titre Windows au-dessus
de Trame. Trois maquettes proposées, la 1 choisie, puis « tous les cas de figure dans les
paramètres ». Nouveau réglage **Réglages › Fenêtre › Barre de la fenêtre**
(`reglages.barre_fenetre`) :

| Style | Ce qu'on voit |
|---|---|
| `bande` (défaut) | réduire, agrandir, fermer au bout de la barre d'adresse ; la croix touche le coin |
| `bandeau` | une bande de 28 px au-dessus de tout, titre de l'onglet au milieu, boutons à droite |
| `rail` | trois pastilles en haut du rail (au-dessus du « + » quand le rail est replié) |
| `windows` | la barre de titre de Windows, comme avant (secours) |

| Évolution | Pourquoi |
|---|---|
| `with_decorations(false)` + `with_undecorated_shadow(true)` (tao) | tao garde alors les bords invisibles de Windows à gauche, à droite et en bas : on redimensionne comme avant, avec l'ombre ; agrandie, la fenêtre ne couvre pas la barre des tâches |
| Le vide de la barre et du haut du rail → `drag_window` ; double-clic → agrandir | les surfaces de Trame couvrent tout le haut : c'est l'interface qui joue la barre de titre (`src/app/fenetre.rs`, message `fenetre`) |
| Bande de 4 px tout en haut → `drag_resize_window(North)` ; coins dans le rail et la barre | le bord du haut est sous les webviews, Windows ne le voit pas |
| Survol du bouton agrandir 0,7 s → Win+Z (seulement si Trame est au premier plan) | les dispositions d'ancrage de Windows 11 ne s'ouvrent seules que sur un vrai `HTMAXBUTTON`, qu'une page WebView2 ne peut pas présenter |
| Fermer = `WM_CLOSE` | même chemin que la croix de Windows : la session est sauvée |
| Changer de style ne redémarre rien | `set_decorations` à chaud, géométrie recalculée (le bandeau décale la barre et la page de 28 px) |

Preuve : `cargo test --release` 172/172 ; `tester-ui.mjs` 60/60, dont un cas neuf (boutons,
glisser, double-clic, bord du haut, agrandie, les quatre styles, rail replié). **Pas encore
éprouvé dans une vraie fenêtre** : le profil est unique et le Trame de Clément était ouvert.

### Suite, nuit — F5 reprend la vidéo, plus de flash blanc

Deux remarques de Clément : sur YouTube, F5 repartait du début ; et chaque onglet qui charge
faisait un éclair blanc d'une seconde.

| Évolution | Pourquoi |
|---|---|
| `ui/raccourcis.js` note la position de la vidéo principale (la plus grande, déjà lancée) dans `sessionStorage` à F5/Ctrl+R et à `pagehide`, et la remet au chargement suivant (même adresse, note de moins de 30 s, lecture relancée si elle tournait) | `sessionStorage` survit au rechargement d'un onglet ; `pagehide` couvre aussi le menu « Recharger » et le rechargement de secours |
| Sur `youtube.com/watch`, F5 réécrit d'abord l'adresse avec `t=…s` (`history.replaceState`) | le lecteur de YouTube s'y place lui-même, publicité comprise ; poser `currentTime` à la main risquait de viser la pub |
| Fond des onglets web et du coup d'œil : `COULEUR_FOND` (sombre) au lieu du blanc | un onglet naît sur `about:blank` avant de charger sa page : ce blanc était l'éclair |

Preuve : `cargo test --release` 172/172, `tester-ui.mjs` 60/60 ; essai dans Edge invisible avec
une vraie vidéo : à 8 s, F5 → reprise à 8 s ; menu « Recharger » → reprise aussi. Le cas
YouTube (`t=`) n'est pas éprouvé hors ligne.

## 10 sexies. 2026-11-22, soir — plus de « Réveiller », pied du rail replié (0.4.2)

« Réveiller » (rail, vue Trame « Réveiller tout », commande « Réveiller tous les onglets ») ne
servait à rien : les onglets réveillés d'un coup restent froids (vitalité < 0,16), et la veille
automatique les rendormait au battement suivant. Constaté au journal de Clément le 22/11 à
19 h 31 min 24 s : « 15 onglet(s) réveillé(s) » puis « 16 onglet(s) en veille » dans la même
seconde, après 15 pages chargées pour rien. Retiré partout ; un onglet se réveille toujours
d'un clic.

Rail replié (64 px) : le pied montrait rien. Il garde maintenant la jauge de mémoire (détail au
survol) et l'icône **Veille** ; Capsules reste au rail déplié (choix de Clément). Déplié, les
deux boutons portent leur icône.

Preuve : `cargo test --release` 172/172, `tester-ui.mjs` 61/61 (cas neuf : pied déplié et
replié, plus de bouton Réveiller).

## 10 septies. 2026-11-22, nuit — le rail replié en « dock »

Clément trouvait le rail replié « léger » : icônes de 16 px (12 px en réalité, la règle des
pastilles de fenêtre `.pastille` écrasait celle des onglets), chaque fil dans une carte bordée,
une tête de fil réduite à un chevron. Trois maquettes à taille réelle
(`docs/maquettes/rail-replie.html`) ; choix de Clément : la piste A, plus deux idées de la B.

| Évolution | Détail |
|---|---|
| Plus de cartes | un filet entre deux fils, la bande d'accès rapides sans cadre ni case « + » |
| Icônes de 22 px | dans des tuiles de 44 × 40 ; l'onglet affiché a un trait turquoise au bord du rail |
| Couleur par fil | tirée de l'identifiant (`couleurFil`), ambre pour un fil cloisonné ; pastille + nom en capitales ; un clic sur la tête replie ou déplie |
| Fil fermé en pile | ses icônes empilées sur une ligne, « +N » au-delà de 3 (`pileFil`) |
| Son et veille sur l'icône | haut-parleur en pastille sur le coin (clic = couper) ; onglet en veille grisé avec un point violet |
| Jauge en anneau | autour du bouton Veille (`--jauge`), qui porte le détail de mémoire dans sa bulle |

Seul le rail replié change (`@media (max-width: 130px)`), le rail déplié reste tel quel.

Preuve : `tester-ui.mjs` 62/62 (cas neuf : fils colorés, tuiles de 44 px, pile « +2 », clics),
capture du rail replié en navigateur invisible. Pas encore vu dans la vraie fenêtre.

## 10 septies. 2026-11-22, soir — saute-pub 2.0 : les pubs retirées avant le lecteur

Demande de Clément : « est-ce que tu peux pas intégrer quelque chose de plus puissant ». Le
saute-pub 1.0 ne faisait que réagir (pub chargée, puis son coupé, ×16, « Passer ») parce que le
moteur maison n'injectait qu'en fin de chargement.

| Ajout | Détail |
|---|---|
| **`js_debut`** (manifeste) | scripts ajoutés au script de début de document de chaque onglet (`extensions::script_de_debut`), gardés par les motifs de l'extension traduits en expressions rationnelles (`motif_en_regex`) |
| **`debut.js` du saute-pub** | retire `adPlacements`, `adSlots`, `playerAds`, `adBreakHeartbeatParams` (et le message « bloqueurs interdits ») de `ytInitialPlayerResponse`, `ytInitialData`, `JSON.parse` et des réponses `fetch` de `/youtubei/v1/player`, `/next`, `reel_watch_sequence`. Le lecteur ne programme aucune pub |
| **`bloquer`** (manifeste) | filtres `WebResourceRequested` étroits (pas `*` sur toutes les ressources) posés à la création de l'onglet ; l'état allumé est partagé (`moteur_ext::Blocage`) et suit l'interrupteur sans recréer l'onglet ; réponse 204. Le nettoyage des adresses ne s'applique plus qu'aux documents (`ResourceContext`) |
| **Encarts** | accueil, recherches, colonne de droite (`ytd-ad-slot-renderer`, `#masthead-ad`…) masqués |
| **SponsorBlock** | passages « sponsor », « autopromotion » et « rappel d'abonnement » sautés une fois chacun. Seuls les 4 premiers caractères du SHA-256 de l'identifiant de la vidéo partent vers `sponsor.ajay.app`, sans cookie ni référent |
| **Mise à jour des livrées** | une extension livrée de version plus récente remplace celle du profil, l'ancienne est gardée dans `extensions-anciennes\` (sinon la 2.0 n'arrivait jamais chez Clément) |

Preuves (Trame d'essai caché, profil jetable, vrai YouTube, `IVSe1InCSI4`, dont la page brute
contient `adPlacements`) : réponse du lecteur `OK` **sans** `adPlacements` ni `playerAds`,
11 réponses élaguées, aucune pub affichée ; `youtube.com/ptracking` refusé au réseau ; lecture
à 26,7 s → **59,7 s** (sponsor 27 → 60 s), bulle « Sponsor sauté ». `cargo test --release`
176/176 (4 nouveaux).

Limites : une pub **cousue dans le flux** (essais de YouTube depuis 2025) n'est pas décrite
dans la réponse : le filet de la 1.0 (son coupé, ×16, « Passer ») reste en place. YouTube peut
afficher son avertissement anti-bloqueur ; ses CGU interdisent les bloqueurs.

## 10 octies. 2026-11-22, nuit — la vidéo flottante (étape 1)

Demande de Clément, sur le modèle de l'icône d'Opera GX : sortir une vidéo de la page dans
une petite fenêtre qu'on place où on veut, sur n'importe quel écran. Choix : « la 1, puis la 3 ».
Étape 1 = la fenêtre flottante de Chromium (`requestPictureInPicture`) ; étape 3 (à venir) =
déplacer l'onglet entier dans une mini-fenêtre de Trame, pour garder toutes les commandes du site.

| Ajout | Détail |
|---|---|
| **`ui/incrustation.js`** | injecté dans chaque onglet avec les raccourcis (`INCRUSTATION` dans `app.rs`). Au survol d'une vidéo d'au moins 200 × 120, une pastille « Sortir la vidéo » apparaît en haut de l'image (racine fantôme fermée, construite sans HTML brut à cause des Trusted Types de YouTube) ; elle devient « Ramener » quand la vidéo flotte et disparaît après 2,5 s sans mouvement ou en plein écran |
| **Alt+P** | sort ou ramène la vidéo survolée, sinon la vidéo principale (celle qui joue, sinon la plus grande) |
| **Clic protégé** | appui, clic et double clic de la pastille n'atteignent pas le lecteur (YouTube mettait en pause) ; l'attribut `disablepictureinpicture` d'un site est retiré |

Preuves : `tester-ui.mjs` 64/64 (2 nouveaux cas : pastille au survol, pas sur une petite vidéo ;
clic sans pause ni clic transmis au lecteur) ; vrai WebView2 (Trame caché, profil jetable,
vidéo de canevas) : `pictureInPictureEnabled` vrai, clic → vidéo flottante, lecture continue,
Alt+P la ramène puis la ressort. `cargo test --release` 176/176. Pas encore installé chez Clément.

Le script est injecté aussi dans les cadres (`with_initialization_script_for_main_only(…, false)`,
car `with_initialization_script` de wry ne vise que la page principale) : un lecteur YouTube
intégré à un autre site a sa pastille. Vérifié dans le vrai WebView2 avec un cadre d'une autre
origine.

Limite : la fenêtre de Chromium n'a que lecture/pause et retour, d'où l'étape 3.

### Étape 3 : l'onglet entier en mini-fenêtre (même soir)

| Ajout | Détail |
|---|---|
| **Pastille en deux parties** | « Sortir la vidéo » (incrustation de Chromium, Alt+P) et une icône de fenêtre (Alt+Maj+P, page principale seulement) qui envoie `{m:"flottant", action:"ouvrir"}` |
| **`src/app/flottant.rs`** | fenêtre tao sans barre, toujours au premier plan, hors barre des tâches, 480 × 270 en bas à droite de l'écran de Trame (puis la dernière place et taille). La webview de l'onglet y est **reparentée** (`WebViewExtWindows::reparent`) et **retirée de `webviews`** pendant le vol : placement, visibilité, veille et empilement de la fenêtre principale l'ignorent sans exception à écrire partout ; `reveiller` refuse de lui créer une seconde page |
| **Création de la fenêtre** | seule la boucle a la cible : `traiter_ipc` note `flottant_demande`, `main.rs` appelle `ouvrir_flottant` juste après ; les événements de la mini-fenêtre sont triés par `window_id` (taille → la page suit ; Alt+F4 → la page revient sans fermer Trame) |
| **Dans la page** | `__trame_flottant(true)` étire le lecteur (`#movie_player` sur YouTube, sinon le plus haut ancêtre de la vidéo qui ne la déborde pas de plus de 48 px) en `position: fixed` plein cadre ; barre au survol : la tirer déplace la fenêtre (`drag_window`), « Ramener » remet la page dans son onglet et l'affiche |
| **Garde-fous** | l'onglet actif qui part est remplacé par son voisin le plus proche du même fil (ou un nouvel onglet) ; l'activer depuis le rail le ramène ; le fermer ferme sa mini-fenêtre ; depuis la mini-fenêtre, seul Ctrl+W reste un raccourci (les autres viseraient l'onglet actif) ; côte à côte défait |

Preuves (vrai WebView2, Trame caché, profil jetable) : page d'essai 1148 × 810 → **480 × 270**
en vol, lecture continue, barre sous le pointeur, retour à 1148 × 810 ; vrai YouTube : lecteur et
vidéo à `[0, 0, 480, 270]`, barre de commandes à l'intérieur, retour exactement à la mise en page
d'avant (`[16, 68, 765, 430]`) une fois l'onglet réaffiché. `tester-ui.mjs` 67/67,
`cargo test --release` 176/176. Pas encore installé chez Clément ; déplacement à la souris et
redimensionnement par les bords pas encore essayés à la main.

## 10 nonies. 2026-11-22, nuit — ranger les groupes du rail (0.4.7)

Demande de Clément, capture du rail à l'appui : « je voudrais pouvoir réorganiser les groupes
d'onglets ». Jusque-là l'ordre était celui de la création des fils et rien ne le changeait
(l'heure du fil, `actif_le`, ne servait qu'à la fraîcheur du tissage).

| Ajout | Pourquoi, et où |
|---|---|
| **`Session::deplacer_fil(id, avant)`** (`src/model.rs`) | l'ordre des fils **est** `session.fils` : il tient donc au redémarrage, et c'est la même liste qui ordonne les cartes du rail et les bandes de la trame — une seule vérité, deux vues qui ne peuvent pas diverger. Rend `false` quand rien ne bouge (dépôt devant soi-même ou juste après soi) : ni session salie, ni redessin inutile. Un `avant` disparu (fil fermé entre-temps) vaut « en dernier » plutôt que de perdre le groupe |
| **Message `filDeplacer`** (`{ id, avant }`, `avant: 0` = en dernier, même convention que `fil: 0` de « tisser ») | le rail n'écrit pas la session : il demande, l'hôte décide, marque et sauve |
| **Le geste** (`ui/chrome.js`) | on glisse la **tête** du groupe (chevron, nom, compteur) au pointeur. Un trait turquoise dit où il tombera — moitié haute d'une carte = devant elle, moitié basse = après. Un seuil de 6 px distingue l'appui du glissement, et le clic qui suit un glissement est avalé : dans le rail replié, ranger ne replie pas le groupe |
| **Pourquoi le pointeur et pas le glisser-déposer HTML** | sur ces mêmes cartes, le dépôt d'un onglet (`tisser`) lit déjà `text/plain` ; distinguer un groupe d'un onglet au dépôt aurait empilé deux gestes sur une cible. Et le glisser HTML n'existe pas au doigt. Le pointeur capturé couvre souris, stylet et doigt, et un relâchement hors de la fenêtre n'y laisse pas de geste ouvert (`pointercancel` et `blur` remettent la carte en place) |
| **Gel du redessin pendant le geste** | le rail est reconstruit à chaque poussée qui change sa signature (une toutes les 400 ms pendant un défilement) : sans gel, la carte saisie serait détruite au milieu du mouvement. C'est le même verrou que la bande d'accès rapides (`glisseRail`) |
| **Le trait, dessiné dans la carte** | premier essai : un trait de 2 px posé dans l'interstice, à `top: -6px`. La classe était bien posée, le trait invisible — `#fils` défile (`overflow: auto`) et coupe tout ce qui dépasse au-dessus de la **première** carte, c'est-à-dire le rang le plus demandé. Trouvé à la capture, pas par le test (qui ne vérifiait que la classe) : le trait est passé à `top: 0` / `bottom: 0`, dans la carte, et le test vérifie désormais cette position |
| **`outils/eprouver-rangement.mjs`** | banc d'essai du **vrai geste** : un glissement de souris envoyé par CDP dans le rail du binaire, profil jetable, fenêtre cachée, puis relecture de l'ordre des cartes **et** de `session.json` |

Preuves : `cargo test --release` **177/177** (le cas neuf vérifie les quatre rangs, les deux
dépôts sans effet, un fil inconnu et la relecture depuis le disque) ; `node outils/tester-ui.mjs`
**73/73** (5 cas neufs : le trait et le message unique au relâchement, le dépôt sous le dernier
`avant: 0`, l'appui qui ne range rien, le rail replié qui ne replie pas, et la carte saisie qui
survit à une poussée de l'hôte) ; binaire de **release** : rail `[1,2,3]` → un vrai glissement →
`[3,1,2]`, groupe non replié, `session.json` `[3,1,2]`.

Livrables (2026-11-22, même soir) : `node outils/faire-setup.mjs` a compilé, **signé
l'exécutable** (Azure Trusted Signing), fabriqué l'installation Inno Setup puis **signé le setup**.
Les deux sont `Valid`, horodatés, signataire `CN=CAHAGNE CLEMENT` ; `Trame-0.4.7-setup.exe`
(2,8 Mo) et `Trame-0.4.7-portable.zip` (1,2 Mo) sont dans `dist\`, avec `SHA256SUMS.txt`. Version
étiquetée `v0.4.7`. `verifier-setup.mjs` n'a pas été rejoué (l'installation 0.4.6 tournait) : il
installe et désinstalle une copie, donc il touche HKCU — à lancer autour de l'installation.

Pas encore essayé à la main dans la fenêtre de Clément.

Limite dite franchement : au **doigt**, le geste fait défiler le rail (le navigateur garde le
panoramique) ; il faudrait un appui long pour l'empêcher, et ce n'est pas fait.

## 10 decies. 2026-11-22, nuit — la reprise de la lecture après F5 (0.4.8)

Demande de Clément : « quand je lis une vidéo YouTube et que je rafraîchis le navigateur, la vidéo
ne reprend pas vraiment là où j'en étais. Moi j'ai une avance d'une quarantaine de secondes, et sur
un autre PC il revient toujours à 27 secondes de lecture. »

Le mécanisme livré en 0.4.1 (réécrire `t=…s` dans l'adresse et laisser YouTube se placer) n'a
**jamais fonctionné sur YouTube**. Mesuré, pas supposé — banc d'essai neuf
`outils/eprouver-reprise.mjs` : Trame d'essai, profil jetable, fenêtre cachée, port CDP, **vraie
page YouTube**, position posée à 95 s, puis un **vrai F5** (`Input.dispatchKeyEvent`) reçu par
`ui/raccourcis.js` comme sous un doigt humain.

| Cas | Avant F5 | Après F5, 0.4.6 | Après F5, 0.4.8 |
|---|---|---|---|
| YouTube, vidéo **en pause** à 95 s | 95 s | **0 s**, en pause | **95 s**, en pause |
| YouTube, vidéo **en lecture** à 95 s | 95 s | **0 s**, en pause | **95 s**, en lecture |
| page ordinaire, pause à 5 s | 5 s | 5 s puis elle joue | **5 s**, en pause |

| Défaut trouvé | Mesure |
|---|---|
| **YouTube ne lit `t=` qu'au démarrage de la lecture** | après le F5 la vidéo reste à 0 s ; elle ne saute à 95 s qu'une fois la lecture relancée à la main |
| **La note de Trame était jetée dès que `t=` était là** (`if (youtube() && has("t")) return;`) | dans ce cas précis **personne** ne reposait la position : ni YouTube (pas encore), ni Trame (par construction) — d'où le 0 s |
| **`t=` restait dans l'adresse**, jamais nettoyé | l'adresse après F5 : `…/watch?v=…&t=95s` ; un lien copié ou un onglet rouvert repartait de là |
| **La note n'était écrite qu'à `pagehide`** | un onglet tué, une veille, ou une **publicité** en cours de lecture : rien à reprendre (une pub n'a pas la durée de la vidéo) |
| **Rien ne vérifiait le résultat** | la valeur choisie par le lecteur était définitive |

| Évolution | Pourquoi |
|---|---|
| Le `t=` **n'est plus écrit** | une seule source de vérité : la note. Un `t=` **déjà présent** au chargement (lien partagé) reste respecté — c'est une intention, pas une note |
| **La note suit la lecture** : rechargement demandé, `pagehide`, `beforeunload`, page masquée, et **toutes les 2 s** tant qu'elle est visible | un onglet tué n'a pas de `pagehide` |
| **Jamais pendant une publicité** (`.ad-showing` / `.ad-interrupting`) | une pub n'est pas la vidéo |
| **Même page, même vidéo** : adresse (moins `t`) ou identifiant YouTube `v`, **et** durée à ±1,5 s ; note valable **2 minutes** | une autre vidéo du même site ne reçoit pas la position |
| **Pose puis vérification** : position reposée, puis contrôlée pendant 25 s (prolongée tant que dure une pub, 3 min au plus). La position attendue avance avec la lecture (et son `playbackRate`), ce qui distingue « la vidéo avance » de « le lecteur l'a déplacée » | un lecteur qui replace sa vidéo est corrigé ; mesuré : sans cela, YouTube laisse la vidéo à 0 |
| **Un geste de Clément annule la reprise** (`pointerdown`, `keydown` capturés pendant la fenêtre) | son geste prime toujours sur la reprise |

Preuves : `outils/eprouver-reprise.mjs` — YouTube en pause **95 s → 95 s**, YouTube en lecture
**95 s → 95 s**, page ordinaire **5 s → 5 s**, écart 0 dans les trois cas, adresse sans `t=` et
note consommée ; `cargo test --release` **177/177** ; `node outils/tester-ui.mjs` **73/73**.

Ce qui n'est **pas** vérifié : le symptôme des deux PC de Clément (40 s d'un côté, 27 s de
l'autre) n'est pas reproduit — Trame ne calcule ni l'un ni l'autre, et il rendait 0 s. Ce qu'il
voyait était donc la reprise **de YouTube**, qui dépend du compte et de la machine ; Trame bloque
en plus `ptracking` (relevé dans le journal de Clément), le point où YouTube enregistre la
progression, ce qui fige la position mémorisée. Le correctif supprime la dépendance, il ne
démontre pas les deux nombres. Le cas d'un YouTube **connecté** n'a pas été mesuré (profils
d'essai déconnectés).

## 10 undecies. 2026-11-22, nuit — réduire ne tue plus Trame (0.4.9)

Signalé par Clément juste après l'installation de la 0.4.8 : « si je réduis l'application dans la
barre des tâches elle se ferme ».

**Reproduit et expliqué, pas deviné.** Le processus sortait en **code 101** — une panique Rust :

```
thread 'main' panicked at library\core\src\num\f64.rs:1432:9:
min > max, or either was NaN. min = 46.0, max = 13.68
```

| Ce qui s'est passé | Pourquoi |
|---|---|
| Réduite, la fenêtre de Trame ne fait plus **19 px** de haut | Windows ne lui laisse qu'une taille symbolique |
| `shell::geometrie` borne la barre haute entre `BARRE_H` (46 px) et **72 % de la hauteur** (13,68 px) | la borne haute tombait **sous** la borne basse |
| `f64::clamp` **panique** quand `min > max` | d'où le code 101, et la fenêtre qui disparaît |
| **Rien dans le journal** | une panique n'écrit que sur stderr, et la version release n'a pas de console : il a fallu relancer le binaire en capturant stderr pour lire le message. C'est exactement le piège « l'absence de trace n'est pas une preuve d'absence » |

| Correctif | Où |
|---|---|
| Le minimum de la barre l'emporte sur le plafond : `(h * 0.72).max(BARRE_H * echelle)` | `src/shell.rs` — toute fenêtre minuscule est désormais sans danger, quel qu'en soit l'appelant |
| Deux `clamp` du menu de la barre avaient le même défaut (`bl - 1`, `bl - x`) : bornes hautes ramenées à au moins la borne basse | `src/app.rs` |
| Fenêtre réduite : **on ne dispose plus rien** (`redimensionner` sort si `is_minimized()`) ; la restauration renvoie un `Resized` avec la vraie taille | `src/app.rs` — plus de travail inutile sur une géométrie qui ne veut rien dire |
| **Crochet de panique branché sur le journal** (`!! PANIQUE : …`) | `src/log.rs` — un plantage laisse désormais une trace, et ne se confond plus avec une fermeture propre |
| Sonde `TRAME_SONDE_PANIQUE` (panique volontaire au démarrage) | prouve que le crochet écrit bien dans le journal |
| **`outils/eprouver-reduire.mjs`** | banc d'essai du vrai geste : `WM_SYSCOMMAND`/`SC_MINIMIZE` envoyé à la fenêtre du binaire par `outils/piloter-fenetre.ps1` (qui gagne `-Processus`, `-Reduire`, `-Restaurer`), puis contrôle que le processus vit, que la fenêtre est bien réduite, qu'elle se restaure, et que le journal ne porte aucune panique |

Preuves : **contre le binaire 0.4.8 installé, le banc échoue** (processus mort après le réduire,
journal muet) ; **contre le binaire corrigé, il passe** (`minimisee=True`, processus vivant,
restauration à `minimisee=False`, aucune panique, et la sonde écrit
`!! PANIQUE : … sonde : panique volontaire`). `cargo test --release` **178/178** (un cas neuf :
la géométrie de fenêtres de 19, 1 et 0 px, à quatre échelles) ; `node outils/tester-ui.mjs`
**73/73**.

Limite dite franchement : le bouton « réduire » **de la barre de Trame** n'a pas été cliqué dans
le banc (il passe par `set_minimized(true)`, donc par le même `WM_SIZE`/`SIZE_MINIMIZED` que le
geste éprouvé) ; et le correctif de `shell.rs` ne dépend d'aucun chemin d'appel.

## 10 duodecies. 2026-11-22, nuit — le retour de la fenêtre d'incrustation ramène l'onglet (0.4.10)

Signalé par Clément après la 0.4.9 : « quand je clique sur sortir la vidéo, je sors bien la vidéo
du navigateur, je peux changer d'onglet et naviguer ; mais si je clique sur retour à l'onglet, il
renvoie bien la vidéo en cours de lecture, mais il ne m'affiche pas l'onglet dans lequel il y a la
vidéo. »

Exact, et **mesuré** — banc d'essai neuf `outils/eprouver-flottant.mjs` (page locale avec une
vidéo, incrustation demandée par un vrai clic, second onglet activé, puis sortie de
l'incrustation, en lisant `document.visibilityState` de l'onglet de la vidéo) :

| Étape | 0.4.9 | 0.4.10 |
|---|---|---|
| vidéo sortie en fenêtre d'incrustation | `pip: true` | `pip: true` |
| second onglet activé | onglet de la vidéo `hidden` (la fenêtre d'incrustation survit) | idem |
| retour à l'onglet | `pip: false`, onglet **`hidden`** — la vidéo revient dans une page invisible | `pip: false`, onglet **`visible`** |

| Défaut trouvé | Pourquoi |
|---|---|
| Dans Chrome, **c'est le navigateur** qui montre l'onglet quand la fenêtre d'incrustation est fermée par « retour à l'onglet » | WebView2 n'a pas d'onglets : Chromium rend la vidéo à sa page et s'arrête là |
| `ui/incrustation.js` écoutait déjà `leavepictureinpicture`… | mais seulement pour relancer une vidéo que la croix met en pause (correctif de la 0.4.6) : **rien** ne demandait à Trame d'afficher l'onglet |

| Correctif | Où |
|---|---|
| La page demande l'affichage : `{ m: "flottant", action: "montrer" }` sur `leavepictureinpicture`, **si la page est masquée** et **si la vidéo ne s'est pas terminée** (une vidéo finie ferme aussi la fenêtre d'incrustation : là, on ne vole pas l'onglet) | `ui/incrustation.js` |
| `"montrer"` traité par l'hôte : l'onglet passe au premier plan ; le bouton « Ramener » de la mini-fenêtre et le retour d'incrustation passent par la **même** fonction (`montrer_onglet`) | `src/app/flottant.rs` |
| `outils/cdp.mjs` : le client CDP des bancs d'essai, extrait pour être partagé (reprise, réduire, flottant) au lieu d'être recopié | `outils/` |
| **`outils/eprouver-flottant.mjs`** | le banc d'essai ci-dessus, rejouable |

Preuves : contre le binaire 0.4.9, le banc rend `hidden` et annonce l'échec ; contre le binaire
corrigé, il rend `visible` et annonce « le retour de l'incrustation ramène bien l'onglet ».
`cargo test --release` **178/178**, `node outils/tester-ui.mjs` **73/73**, et les deux autres
bancs (`eprouver-reprise.mjs`, `eprouver-reduire.mjs`) rejoués après l'extraction du client CDP.

Conséquence assumée : la **croix** de la fenêtre d'incrustation ramène aussi l'onglet. Chromium
ne dit pas laquelle des deux commandes (« retour à l'onglet » ou « fermer ») a mis fin à
l'incrustation — les deux produisent le même événement. Si Clément préfère que la croix le laisse
où il est, il faudra une autre source (Chromium ne la donne pas).

Ce qui n'est pas éprouvé : la vidéo du banc ne joue pas (lecture bloquée dans le profil jetable) ;
l'incrustation s'ouvre quand même, et le mécanisme mesuré — le retour — ne dépend pas de la
lecture. Le geste réel de Clément (bouton « retour à l'onglet » de la fenêtre de Chromium) n'est
pas cliquable depuis CDP : c'est `exitPictureInPicture()`, qui produit le même événement, qui est
employé.

## 10 terdecies. 2026-11-22, 22 h — la sauvegarde du profil, et ce qu'elle refuse d'emporter

Demande de Clément, mot pour mot : « je voudrais pouvoir sauvegarder la configuration, et
pourquoi pas exporter les favoris — ou sans garder les favoris ».

**La mesure qui a commandé la conception**, prise sur le profil réel avant d'écrire une ligne :
**1 217 Mo, 10 204 fichiers**. Et dedans, ce qui fait que Trame est *ce* navigateur pèse
**19 Ko** (session 6,2 · zap 4,7 · favoris 3,3 · capsules 2,4 · accès 1,3 · ext-storage 0,7 ·
réglages 0,6). Le reste est du WebView2 (854,6 Mo de cookies et de connexions), des
téléchargements (283,8), des extensions Chrome (77,6) et la mémoire de lecture (489,9 Ko).

**Un rapport de 1 à 64 000, et une conclusion de conception** : une sauvegarde utile emporte la
petite colonne et **laisse la grosse dehors en le disant**. Le fichier porte donc la liste de ce
qu'il n'a pas pris, avec la raison (`exclus`) — sinon il fait croire qu'il protège ce qu'il n'a
pas pris.

| Ce qui a été ajouté | Où |
|---|---|
| Module de sauvegarde : contenus, archive `.trame` (JSON lisible, en-tête versionné, empreinte SHA-256), lecture, restauration, résumé de ce qui change | `src/sauvegarde.rs` |
| Restauration **appliquée au démarrage suivant**, après la prise du verrou et avant la lecture de la session ; ancien profil copié dans `avant-restauration-<date>\` | `src/main.rs`, `src/sauvegarde.rs` |
| Six commandes : sauvegarder, choisir une archive, confirmer, annuler, exporter les favoris, les importer | `src/app.rs` |
| Dialogue de fichier en fil annexe (il bloque la boucle : même leçon que l'installation d'extension) ; `UserEvent::Sauvegarde` | `src/app.rs`, `src/systeme.rs` |
| Section « Sauvegarde du profil » et « Favoris — échange avec les autres navigateurs » ; le catalogue des contenus vient de l'hôte, pas du HTML | `ui/reglages.html`, `etat.sauvegarde` |
| Export **et** import au format Netscape (Chrome, Edge, Firefox) : entités décodées, doublons écartés, `javascript:` refusé | `src/sauvegarde.rs` |
| Nom de fichier des accès rapides ramené à une seule constante, pour que la sauvegarde et l'application ne divergent pas | `src/carnet.rs`, `src/app/outils.rs` |

**Deux pièges, tous les deux trouvés par les bancs d'essai, pas par la lecture :**

1. **`serde_json` trie les clés de tous les objets**, imbriqués compris (sa `Map` est une
   `BTreeMap`). Le banc qui écrivait une archive à la main calculait l'empreinte en ne triant que
   le premier niveau : Trame a répondu « l'empreinte ne correspond pas ». Le calcul côté banc
   trie désormais **à tous les niveaux**. Conséquence pour le format : il est canonique, et un
   outil qui ne trie pas obtient une autre empreinte — ce n'est qu'un avertissement au journal,
   jamais un refus, parce que le fichier reste du JSON qu'on peut corriger à la main.
2. **Sauvegarder sans avoir écrit la session** aurait ignoré les onglets ouverts dans les
   dernières secondes : `session.json` n'est réécrit qu'à l'arrêt, ou toutes les 8 s par le
   battement. La commande appelle donc `App::sauver` avant de lire le disque.

**Trois défauts des bancs d'essai, révélés en les rejouant** (aucun n'était causé par ce
chantier, tous étaient rouges avant lui) :

| Scénario | Ce qui échouait | Pourquoi | Correctif |
|---|---|---|---|
| `j`, `p` | le dossier `telechargements` du profil n'existe pas | Trame télécharge dans le dossier « Téléchargements » de Windows quand aucun dossier n'est choisi — comportement changé, bancs pas mis à jour. Ils **écrivaient dans les vrais téléchargements de Clément** (`preuve-trame.pdf`, `essai.crx`). | les deux scénarios fixent le dossier **dans le profil jetable** |
| `o` | « 1 active(s) » puis « 0 active(s) » | `saute-pub`, livrée allumée le 2026-11-22, fait compter une active de plus | le compte est désormais **relatif** (une active de moins après la coupure), et le fait précis — `essai` activée, puis réellement injectée — reste vérifié |

Preuves : `cargo test --release` **186/186** (huit cas neufs : aller-retour complet, archive qui
tente d'écrire `webview2\` ou de sortir du dossier des capsules, version inconnue refusée,
résumé qui compte les ajouts et les pertes, aller-retour HTML avec entités et sans titre,
import tolérant et sans doublon, sauvegarde vide refusée, noms de fichiers comparés à ceux du
profil) ; `node outils/tester-ui.mjs` **76/76** (le catalogue vient de l'hôte, une case décochée
survit à une poussée d'état, le résumé s'affiche sans rien appliquer, les six commandes
partent) ; `node outils/verifier-app.mjs` **`z`** (une archive écrite par le banc est appliquée
au démarrage : 3 onglets, 2 favoris, le moteur de l'archive ; l'ancien profil est gardé tel quel
et le compte rendu s'affiche) et **`z2`** (une archive en version 99 est refusée, rien n'est
touché, la demande est gardée sous `.echec.json`).

Ce qui n'est pas fait, dit franchement : pas de sauvegarde **automatique** (à date fixe, dans un
dossier choisi) ; l'import HTML **aplatit** les dossiers de favoris de Chrome (un favori n'a pas
de groupe dans le modèle — seuls les accès rapides en ont).

### Suite, 22 h 40 → 23 h 10 — la 0.4.11 signée, et trois pièges de fabrication

Clément a demandé que la version livrée contienne le module (« 1 » dans la liste des décisions).
La fabrication a révélé trois défauts qui n'ont rien à voir avec la sauvegarde, mais qui auraient
livré un mauvais binaire — tous les trois corrigés et consignés ici parce qu'ils reviendront.

| Défaut | Mesure | Correctif |
|---|---|---|
| **Inno Setup relisait `target\release\trame.exe`** après la signature, au lieu de la copie signée de `dist\` | un `cargo build` concurrent a remplacé l'exe **entre** la signature et la compilation du setup : l'installation a embarqué un exe **non signé** sous un setup signé (constaté : 3 123 712 o / `NotSigned` installés contre 3 138 232 o / `Valid` dans `dist\`) | `trame.iss` empaquette `..\dist\trame.exe` — la copie signée, celle qui ne peut plus changer. Prouvé par une installation jetable : le exe installé est **octet pour octet** `dist\trame.exe`, `Valid` |
| **Inno reprend le dossier de l'installation précédente** (`UsePreviousAppDir`) | après un contrôle dans `%TEMP%`, les installations « réelles » allaient dans `%TEMP%\trame-controle3\…` sans toucher `%LOCALAPPDATA%\Programs\Trame` — l'exe du poste restait l'ancien, et rien ne le disait | l'installation impose `"/DIR=%LOCALAPPDATA%\Programs\Trame"`, et la recette est écrite dans le README |
| **`verifier-setup.mjs` partage l'identifiant d'application de la vraie installation** | rejoué alors qu'une 0.4.10 était installée, son désinstalleur a retiré le raccourci, l'entrée « Applications », l'inscription navigateur et le dossier mémorisé de la vraie installation : le poste est resté avec un exe orphelin | le contrôle **refuse** de tourner si une installation réelle existe (code 3, motif affiché), `--force` pour passer outre. Réparé sur le poste en réinstallant |

> Ce qui est **vérifié** sur le poste de Clément après installation : `trame.exe` 0.4.11
> `Valid` (horodaté, `CN=CAHAGNE CLEMENT`) et de même empreinte que `dist\trame.exe` ; raccourci
> du menu Démarrer, entrée « Applications » (`RegisteredApplications\Trame`), inscription
> navigateur, désinstalleur 0.4.11 pointant sur `%LOCALAPPDATA%\Programs\Trame\` ; interface
> livrée contenant la section « Sauvegarde du profil » ; démarrage réel sur un profil jetable
> (une ligne « Trame demarre », aucune panique) ; profil réel `%LOCALAPPDATA%\Trame` intact.

Une autre session travaillait dans ce dossier **pendant** la fabrication (`src/app.rs` 22 h 52,
`src/telechargements.rs` 22 h 52, un test de plus : 187 au lieu de 186). Le livrable est donc
construit sur l'arbre **testé à cet instant** — 187/187, 76/76 — et non sur l'état de 22 h.

## 10 quaterdecies. 2026-11-23, 0 h 45 — « Afficher dans le dossier » n'ouvre plus Documents (0.4.12)

Signalement de Clément, sur le PC de son fils (installation neuve) : « quand il télécharge des
documents ou des fichiers, ça les met bien dans Téléchargements, mais quand je fais ouvrir le
dossier, ça m'ouvre le dossier Documents. »

**La cause, mesurée sur ce poste** : `explorer` était appelé avec **un seul** argument,
`/select,C:\…\mon document.pdf`. Dès que le chemin contient une espace, Windows cite l'argument
entier ; `explorer` ne reconnaît plus le préfixe `/select,`, n'échoue pas — il ouvre son dossier
par défaut. Les trois formes ont été essayées :

| Forme envoyée | Résultat |
|---|---|
| `/select,C:\…\essai-trame select\mon document.pdf` en un argument (l'ancien code) | ouvre **Documents**, ou réactive la fenêtre Documents déjà ouverte |
| `/select,"C:\…\file.pdf"` en un argument | rien |
| `/select,` **puis** le chemin, deux arguments (le nouveau code) | ouvre le bon dossier **et** sélectionne le fichier |

Un second défaut, de la même famille, est retiré au passage : « Ouvrir le dossier » (l'en-tête de
la page) passait par `explorer.exe <dossier>` et gardait en mémoire le dossier calculé au
démarrage. Il relit maintenant le réglage au moment du clic et passe par `ShellExecuteW`, qui
**échoue franchement** au lieu de se rabattre sur Documents ; l'échec s'annonce à l'écran.

| Ce qui a été ajouté | Où |
|---|---|
| `arguments_select()` : deux arguments, jamais un | `src/externe.rs` |
| `ouvrir_dossier()` par `ShellExecuteW`, avec `false` quand Windows refuse | `src/externe.rs` |
| Les deux commandes relisent le dossier, journalisent ce qu'elles ouvrent, signalent un échec | `src/app.rs` |

**Ce que Clément demandait au fond** — pouvoir choisir un dossier, qu'il soit mémorisé, que les
nouveaux fichiers y aillent et que les anciens restent où ils sont — était déjà le
comportement : `reglages.json` garde `dossier_telechargements`, et chaque entrée de
`telechargements.json` garde son propre `chemin`. Ce qui manquait, c'était que le bouton ouvre le
bon dossier.

Preuves : `cargo test` **188/188** (dont le nouveau `montrer_dans_le_dossier_donne_deux_arguments`).
Bout en bout sur le binaire réel, profil jetable avec une destination mémorisée **contenant des
espaces** et un ancien fichier dans un autre dossier : le nouveau fichier arrive dans la
destination mémorisée, l'ancien reste où il est, « Ouvrir le dossier » ouvre la destination
mémorisée, « Afficher dans le dossier » de l'ancienne entrée ouvre **son** dossier — et jamais
Documents. `dist/` 0.4.12 signé (Azure, `Valid`, horodaté) et installé par son propre setup
(silencieux, code 0, `trame.exe` installé identique à `dist\trame.exe`).

## 10 quindecies. 2026-11-23 — les réglages en six rubriques (0.4.40)

Demande de Clément : « restructurer la page paramétrage ». Maquette choisie :
`docs/maquettes/reglages.html` (« toutes, de 1 à 9 »).

| Avant | Après |
|---|---|
| 16 cartes à la suite, dans l'ordre où elles avaient été ajoutées | une colonne à gauche, six rubriques : Général, Apparence, Onglets et mémoire, Confidentialité, Extensions, Données et sauvegarde ; la dernière ouverte est retenue, `reglages.html#extensions` ouvre la bonne |
| aucun moyen de trouver un réglage | « Chercher un réglage » : sans accents ni majuscules, toutes rubriques confondues |
| version, navigateur par défaut et moteur dispersés | bandeau d'état en tête de Général ; point orange sur « Général » si Trame n'est pas le navigateur par défaut ou si une mise à jour attend |
| la pause des onglets cachés dans « Confidentialité », la barre d'extensions seule, l'historique dans « Système » | chacun rangé avec son sujet |
| listes déroulantes pour la barre de fenêtre et le plein écran, boutons texte pour la teinte | vignettes dessinées, pastilles de teinte ; le `<select>` reste la source envoyée à l'hôte |
| extensions Trame et Chrome dans deux cartes, notes techniques toujours affichées | une rubrique, deux onglets, installation en haut, notes repliées sous « Comment ça marche » |
| compteurs de mémoire en bas de carte | en tête de rubrique, avec une barre Trame / extensions Chrome / mémoire libre (`budget.dispoMo`, `budget.extensionsMo`) |
| « Vider l'historique » sans confirmation | zone « Effacer » en rouge ; les deux effacements demandent confirmation |
| « Garder l'affiliation » au même niveau que « Nettoyer » | en retrait dessous, grisé quand le nettoyage est coupé |

Aucun réglage ajouté ni retiré, aucun message vers l'hôte changé : tous les identifiants que le
script et les tests utilisent sont gardés. Tests : `tester-ui.mjs` 77/77 (les cas des réglages
passent par la rubrique avant de cliquer ; nouveau cas « rubriques, recherche, vignettes et
historique confirmé »).
## 10 sexdecies. 2026-11-23 — une teinte change vraiment la coquille (0.4.41)

Constat en éprouvant les teintes : `chrome.css` gardait **28 bleus et violets écrits en dur**
(`rgba(96,165,250,…)`), `--surface` n'y servait qu'une fois, les fonds ne bougeaient jamais,
« Ciel » avait les couleurs exactes de « Défaut », et les pastilles des réglages dessinaient
d'autres couleurs que celles appliquées.

| Correction | Où |
|---|---|
| Une seule table de teintes (fonds, trois surfaces, deux accents), lue par la coquille ET les réglages | `ui/teintes.js` (`window.TrameTeintes`) |
| Toutes les transparences d'accent passent par `--accent-rgb` / `--accent-2-rgb` ; trois fonds en dur deviennent des variables | `ui/chrome.css` |
| « Ciel » devient un vrai bleu ciel (`#38bdf8`), distinct de « Défaut » | `ui/teintes.js` |
| Les pastilles montrent les vraies couleurs ; la page des réglages prend elle aussi la teinte ; les vignettes suivent les surfaces | `ui/reglages.html` |
| « Dynamique » n'est plus annoncé « pas encore branché » : il l'est (meta `theme-color`, validée par l'hôte) | `ui/reglages.html` |

Preuves : `tester-ui.mjs` **78/78**, dont « teintes : chacune repeint fonds, surfaces et accents ».
Sur le **vrai binaire** (profil jetable, fenêtre cachée, CDP) : pour chacune des 7 teintes, rail,
barre et réglages lisent les mêmes `--fond`, `--surface-2`, `--accent`, `--accent-rgb`, et le
fond peint change ; `reglages.json` garde le choix ; le journal note chaque teinte.
**0.4.42** : le Nouvel onglet, les téléchargements, le coup d'œil (`trame.html`) et Zap suivent aussi la teinte (`TrameTeintes.suivre(s)` dans leur `etat`, 18 transparences et 3 fonds en dur remplacés). Vérifié sur le vrai binaire : Ambre puis Violet repeignent Nouvel onglet et téléchargements, Défaut rend l'origine.
**0.4.43 — tout suit la teinte, popups compris.** Les éléments que Trame pose DANS les pages web
ne voient pas les variables de la coquille : ils avaient leurs couleurs en dur. Désormais
`src/teinte.rs` relit `ui/teintes.js` (embarqué, une seule source) et remplace `__TRAME_TEINTE__`
par la palette du moment dans : le **popup de mise à jour** (bouton Installer = accent, texte
sombre), la **barre du coup d'œil**, la **recherche dans la page** (Ctrl+F), le **sélecteur de
Zap** (halo, étiquette, menu, champ) et la **pastille des vidéos** (à la création de l'onglet).
Le fond des webviews avant le premier rendu prend le fond de la teinte (au lieu d'un bleu fixe).
Dans la coquille : menus, poignées de défilement, pastilles et textes bleu clair passent par
`--surface-4` et `--accent-clair` (dérivés par `color-mix`). Seul reste fixe ce qui a un sens :
rouge = fermer/danger, vert = actif, ambre = tiède, blanc = texte, carré du logo.
Preuves : `cargo test` **194/194** (palette lue dans `teintes.js`, popup injecté), `tester-ui`
**79/79** (scripts injectés avec une palette Menthe) ; vrai binaire en Ambre : la recherche dans la
page et la barre du coup d'œil sont ambrées (captures).
**0.4.45 — le double-clic renomme un groupe.** La tête d'un groupe capture le pointeur pour le
glisser-ranger ; Chromium lui renvoie alors `click` et `dblclick`, et le nom ne recevait jamais le
double-clic (signalé par Clément le 23/11). Le double-clic s'écoute sur la tête ; le champ de
saisie survit aux poussées de l'hôte et ne se referme qu'une fois (retirer le champ déclenche
`blur`). Preuve : `tester-ui` **80/80**, dont un vrai double-clic souris qui échouait avant.
**0.4.46 — des infobulles à la teinte.** Le rail dessine ses bulles (`brancherBulles` dans
`chrome.js`) au lieu de celle de Windows : surface et liseré de la teinte, pointe vers l'élément,
geste (« Double-clic ») et touches (« Ctrl+B ») en pastilles, explication après « : » en
seconde ligne. Les `title` restent la source : au survol ils passent dans `data-bulle`. Rail
replié et barre gardent la bulle native (une webview de 64 px ou 40 px couperait la nôtre).
Preuve : `tester-ui` **81/81**.
**0.4.48 — des infobulles Trame partout.** « Tu as oublié les autres » (Clément, 23/11) : une seule
implémentation, `ui/bulles.js`, chargée par la coquille et par réglages, nouvel onglet,
téléchargements et zap. Mise en forme tirée du `title` : version à part, motif d'adresse en police
fixe, gestes en pastilles regroupés en pied (« Cliquer pour », « Clic : … · clic droit : … »).
Ce qu'une webview ne peut pas peindre hors de ses bords passe par la **colonne du panneau de la
barre** : la barre demande `panneau` + `zone`, l'hôte l'agrandit et ne découpe que la carte.
Puces d'extensions : carte sous la barre. Rail replié (64 px) : message `bulle` relayé par l'hôte
(`app/outils.rs`, avec la hauteur de la fenêtre), carte posée à droite de l'icône, remontée pour
une icône du bas ; ce message ne referme pas un menu ouvert. Preuves : `tester-ui` **84/84**,
`cargo test` **194/194**, `outils/eprouver-bulles.mjs` sur le vrai binaire caché (barre
76 → 198 px puis retour ; carte du rail collée à l'icône, texte entier dans la fenêtre).
(0.4.48 construite mais pas mise en ligne : la 0.4.49 la contient.)
**0.4.49 — le clic droit d'un accès du rail replié.** Replié, le rail ouvrait un Nouvel onglet à
la place du menu (« le clic droit ne fonctionne pas », Clément, 23/11). Il envoie `menuAcces`,
l'hôte le relaie à la barre avec la hauteur de la fenêtre, et la barre dessine le même menu
(`menuAcces` dans `chrome.js`, commun aux deux) collé au rail, à la hauteur de l'icône. Preuves :
`tester-ui` **85/85** (menu, Ouvrir, Modifier avec l'ancienne adresse et le groupe), banc
`eprouver-bulles.mjs` sur le vrai binaire (clic droit réel : menu dans la barre, entier).
**0.4.50 — le rail replié à son goût.** « Dissocier visuellement les applications ancrées et les
onglets » (Clément, 23/11) : maquette `docs/maquettes/rail-replie-acces.html` (4 variantes), puis
« prendre les meilleurs » dans Réglages > Apparence > Rail replié. Deux choix combinables :
**accès rapides** `cases` (origine), `socle`, `ronds`, `grille` ; **groupes d'onglets**
`etiquettes` (origine), `cartes`, `trait`. `reglages.json` (`rail_acces`, `rail_onglets`, valeurs
inconnues refusées), message `railStyle`, attributs `data-rail-acces` / `data-rail-onglets` sur
`<body>`, tout le dessin en CSS et seulement rail replié. La bulle de la tête d'un groupe donne
son nom (seul endroit où il se lit en style `trait`). Preuves : `tester-ui` **86/86** (quatre
combinaisons mesurées, déplié inchangé), `cargo test` **194/194**, vrai binaire caché (réglage
appliqué au rail, enregistré, valeur inconnue refusée).
## 11. L'état aujourd'hui

```
cargo test --release            194/194
node outils/tester-ui.mjs        79/79
node outils/verifier-app.mjs     sans reseau : 24/24 (dont les scenarios `z` et `z2`, sauvegarde)
node outils/verifier-setup.mjs   refuse (code 3) tant qu'une vraie installation existe — par conception
```

**Ce qui reste ouvert, dit franchement :**

- La **signature** des extensions n'est pas vérifiée (WebView2 charge du local). L'interface le dit.
- Ombres des fenêtres posées sur la page : une fenêtre à part (transparente, traversée par les
  clics) les dessine ; elles suivent la fenêtre de Trame, mais pas encore un défilement *animé*
  du menu (elles se recalent à la géométrie suivante).
- Dark Reader dit « Cette page est protégée par le navigateur » dans sa fenêtre : il cherche
  l'onglet actif par l'API `tabs`, que WebView2 n'expose pas. Ses réglages globaux fonctionnent.
- Fils cloisonnés : une extension déposée à la main **sans clé** reçoit dans chaque profil un
  identifiant calculé sur son chemin ; sa fenêtre d'action peut alors ne pas s'ouvrir dans une
  cloison. Les extensions du magasin, elles, gardent leur identifiant partout.
- Chaque extension Chrome active coûte de la mémoire (service worker) : 711 Mo avec un onglet
  réveillé sans extension Chrome, 825 Mo avec uBlock Origin Lite et Dark Reader (même session).
- Les extensions Chrome s'appliquent **aussi** au rail et à la barre (ce sont des pages web).
- Pas de `webRequest` complet, pas de `nativeMessaging`, pas de navigation privée, pas de
  synchronisation.
- Le **clavier** est testé dans Chromium (17 combinaisons), mais pas dans la fenêtre : piloter le
  clavier depuis un autre processus perd des frappes.
- Le dossier **est un dépôt git** (`main`, de la 0.4.10 à la 0.4.12), mais **aucun dépôt distant**
  n'est configuré : l'histoire est locale, rien n'est publié.
- La **sauvegarde du profil est manuelle** : rien ne la planifie (pas de copie à date fixe dans
  un dossier choisi).
- L'**import** de favoris HTML aplatit les dossiers de Chrome : un favori n'a pas de groupe dans
  le modèle, seuls les accès rapides en ont.

## 12. Où lire quoi

| Question | Réponse |
|---|---|
| Pourquoi les onglets s'endorment ? | `docs/CONCEPT.md` § vitalité, et la mesure de 112 Mo/onglet |
| Comment l'interface est servie ? | `docs/ARCHITECTURE.md` § 5 (et pourquoi pas `file://`) |
| Quels pièges ont coûté du temps ? | `docs/ARCHITECTURE.md` § 8, 9, 14 |
| Comment écrire une extension Trame ? | `docs/FORMAT-EXTENSION.md` |
| Comment installer/gérer une extension Chrome ? | `README.md` § Installer une extension Chrome |
| Qu'est-ce qui est testé, et comment ? | `README.md` § Tester, et `_tools/rapports/2026-11-21-tests-trame.md` |
