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

## 11. L'état aujourd'hui

```
cargo test --release            154/154
node outils/tester-ui.mjs        53/53
node outils/verifier-app.mjs     --reseau : 25/25 (dont magasin, « Ajouter a Chrome », liens externes, outils)
node outils/verifier-setup.mjs   tout vert, inscription navigateur posee puis retiree   (2026-11-22)
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
- Le dossier n'est **pas un dépôt git** : aucune histoire versionnée, aucun `git log`.

## 12. Où lire quoi

| Question | Réponse |
|---|---|
| Pourquoi les onglets s'endorment ? | `docs/CONCEPT.md` § vitalité, et la mesure de 112 Mo/onglet |
| Comment l'interface est servie ? | `docs/ARCHITECTURE.md` § 5 (et pourquoi pas `file://`) |
| Quels pièges ont coûté du temps ? | `docs/ARCHITECTURE.md` § 8, 9, 14 |
| Comment écrire une extension Trame ? | `docs/FORMAT-EXTENSION.md` |
| Comment installer/gérer une extension Chrome ? | `README.md` § Installer une extension Chrome |
| Qu'est-ce qui est testé, et comment ? | `README.md` § Tester, et `_tools/rapports/2026-11-21-tests-trame.md` |
