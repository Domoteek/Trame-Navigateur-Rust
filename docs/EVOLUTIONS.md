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
**0.4.51 — la sauvegarde automatique et la navigation privée.** Deux limites écrites au § 11 ont
cessé d'en être. **Sauvegarde automatique** (`sauvegarde_auto.rs`) : une archive `.trame` dans un
dossier choisi toutes les N heures, rotation après écriture seulement, et **seulement** sur les
noms que Trame écrit (`Trame-AAAA-MM-JJ-HHhMMmSS.trame`, valeurs de date vérifiées : un
`Trame-a-moi.trame` est épargné) ; sans dossier choisi, rien ne part ; l'état (`derniere_ms`,
fichier, erreur) vit dans `sauvegarde-auto.json` et un échec n'insiste pas avant une heure.
**Navigation privée** (`Ctrl+Maj+N`) : un fil `prive` avec son propre profil WebView2
(`prive-<id>`), retiré de la session écrite par une fonction **pure** testée
(`model::session_sans_prives`), sans historique ni mémoire de lecture, et purge du profil au
démarrage suivant. Deux mesures ont conduit le code : Chromium **recrée** le dossier d'un profil
qu'il connaît (d'où le retrait des entrées de `Local State`, et l'aveu que le dossier peut
réapparaître **vide** — insister ne converge pas), et la purge doit passer **avant**
`preparer_contexte`, sinon le moteur tient déjà les fichiers. Preuves : `cargo test` **204/204**
(dont 10 neufs), `tester-ui` **87/87** (nouveau cas de la section sauvegarde automatique, et
`Ctrl+Maj+N` dans le tableau des touches), `outils/eprouver-prive.mjs` sur le vrai binaire
(**cookie posé dans la session privée, absent après** ; session écrite sans le fil), et un profil
d'essai pour la sauvegarde (archive écrite, une ancienne effacée, `notes.txt` intact).
**0.4.52 — deux défauts de la 0.4.51, trouvés par le banc.** Clément a demandé « on est pas censé
avoir la navigation privée ? » : elle existait, mais deux choses la rendaient inopérante.
(1) **L'onglet privé naissait dans le profil principal** : `ouvrir_prive` appelait `ouvrir_onglet`,
qui crée la webview **dans** la fonction (avec `cloison_du_fil` appliqué avant le déplacement), puis
`deplacer_vers_fil` changeait le fil — trop tard. Les cookies de la « navigation privée » étaient
donc ceux du navigateur ; le banc le voyait en ne trouvant **aucun** dossier `WV2Profile_prive-*`.
Correction : `ouvrir_onglet_dans(.., fil_impose)`, l'onglet naît dans le fil privé, la webview est
créée avec le profil jetable. (2) **Les raccourcis ne marchaient pas quand le clavier était dans le
rail ou la barre** : les surfaces recoivent `AMORCE_UI`, pas `raccourcis.js`, et `chrome.js` ne
gère que `Échap`. Correction : les surfaces recoivent `raccourcis.js`, qui s'arrête aux touches
quand `location.search` porte `surface=` (sinon le défilement du rail écraserait la position de
lecture de l'onglet actif, et un Alt+clic sur un accès ouvrirait un aperçu). Preuves :
`eprouver-prive.mjs` **tout tient** (touche réelle dans une webview puis **depuis le rail**, fil
visible dans le rail, profil séparé, cookie dans le profil privé et **pas** dans le principal,
session écrite sans rien de privé, purge au démarrage, cookie disparu), `cargo test` **204/204**,
`tester-ui` **87/87**.
**0.4.53 — trois entrées visibles pour la navigation privée, et un défaut de routage.**
« Comment faire pour transférer en navigation privée, plus intuitif que les touches ? » (Clément,
23/11). Une touche qu'on ne connaît pas n'existe pas : trois entrées, chacune placée là où l'on
regarde déjà — le **pied du rail** (bouton `Privé`, icône « œil barré » dessinée pour l'occasion :
ni cadenas, qui dirait « sécurisé », ni bouclier, qui dit déjà « cloison » ; il reste visible rail
replié, empilé sous la jauge), la **page Nouvel onglet** (sous la barre de recherche : « un onglet
à part, qui ne laisse rien derrière lui »), et le **menu de la page** (« Ouvrir en navigation
privée », une ligne parmi les actions de la page). Toutes envoient le même message que
`Ctrl+Maj+N`. **Et c'est là que le défaut est apparu** : ces messages n'étaient **pas routés** —
ils étaient écrits dans une fonction que seules les commandes de téléchargement atteignent, donc
l'hôte les ignorait **en silence**. Les réglages de la **sauvegarde automatique** de la 0.4.51
avaient le même défaut : l'interrupteur, le dossier, l'intervalle, la rotation et « Sauvegarder
maintenant » ne faisaient rien depuis l'interface (la fonction marchait seulement si
`reglages.json` était écrit à la main). **Pourquoi les tests ne l'ont pas vu** : le banc d'interface
vérifie que la page *envoie* le message, et l'épreuve de la sauvegarde utilisait un profil aux
réglages déjà écrits — personne ne regardait l'**effet**. D'où un banc neuf,
`outils/eprouver-entrees.mjs`, qui clique dans le vrai binaire et regarde ce que l'hôte fait (fil
privé créé, onglet ouvert, réglages écrits). Preuves : `cargo test` **204/204**, `tester-ui`
**88/88** (+1 cas ; le cas du pied du rail dit maintenant « Veille, Privé et Capsules »), les deux
bancs d'épreuve verts.
**0.4.54 — le clic droit d'un onglet propose tout ce qu'on peut lui faire.** « Vérifier aussi à
optimiser le clic droit sur un onglet pour proposer tout ce que l'on peut y faire » (Clément,
23/11). Audit : le menu offrait veille/réveil, recharger (seulement si endormi), couper le son,
copier l'adresse et déplacer vers un fil — **il n'avait même pas « Fermer l'onglet »**. Ajouts :
fermer, **fermer les autres du fil** (le fil, pas tout le navigateur), **dupliquer** (même fil,
juste à côté, par `ouvrir_onglet_dans` — donc le bon profil, cloison comprise), recharger pour
tout onglet, **ancrer/détacher**, **à lire plus tard**, **ajouter le site aux accès rapides**,
**copier le titre** (nouvelle ligne `ligneCopierTitre`). Deux commandes hôte n'existaient pas et
sont écrites : `fermerAutres` et `dupliquer` (`src/app.rs`), routées dans `traiter_ipc` — pas dans
la fonction piège de la 0.4.53. Un test dépendait de la hauteur du menu (« haut = 138 ») : rendu
robuste (« au clic, ou remonté pour tenir dans la fenêtre »), parce qu'un menu plus riche est plus
haut. Preuves : `cargo test` **204/204**, `tester-ui` **88/88** (le cas du menu d'onglet vérifie
sept lignes de plus et clique « Dupliquer » et « Fermer les autres du fil »), `eprouver-entrees`
**7/7** sur le vrai binaire (dupliquer ouvre un onglet de plus, fermer les autres en ferme).
**0.4.55 — les deux trous de sécurité concrets sont fermés.** Choix de Clément sur la liste des
manques (« les deux signatures »). (1) **La mise à jour vérifie son installeur avant de le
lancer** : `src/signature.rs` appelle `WinVerifyTrust` (l'API qu'utilisent SmartScreen et
l'Explorateur), et le téléchargement passe désormais par un événement — c'est le fil principal qui
vérifie et qui refuse, donc Clément lit le refus à l'écran. Un setup non signé, ou dont la
signature ne tient pas, **ne part pas**. Réserve écrite : on vérifie que la signature est valide et
de confiance, **pas qui a signé** — un installeur signé par un autre éditeur de confiance
passerait. (2) **Un `.crx` v3 est vérifié** (`crx::verifier_signature`) : la clé publique de la
preuve doit donner le `crx_id`, et la signature RSA-SHA256 doit tenir sur
`"CRX3 SignedData\0" | u32le(taille de SignedData) | SignedData | ZIP`. Un paquet modifié après
signature est refusé avec son motif. Coût mesuré : **+168 Ko** de binaire (3 236 024 → 3 407 872)
pour RustCrypto (`rsa` + `sha2` avec l'OID de SHA-256, sans lequel PKCS#1 v1.5 ne se vérifie pas).
**La preuve qui compte** : mes fixtures se signaient avec ma propre compréhension du format, donc
elles ne prouvaient rien sur le format réel — un **vrai paquet du magasin** (uBlock Origin Lite,
9,1 Mo, point d'entrée `clients2.google.com`) est vérifié par le code, et des paquets trafiqués
sont refusés (octet modifié, clé qui ne donne pas le `crx_id`, en-tête sans preuve). Preuves :
`cargo test` **212/212** (8 neufs), dont un test qui ne s'exécute que si `TRAME_CRX_REEL` désigne
un paquet. Reste non vérifié, dit à l'écran et dans la doc : la **provenance** (aucune chaîne de
confiance vers Google) et les paquets sans preuve exploitable (`.crx` v2, ZIP nu, dossier).
**0.4.56 — le menu d'un onglet ne s'écrase plus quand on reclique.** Bug signalé par Clément, avec
deux captures : après plusieurs clics droits, le menu gardait un fond absent et se coupait au
milieu de ses lignes. Mesure sur le vrai binaire (`outils/eprouver-menu-onglet.mjs`, trois clics) :
la barre passait de **886 à 76 px** au deuxième clic, et le menu de 795 px s'y dessinait dans
76 px. **Cause** : la règle « fermer le panneau dès qu'on agit ailleurs que dans la barre »
(`traiter_ipc`) s'appliquait au message `menuPage`, qui vient pourtant du **rail** — donc elle
refermait le panneau juste avant que la barre ne dessine le menu ; `menuAcces` (menu d'un accès du
rail replié) avait le même défaut, latent. **Correction** : `menuPage` et `menuAcces` sont exclus
de cette règle, comme `bulle` l'était déjà, et `ouvrirPanneau()` remesure la hauteur même quand le
panneau est déjà ouvert (un second clic change le contenu, donc la hauteur). Banc neuf :
`outils/eprouver-menu-onglet.mjs` — trois hauteurs de clic, la barre reste à 886 px et le menu ne
déborde jamais. Preuves : `cargo test` **212/212**, `tester-ui` **88/88**, banc du menu vert.
**0.4.57 — deuxième cause du menu écrasé : la hauteur mémorisée par la poignée.** La 0.4.56 avait
corrigé le panneau qui se refermait ; Clément a constaté que le symptôme revenait. Mesure : avec une
hauteur mémorisée de **250 px** et un contenu de **893 px**, le menu est **plafonné à 250** — coupe
nette, sans indice. Origine : la **poignée du bas** enregistrait sa hauteur au `pointerup`, **même
sans aucun glissement** — un simple clic (ou un clic à côté) figeait la taille du menu, et rien de
visible ne permettait de revenir en arrière (seul le double-clic sur la poignée, que personne ne
connaît). Trois corrections : la hauteur ne s'enregistre que sur une **vraie traction** (≥ 4 px de
déplacement) ; la clé de stockage passe en `trame.menuPage.hauteur.v2`, donc les valeurs déjà
enregistrées — celle de Clément comprise — sont **ignorées** sans qu'il ait à faire quoi que ce
soit ; et la barre de défilement du menu devient visible (`--texte-3` au lieu de `--surface-4`,
quasi invisible), pour qu'un menu réduit se voie réduit. Preuves : `eprouver-menu-onglet.mjs`
**13 vérifications** (trois clics, barre stable à 886 px, menu jamais débordé, ancienne clé
ignorée, clic qui n'enregistre rien, traction de 120 px qui enregistre 675 px), `cargo test`
**212/212**, `tester-ui` **88/88**.

**0.4.58 — troisième cause : rouvrir le même menu le coupait à 72 % de la fenêtre.** Capture de
Clément sur la 0.4.57 : menu d'un onglet coupé net vers 637 px dans une fenêtre de 882 (72 %). Deux
défauts qui ne se voyaient qu'ensemble, après une fermeture du panneau. Barre : l'ouverture envoie
`panneau` **sans zone**, ce qui efface la zone côté hôte et replafonne la barre à 72 % ; la mesure
qui suit était ensuite **écartée parce qu'identique à celle d'avant la fermeture** — rien ne
repartait. Hôte : même renvoyée, une zone qui change avec une hauteur égale ne refaisait que la
découpe (`superposer`), pas la taille de la barre. Corrections : la barre oublie sa dernière mesure
à chaque ouverture ; l'hôte réapplique toute la géométrie quand la zone change. Le banc gagne
« fermer puis rouvrir deux fois » ; il ferme désormais son Trame par `taskkill /F` (une fenêtre
cachée ignorait la demande polie et gardait le port de débogage, si bien que les essais suivants
parlaient à l'ancienne instance) et accepte `TRAME_BINAIRE` et `TRAME_PORT_ESSAI`. Preuves :
`eprouver-menu-onglet.mjs` **15/15** (réouvertures à 886 px ; contre-épreuve sur la 0.4.57 :
638 px), `cargo test` **212/212**, `tester-ui` **88/88**.

**0.4.59 — une barre de défilement qu'on voit.** Clément : « je dois à peine la voir ». Le menu
portait `scrollbar-width: thin` + `scrollbar-color` ; dans Chromium ces deux propriétés **désactivent**
les règles `::-webkit-scrollbar` et donnaient un fil de 2-3 px gris sur gris. Désormais : rail de
12 px visible (blanc à 6 %), pouce à la **teinte d'accent** (60 %, 85 % au survol, plein pendant la
traction), 40 px de haut au moins, pour toutes les colonnes de menu. Preuves : capture du vrai
moteur (Trame caché, menu bridé à 420 px), `eprouver-menu-onglet.mjs` 15/15, `tester-ui` 88/88,
`cargo test` 212/212.

**0.4.60 — la bulle du Compteur de visites reste lisible.** Clément : « le compteur de visite
disparaît tout seul ? » — c'était voulu (5,2 s), trop court. Choix de Clément : 15 s **et** un clic
la ferme (curseur main, bord plus franc au survol, plus de `pointer-events: none` sauf pendant
qu'elle s'efface). Extension en **1.1.0** : sans cette montée de version, le profil de Clément
aurait gardé la 1.0.0 (une extension livrée ne remplace celle du profil que si elle est plus
récente). Preuves : Chromium à horloge simulée (visible à 14 s, partie à 16 s, effacée moins d'1 s
après un clic), `cargo test` 212/212, `tester-ui` 88/88.

**0.4.61 — un Nouvel onglet plus serré.** Clément : « optimiser l'espace pris dans la page, un tout
petit peu ». Maquette `docs/maquettes/nouvel-onglet-espacements.html` (trois variantes), choix de
Clément : la 3. Page de 1 240 px au lieu de 1 020 (4 fils par rangée sur son écran, moins sur une
fenêtre étroite), titre « Trame Bonsoir. date » sur une ligne, marges du haut et entre sections
réduites d'environ un tiers, tuiles et cartes un peu moins hautes, et chaque carte de fil garde sa
hauteur (`align-items: start` : une rangée ne prend plus celle du fil le plus rempli). Preuves :
`tester-ui` 88/88, rendu vérifié à 1 610 px (4 colonnes) et à 900 px (2 colonnes, sans défilement
de côté).

## 10 septendecies. 2026-11-23 — Trame sur Android : la piste choisie (rien de construit)

Question de Clément : « est-ce que l'on peut imaginer avoir le navigateur sur Android ? » Trois
pistes posées : (1) wry sur la WebView d'Android, sans aucune extension ; (2) une coquille Kotlin
autour de **GeckoView**, le seul moteur intégrable sur Android qui fait tourner des extensions
(WebExtensions : saute-pub porté, Bitwarden pour Firefox) ; (3) la synchro seule par le home lab.
Clément a demandé la maquette de la 2 : `docs/maquettes/android.html` (0385008), six écrans
(nouvel onglet, adresse en bas, fils en cartes, menu extensions/teintes, onglets du PC, vidéo en
incrustation Android). **« Tous me vont »** : les six écrans sont validés tels quels. Outils
présents sur le PC le 23/11 : Android Studio, SDK + NDK dans `%LOCALAPPDATA%\Android\Sdk`,
JDK 21 (Microsoft). Aucun code Android n'est écrit à cette date.

**Le soir même, choix « la 1 » : premier APK (écran 2), dans `android/`.** Coquille Kotlin sans
Compose (vues Android) autour de GeckoView 160 : barre d'adresse en bas, cinq boutons, onglets
(liste), menu en feuille (Recharger, Partager, extensions avec interrupteur), invites des pages
(alert, confirm, select), plein écran vidéo, onglets gardés d'un lancement à l'autre (seul le
montré se charge). Page d'accueil provisoire `assets/accueil.html`. Recherche = Perplexity,
comme `MOTEUR_DEFAUT`.
- **Saute-pub** : les scripts du PC (`extensions/saute-pub`) sont copiés à la construction ;
  manifeste Firefox MV3 à part (`debut.js` en `world: "MAIN"`, blocage par
  `declarativeNetRequest`, `regles.json`), `journal.js` relaie l'état à l'appli (pastille 🛡) par
  `sendNativeMessage`. Seule retouche, faite par Gradle : `window.__trameSansPub` lu par
  `wrappedJSObject` (monde isolé de Firefox). **Éprouvé sur émulateur** (Pixel 3a, Android 14,
  m.youtube.com) : `debut.js` actif, `retirees: 1` (pub retirée de la réponse du lecteur),
  lecteur suivi, pastille « 🛡 1 ». Aucune vraie pub vidéo vue à l'émulateur.
- **Bitwarden** : installé par l'appli depuis addons.mozilla.org au premier lancement
  (2026.11.0), sa page d'accueil s'ouvre en onglet (tabs.create), son popup s'affiche en feuille
  (écran de connexion vu). Piège : `Action.click()` ne rend aucun popup (Gecko ne trouve pas
  d'onglet actif), d'où un repli qui ouvre `popup/index.html` à l'adresse de l'extension
  (`POPUPS`). Connexion au coffre non essayée (compte de Clément).
- **Construction** : GeckoView 160 impose AGP 9.1 (Gradle 9.3.1, compileSdk 37) et Kotlin 2.4
  (plugin posé sur le classpath). `./gradlew assembleDebug` (toutes architectures, ~530 Mo, pour
  l'émulateur), `assembleRelease` (arm64, bibliothèques compressées, **96 Mo**, signé avec la clé
  de débogage) → `dist/Trame-android-0.1.0.apk`. **Installée le 23/11 à 18 h 52 sur le
  téléphone de Clément** (Xiaomi 22101316G, Android 14, accord de Clément) : démarrage sans
  erreur, saute-pub prêt, Bitwarden installé tout seul (page d'accueil en 2e onglet). Vraie pub
  YouTube et connexion à Bitwarden : à essayer par Clément.
- **0.1.1 (b6dd046), installée le 23/11 à 19 h 20.** Clément : « l'application est
  inutilisable, tu peux rien faire ». Cause : un onglet restauré au lancement n'était jamais
  ouvert (`GeckoView.setSession` n'ouvre PAS la session, contrairement à ce que supposait le
  code) → page noire, et l'adresse tapée ne chargeait rien. L'émulateur ne le montrait pas
  (toujours relancé sur un onglet neuf). Aussi : toucher l'adresse ne la sélectionnait pas
  (`setText` annulait `selectAll`). Vérifié sur le téléphone : onglet restauré affiché, adresse
  remplacée en tapant, m.youtube.com chargé avec 🛡 actif. Le Xiaomi refuse une installation par
  câble tant que la demande à l'écran n'est pas acceptée (`INSTALL_FAILED_USER_RESTRICTED`).
- Pas encore là : barre qui se replie au défilement, glisser sur l'adresse, fils, teintes,
  téléchargements, vidéo en incrustation.

## 10 octodecies. 2026-11-24 — une notice et une aide dans Trame (0.4.62)

Demande de Clément, après avoir cherché comment faire un écran partagé (« il faut un système
d'aide à l'utilisation et une notice ») ; cinq propositions sur `docs/maquettes/aide.html`, il a
choisi « les cinq ». Pendant le travail, une précision : la pastille vidéo doit dire clairement ce
que fait chacun de ses deux boutons — avec « Sortir la vidéo », il croyait devoir rester sur
l'onglet, alors qu'avec l'icône de droite on peut changer d'onglet.

| Ce qui change | Où |
|---|---|
| **Une seule source** pour toute l'aide : `window.NOTICE = {…}` (JSON strict après le `=`) — 14 fiches, les raccourcis, les astuces, les nouveautés. Un script et pas un `.json` : la page s'ouvre aussi en `file://` pour le PDF | `ui/notice.js` |
| **La Notice** : sommaire et recherche à gauche, une fiche à droite (étapes, conseils, « Essayer maintenant », voir aussi) ; les touches deviennent des `<kbd>` ; `?impression` = toutes les fiches à la suite, sur fond blanc | `ui/aide.html` |
| **F1** (fiche du contexte : trame, côte à côte, vidéo sortie, privé, réglages, sites vidéo…), bouton **?** de la barre, ligne « Notice » du menu `⋯`, bouton dans Réglages → Général → Aide | `src/app/aide.rs`, `ui/raccourcis.js`, `ui/chrome.*`, `ui/reglages.html` |
| **La Couture répond** : mots et expressions de chaque fiche, sans accents ni mots vides ; une question ou une phrase nette passe **en tête** avec un bouton « Faire », un mot seul se range en fin de liste, une adresse n'affiche rien | `src/aide.rs`, `src/app.rs` (requête) |
| **Ctrl+/** : le calque des raccourcis, dessiné par la barre dans son panneau ; Échap le referme d'abord (`window.__trame_echap`, appelé par `raccourcis.js`) au lieu d'arrêter la page | `ui/chrome.js` |
| **Astuces** : allers-retours entre deux onglets (→ côte à côte), vidéo quittée qui joue encore (→ fenêtre à part), page refermée puis retapée (→ Ctrl+Maj+T). Une à la fois, une par jour au plus, jamais deux fois la même (`astuces_vues`), coupables ; posée dans la page comme le popup de mise à jour, ses trois boutons ne sont acceptés que tant qu'elle est affichée | `src/app/aide.rs`, `src/model.rs` |
| **PDF** : `outils/faire-notice.mjs` imprime `aide.html?impression` avec Edge sans fenêtre (profil jetable) ; `faire-setup` le met dans `{app}\ui\notice.pdf` et le menu Démarrer (« Notice de Trame ») ; « Télécharger la notice » le copie dans les téléchargements. Sans PDF (poste de dev) : la page d'impression | `outils/`, `trame.iss` |
| **Vidéo** : boutons « **Vidéo seule** » (Alt+P) et « **Fenêtre à part** » (Alt+Maj+P, texte ajouté à l'icône), bulles qui disent ce qui sort et que les deux laissent changer d'onglet | `ui/incrustation.js` |
| **Vidéo seule jamais endormie** : la page annonce `flottant/incrustation` ; l'onglet est traité comme affiché (ni veille, ni budget, ni mémoire basse, ni pause). Avant, rien ne protégeait l'onglet : la veille ou le budget de pages vivantes détruisaient la page et la petite fenêtre disparaissait — sans doute ce qui a fait croire à Clément qu'il fallait rester sur l'onglet | `src/app.rs`, `src/app/flottant.rs` |

**Éprouvé** : `cargo test --release` (218), `node outils/tester-ui.mjs` (95/95, dont 7 nouveaux cas :
chaque raccourci réel de `raccourcis.js` doit figurer dans la notice), et dans le vrai binaire caché
`node outils/eprouver-aide.mjs` (8/8) : vidéo sortie toujours là après « Endormir les onglets
froids » (« 0 onglet(s) en veille »), F1 → fiche `#video`, « split écran » → Côte à côte en tête,
« leboncoin.fr » → aucune aide, calque de 29 lignes, astuce « rouvrir » montrée puis retenue.
**Pas éprouvé** : le bouton « Télécharger la notice » avec un vrai `notice.pdf` installé (il n'existe
qu'après `faire-setup`), et les astuces « côte à côte » et « vidéo » en vraie fenêtre.

## 10 novodecies. 2026-11-24 — des menus qui tiennent sur un portable (0.4.63)

Clément : « sur le PC de mon fils, le menu ⋯ ne fonctionne littéralement pas », puis « ouvre un
menu et redimensionne la fenêtre : le menu reste affiché mais avec rien dedans ». Banc neuf,
`outils/eprouver-menus.mjs` : le vrai binaire caché à quatre tailles (`TRAME_TAILLE`, sonde
ajoutée), chaque menu ouvert comme à la souris, puis une vraie fenêtre redimensionnée
(`outils/redimensionner-fenetre.ps1`, `SetWindowPos` sans activer).

| Constat (mesuré) | Cause | Correction |
|---|---|---|
| Menu ⋯ de 637 px dans une fenêtre de 560 : bas coupé, sans défilement — la section « Côte à côte » (et son Quitter) hors de l'écran | `placerMenuPage` supposait 720 px de fenêtre (`720 - haut`) | l'hôte envoie `hauteurFenetre` / `largeurFenetre` ; le menu se borne à `min(720, fenêtre)` |
| Commandes (rail replié) : 843 px quelle que soit la fenêtre | aucune hauteur maximale | bornées de même, et défilantes |
| Menu ouvert, fenêtre redimensionnée : bas hors de la fenêtre, région de l'hôte restée à l'ancienne place du menu → panneau vide | rien ne réagissait au changement de taille | la fenêtre change de taille → les menus se referment (barre et rail), comme ceux de Windows. Pas d'écouteur `resize` de la barre : l'hôte l'élargit lui-même pour ses menus collés au rail |
| Ctrl+/ rouvert, barre déjà focalisée : le calque se refermait aussitôt | l'hôte donnait le focus APRÈS l'ouverture → `blur` → fermeture | focus d'abord ; un `blur` dans les 400 ms qui suivent l'ouverture du calque est ignoré |

La fiche « Côte à côte » dit aussi comment revenir à une seule page (Quitter garde la page active).
**Éprouvé** : `eprouver-menus.mjs` (8 menus × 4 tailles + 6 redimensionnements, tout tient),
`tester-ui.mjs` 96/96, `cargo test --release` 218. **Pas éprouvé** sur le PC du fils lui-même.

## 10 vicies. 2026-11-24 — vérifier après mise à jour, réparer, rapporter (0.4.64)

Clément : chez un de ses fils, le compteur du lecteur YouTube ne se rafraîchissait plus jusqu'à une
désinstallation complète ; il proposait de vérifier les fichiers à chaque mise à jour. Constat avant
de coder : l'interface est déjà comparée octet par octet à chaque démarrage (`ressources.rs`) ; les
extensions livrées ne l'étaient que par leur NUMÉRO ; le profil du moteur (caches, service workers)
ne l'était pas du tout — c'est lui que la désinstallation remettait à zéro. Trois propositions,
« les trois » :

| Ce qui change | Où |
|---|---|
| **Vérification d'après mise à jour** : `version_vue` dans les réglages ; quand elle change, chaque extension livrée encore présente est comparée par son CONTENU (`dossiers_identiques`) et remise à neuf si besoin, l'ancienne gardée dans `extensions-anciennes\` ; un message dit ce qui a été réparé. Une extension supprimée par l'utilisateur ne revient pas | `src/profile.rs`, `src/app/depannage.rs` |
| **Réparer Trame** (Réglages → Général → Dépannage, ou `> réparer`) : `ClearBrowsingData` cache disque + Cache Storage + service workers pour chaque profil en service (ni cookies ni stockage : les connexions restent), extensions livrées remises à neuf, témoins des extensions Chrome oubliés (réadaptées au démarrage suivant), veille / budget / mémoire basse / pause par défaut | `src/moteur_ext.rs` (`vider_caches`), `src/extensions_chrome.rs`, `src/app/depannage.rs` |
| **Rapport pour Claude** (même endroit, ou `> rapport`) : un `.txt` dans les téléchargements — versions (Trame, Windows, WebView2), fenêtre et échelle, extensions (identiques ou non à la livrée), réglages sans dossiers ni sites exclus, 250 lignes de journal **adresses réduites à leur site** (le PC d'un enfant) | `src/app/depannage.rs` |
| Fiche de la Notice « Quand quelque chose ne va pas » ; la Couture ignore le nom du logiciel (« réparer trame ») et préfère un mot exact à un début de mot (« bloqué » → Dépannage, pas Zappeur) | `ui/notice.js`, `src/aide.rs` |

**Éprouvé** : `outils/eprouver-depannage.mjs` dans le vrai binaire caché (profil « d'une vieille
version » remis à neuf au démarrage, rapport sans adresse ni recherche, réparation complète,
autres réglages intacts — 22/22), `cargo test --release` 220, `tester-ui.mjs` 97/97.
**Pas éprouvé** : sur le PC du fils, où le lecteur YouTube était figé.

## 10 unvicies. 2026-11-25 — le voile gris de YouTube (0.4.65)

Clément : sur le PC d'un de ses fils, « un filtre gris apparaît » sur YouTube, plus rien n'est
cliquable dans la vidéo ; plein écran puis retour répare, un rafraîchissement le remet.

| Constat | Cause la plus probable | Correction |
|---|---|---|
| Le saute-pub cachait `ytd-enforcement-message-view-model` (« les bloqueurs de publicités ne sont pas autorisés ») et `debut.js` retirait ce message de la réponse du lecteur — mais rien ne s'occupait de la boîte de dialogue qui le porte, ni de son **voile** `tp-yt-iron-overlay-backdrop`, plein écran et semi-transparent, qui prend les clics | la boîte restait ouverte, vide ou cachée : voile gris sur la vidéo, à chaque chargement | saute-pub **2.1.0** : la boîte d'avertissement est cachée elle aussi, et un voile ouvert alors qu'**aucune** fenêtre de YouTube n'est visible (deux relevés de suite, 1,4 s) est refermé, le défilement rendu, le compteur `voiles` incrémenté. Les vraies fenêtres (consentement, partager…) gardent le leur |
| Pas de moyen de savoir, chez le fils, si c'était bien cela | — | le rapport pour Claude liste les **compteurs des extensions** (nombres seulement) : `voiles` > 0 confirmera la cause |

Écartée en mesurant : un consentement aux cookies non retenu (le choix tient après rechargement).
**Éprouvé** : `tester-ui.mjs` 100/100 (3 cas : voile « bloqueurs » retiré, voile de « Partager »
gardé, voile en avance sur sa fenêtre gardé) ; vrai YouTube dans Trame caché, profil neuf :
consentement gardé avec son voile, puis lecteur cliquable, rien fermé à tort (`voilesFermes` 0).
**Pas éprouvé** : la fenêtre « bloqueurs » réelle (YouTube la montre selon le compte) — d'où le
compteur dans le rapport.

## 10 duovicies. 2026-11-25 — saute-pub en mode discret (0.4.66)

Après la 0.4.65, Clément : « YouTube capte que j'ai un bloqueur, je n'ai plus le droit de
regarder ». Le saute-pub 2.x retirait les pubs de la réponse du lecteur (`debut.js`) et bloquait
les adresses publicitaires : c'est ce que YouTube détecte. Choix de Clément (« 1 ») : **saute-pub
3.0.0**, `debut.js` et `bloquer` retirés ; les pubs se chargent, `contenu.js` les rend muettes,
×16, les mène à leur fin et clique « Passer » (le comportement de la 1.0). Encarts masqués,
SponsorBlock et voile orphelin gardés. Éprouvé : 220 + 100/100, vrai YouTube dans Trame caché
(lecteur cliquable). **Pas éprouvé** : chez le fils, face à la détection réelle — rien ne garantit
que YouTube ne repère jamais l'accéléré.

**0.4.67 — déplacer les onglets dans les groupes.** « Je peux déplacer les groupes, il faudrait
aussi pouvoir déplacer les onglets dans les groupes » (Clément, 26/11). Le rail ordonne ses onglets
dans l'ordre de `session.onglets` : le déplacement se fait donc à la source, et l'ordre est écrit
dans la session. `model::deplacer_onglet(session, id, avant, fil)` (fonction pure, 2 tests) reçoit
`avant` = l'onglet devant lequel poser, `None` = fin de son fil, et `fil` pour changer de groupe en
même temps. Côté interface, le glisser **déjà présent** des onglets gagne deux cibles : **moitié
haute** d'un onglet = devant lui, **moitié basse** = juste après ; le trait de dépôt reprend la
forme de celui des groupes. Le dépôt sur l'en-tête d'un groupe continue de tisser dans ce fil.
Message `ongletDeplacer`, routé dans `traiter_ipc`. Preuves : `cargo test` **222/222**,
`tester-ui` **101/101**, et `outils/eprouver-onglet-deplacer.mjs` sur le vrai binaire (rail
`[1,2,3,4]` → `[4,1,2,3]`, **et l'ordre écrit dans `session.json` le garde** — le troisième maillon
est celui qui compte au redémarrage). Limite : ce glisser est celui du navigateur, il n'existe pas
au doigt ; le geste des groupes, lui, est au pointeur.

**0.4.68 — un onglet neuf va dans un groupe choisi.** « Quand on fait ajouter en haut, ça crée un
groupe et un onglet » (Clément, 26/11). **Le pourquoi, écrit pour ne pas le redécouvrir** : un
onglet neuf n'a aucun contenu ; or le tissage note un fil par provenance, domaine, proximité
lexicale et fraîcheur — sans contenu, aucun fil ne marque de points, donc `weave::evaluer` ne rend
aucun candidat au-dessus du seuil et `ouvrir_onglet` **crée un fil**. Le comportement était logique
et n'était pas le bon. Trois corrections :
(1) chaque groupe porte un **« + »** (`Nouvel onglet dans ce groupe : s'ajoute à la fin de « … », sans
tissage`) qui envoie `{m:"nouvelOnglet", fil: <id>}` — le fil est **imposé** ;
(2) le **« + » de la tête du rail**, **Ctrl+T** et la commande « Nouvel onglet » ouvrent dans le fil
de l'onglet **affiché** (`App::fil_affiche`), donc plus de groupe improvisé ;
(3) le tissage automatique reste pour les onglets ouverts **depuis une page** (lien, adresse), où
provenance et domaine veulent dire quelque chose. « ＋ Nouveau fil » reste le geste explicite.
Preuves : `cargo test` **222/222**, `tester-ui` **102/102** (+1 cas : le « + » du groupe demande son
fil ; celui de la tête n'en force aucun), et `outils/eprouver-nouvel-onglet-groupe.mjs` sur le vrai
binaire : `[{"fil":1,"onglets":1}]` → après le « + » du groupe `2` onglets, après celui de la tête
`3`, **aucun groupe créé**, et la session écrite met les trois dans le fil 1.

**0.4.69 — le côte à côte demande où il met la page.** « Dès que je mets un côte à côte ça la met à
droite ; et si je clique sur une autre fenêtre ça remplace celle de droite, mais celle de gauche ne
se remplace jamais » (Clément, 26/11). **Mesuré avant de toucher** (sonde
`outils/sonder-cote-a-cote.mjs`, vrai binaire, profil neuf, 3 onglets) : un Maj+clic donnait
toujours `(actif, nouveau)` — la gauche était donc « la page où j'étais », jamais un endroit ;
depuis la page de **gauche** le nouveau venu remplaçait la droite (la gauche intacte, d'où le
constat), et depuis la page de **droite** il remplaçait la gauche **en échangeant les deux places**
(`(a,b)` devenait `(b,c)` : la page lue sautait à gauche). Un clic simple sur un troisième onglet
n'affichait plus la paire, mais le rail gardait les étiquettes « gauche » / « droite ».
Correctifs :
(1) `model::placer_cote(paire, actif, nouveau, cote)` — fonction **pure**, 4 tests — fait désormais
prendre à la page neuve la place de celle qu'on **ne lit pas** : la page lue ne change **jamais** de
côté ;
(2) le menu dit les deux côtés, sans détour : **« À gauche »** et **« À droite »** remplacent la
ligne unique « À côté de « … » », et le clic droit sur un onglet du rail, pendant un côte à côte,
propose **« Remplacer la page de gauche »** et **« Remplacer la page de droite »** — c'est la
destination, choisie à la main ;
(3) les marques « gauche » / « droite » du rail ne s'affichent que si la paire est **à l'écran**
(une paire en attente ne ment plus).
Choix gardé : le clic simple continue d'afficher l'onglet **seul** (la paire reste en attente) —
c'est le seul geste qui redonne une page pleine largeur sans passer par « Quitter », et la Notice le
dit déjà. Preuves : `cargo test` **226/226** (+4), `tester-ui` **105/105** (+3), et
`outils/eprouver-cote-a-cote.mjs` sur le vrai binaire : Maj+clic → `(1, 3)` ; depuis la gauche →
`(1, 2)` ; depuis la droite → `(3, 2)`, **la page lue reste à droite** ; le menu remplace la page de
droite → `(3, 1)` ; la session écrite garde `cote: [3,1]`. Limite : deux pages au plus, et les deux
doivent être éveillées (le côte à côte réveille l'autre).

**0.4.70 — plus de page blanche derrière un onglet.** « J'ouvre un raccourci YouTube, je vois déjà la
flèche de retour active ; je lis une vidéo, retour → l'accueil YouTube, encore retour → about:blank »
(Clément, 26/11). **Mesuré avant de toucher** (`outils/eprouver-retour-onglet.mjs`, CDP
`Page.getNavigationHistory` sur le vrai binaire) : l'historique de l'onglet était
`["about:blank", "https://example.com/"]`, position 1, bouton actif — exactement le constat. La cause
n'est pas un oubli mais la **conséquence de l'ordre voulu** : la webview naît sur `about:blank` et
n'est chargée qu'ensuite (c'est ce qui évite la page inerte d'Amazon et de YouTube), or un `navigate`
**ajoute** une entrée au lieu de la remplacer. Correctif : la **première** navigation passe par
`location.replace(<adresse>)` (`remplacer_entree`) — elle remplace l'entrée blanche, donc
l'historique commence à la vraie page et « Retour » reste inactif tant qu'il n'y a rien de vrai
derrière. Repli : si le document n'est pas prêt, `load_url` ordinaire (l'état d'avant). Preuves :
`cargo test` **226/226**, `tester-ui` **105/105**, et le banc sur le vrai binaire :
`["https://example.com/"]`, position 0, bouton **inactif**, document `complete` (titre « Example
Domain » — donc pas de page inerte) ; puis `["…/", "…/?deux"]` et le Retour ramène sur la première,
bouton de nouveau inactif. Piège écrit dans `docs/ARCHITECTURE.md` (« Piège n° 4 »).

**0.4.71 — une fenêtre demandée sans adresse ne crée plus d'onglet.** « Sur une recherche Google,
cliquer un résultat me crée un nouvel onglet alors qu'il n'y a pas besoin » (Clément, 26/11).
**Mesuré** : un site qui demande une fenêtre **vide** (`window.open()`, donc `about:blank`) puis
écrit dedans retombe, la fenêtre étant refusée, sur la navigation de la page courante — et Trame
créait malgré tout un onglet blanc, qui ne sert à rien : un onglet de trop, à côté de la page qui
navigue. Preuve sur une page de mesure locale (`outils/banc-onglets.html`, vrais clics CDP) :
`window.open()` + repli → **1 page avant, 2 après** (`about:blank` en trop) ; un lien `target=_blank`
et un `window.open(adresse)` → un onglet avec **leur** adresse, ce qui est le comportement de Chrome.
Correctif : dans le traitement de `NouvelleFenetre`, une demande **sans adresse** est journalisée et
**ne crée aucun onglet** ; les fenêtres qui portent une adresse ouvrent un onglet comme avant.
Preuves : `cargo test` **226/226**, `tester-ui` **105/105**, `outils/eprouver-onglet-fenetre.mjs`
sur le vrai binaire (1 → 1 page pour la fenêtre vide, le repli navigue ; +1 onglet pour `_blank` ;
+1 pour `window.open(adresse)` ; la session écrite ne contient aucun onglet blanc).
Limite dite franchement : le clic synthétique sur un **vrai** résultat Google n'aboutit pas (une
couche maison avale les clics CDP), donc le cas Google est reproduit par un **équivalent** mesuré,
pas par Google lui-même ; si un onglet de trop subsiste, c'est l'adresse de cet onglet qui dira
lequel des deux chemins reste en cause.

**0.4.72 — une fenêtre demandée par une page ne crée plus d'onglet.** Deux captures de Clément
(26/11) : depuis une recherche Google, le même article s'ouvre dans **deux** onglets, et celui
d'origine a même le bon historique (« le premier onglet peut revenir en arrière, mais cliquer le lien
m'a créé aussi un nouvel onglet que je ne veux pas »). **Mesuré** sur une page de mesure locale
(`outils/banc-onglets.html`, clics réels CDP) : le motif des moteurs et des boutiques est
`const w = window.open(url); if (!w) location.href = url;` — la fenêtre est refusée (`w` nul), le site
navigue donc lui-même, et Trame ouvrait **en plus** un onglet : `?fenetre-puis-repli` apparaissait
**deux fois**. Correctif : `NouvelleFenetre` **ne crée plus d'onglet** quand il y a un onglet
d'origine — la page s'ouvre dedans (`App::naviguer_dedans`, extrait de `naviguer_actif`, donc la barre
d'adresse et cette règle ne peuvent plus diverger) ; une demande **sans adresse** ne fait rien
(0.4.71) ; sans onglet d'origine (panneau d'extension, vue trame), un onglet reste la seule place
possible. Preuves : `cargo test` **226/226**, `tester-ui` **105/105**, et le banc :
`window.open()` → 1 onglet ; lien `target=_blank` → 1 onglet, la page s'ouvre dedans ;
`window.open(adresse)` → 1 onglet ; `window.open(adresse)` + repli → **1 page, pas 2** ; session
écrite : **1 onglet, aucun blanc, aucun doublon**. Conséquence assumée et écrite dans
`docs/ARCHITECTURE.md` (« Piège n° 5 ») : un lien `target="_blank"` remplace la page courante (le
Retour y ramène) au lieu d'ouvrir un onglet.

**0.4.73 — un onglet de téléchargement ne reste pas noir.** Deux captures de Clément (26/11, vidéo
YouTube en plein écran puis bascule vers Discord) : lancer un téléchargement (Trame est le navigateur
par défaut) donnait une **fenêtre noire** et un navigateur à fermer de force ; au redémarrage, le même
onglet **retéléchargeait** le fichier. Le journal du profil réel a donné la chaîne exacte :
`lien externe : cdn.discordapp.com/…/Trame-0.4.39-setup.exe` → `onglet 415 reveille` →
`telechargement : … -> Downloads\Trame-0.4.39-setup (1).exe` → `!! onglet 415 : premier chargement
sans fin apres 12 s (jamais commence)`, puis au démarrage suivant la même séquence. **Cause** : la
navigation vers un fichier **est** le téléchargement, donc aucune page n'arrive jamais ; l'onglet
reste vide (noir) et la session le restaure, ce qui relance le fichier. **Mesures avant de choisir le
remède** (`outils/sonder-telechargement-onglet.mjs`, serveur local lent) : refermer l'onglet **en plein
téléchargement** laisse le fichier se terminer (24 Mo complets) ; le refermer **dès
`DownloadStarting`** l'annule (0 octet). D'où la règle : un onglet **sans page** dont l'adresse est
**celle du fichier** (comparaison hôte + chemin, `urls::meme_fichier`, les CDN changeant leurs jetons)
est noté, puis **refermé dès que des octets coulent** (`suivre_telechargements`, ou au premier état du
moteur) ; s'il a un historique derrière lui (une page qui a navigué vers le fichier), il **revient en
arrière** et la page reste. Preuves : `cargo test` **227/227**, `tester-ui` **105/105**, et le banc
`outils/eprouver-telechargement-onglet.mjs` : l'onglet du fichier apparaît puis **se referme seul**,
le fichier de **16 Mo arrive entier**, la **session écrite ne le garde pas** (plus de
re-téléchargement), et le clic sur un lien de fichier **depuis une page** laisse la page en place.

**0.4.74 — le plein écran d'une vidéo ne casse plus.** « Cette fois ça ne plante plus, mais la vidéo
reste beuguée et la fenêtre saute ; si je double-clique ça la remet dans la fenêtre normale, et il faut
redoubler-cliquer pour avoir un plein écran correct » (Clément, 27/11, capture d'un direct avec son
chat). **Le journal du profil réel** montre la chaîne : `onglet 418 : plein ecran (ecran)` →
`lien externe : …Trame-0.4.39-setup.exe` → `onglet 419 reveille` (le lien prend l'écran) →
`fermeture de l'onglet 419 : on active 418`. **Cause** : le moteur garde SON état de plein écran
(`ContainsFullScreenElement`) ; Trame sortait du plein écran (pour montrer l'onglet du lien) **sans le
lui dire**. La page restait donc en affichage plein écran dans une fenêtre normale, et le moteur
réclamait le plein écran que Trame venait de quitter — d'où la fenêtre qui saute. Trois corrections :
(1) un **lien reçu de Windows s'ouvre en arrière-plan** quand une page est en plein écran
(`ouvrir_onglet_mode(..., premier_plan = false)`), donc la vidéo ne bouge pas et le fichier arrive
quand même ; (2) quand Trame sort **vraiment** du plein écran, il **prévient la page**
(`document.exitFullscreen()`), et il le fait **avant** de changer d'onglet — mesuré : une page passée
en arrière-plan n'a plus le droit d'en sortir elle-même, `exitFullscreen` restait sans effet ;
(3) un **garde-fou d'1,5 s** (`plein_ecran_quitte`) : un « plein écran » signalé juste après une
sortie est le reflet de l'état d'avant, on n'y rentre pas. Preuves : `cargo test` **227/227**,
`tester-ui` **105/105**, et le banc `outils/eprouver-plein-ecran-telechargement.mjs` en mode
**« ecran »** (celui de Clément) : lien déposé pendant un plein écran → fichier de 8 Mo **entier**, page
**toujours en plein écran**, **aucun onglet de trop**, onglet affiché inchangé ; puis un vrai
changement d'onglet → la page **sort** du plein écran (états égaux), sans rentrée parasite.

**0.4.75 — le portier « tunnel obligatoire » (premier morceau du VPN).** Question de Clément
(27/11) : « est-ce qu'on peut prévoir l'utilisation de VPN ? » Mesures d'abord, sur cette machine :
**WireGuard n'est pas installé**, aucune carte VPN/Wintun/TAP, aucun profil VPN Windows, et Trame
s'installe **par utilisateur** (`PrivilegesRequired=lowest`) donc tourne **sans droits admin**. Un
tunnel WireGuard embarqué demandera donc un pilote (Wintun), des routes, un DNS et un kill switch —
tous derrière l'élévation : décision prise, un **service compagnon installé une fois**. Et un essai du
mécanisme de proxy (`outils/sonder-proxy.mjs`, SOCKS5 local) a montré que le moteur passe bien par le
proxy **en lui donnant le nom d'hôte** (`example.com:443`), donc sans fuite DNS.
Avant le tunnel lui-même, le **portier** (choix de Clément : « refuser de naviguer ») est livré :
- `src/reseau.rs` — `tunnel_parmi` (fonction **pure**, 4 tests) décide ce qu'est un tunnel : carte
  **montée** et (type *tunnel* **ou** nom connu : wireguard, wintun, tap-windows, openvpn, tailscale,
  zerotier, nordlynx, proton, vpn). Le **PPP seul ne compte pas** (une connexion PPPoE n'est pas un
  tunnel). Enumération Windows par `GetAdaptersAddresses` (features `IpHelper`, `Ndis`, `WinSock`).
- Réglage `tunnel_oblige` (Réglages → Confidentialité → **Tunnel (VPN)**), avec l'état réel juste à
  côté (« Tunnel détecté : … ») et un bouton **Relire**.
- Portier : la garde de navigation refuse tout site quand le réglage est allumé sans tunnel
  (`urls::interne_ou_locale` laisse passer `trame:`, `about:`, `file:`, `localhost`/`127.0.0.1` et
  `*.localhost` — sans quoi on ne pourrait plus atteindre les réglages pour l'éteindre), et une
  navigation lancée par Trame est remplacée par la page `ui/tunnel.html?url=…` qui explique et propose
  **« Réessayer »**. L'état du réseau se relit au battement (au plus une fois toutes les 5 s).
Preuves : `cargo test` **231/231** (+5), `tester-ui` **107/107** (+2), et le banc
`outils/eprouver-tunnel.mjs` sur le vrai binaire : portier allumé → la page du tunnel s'affiche,
**aucun document du site n'est chargé**, les **réglages restent accessibles**, puis le portier éteint
par la **vraie bascule** → l'adresse s'ouvre. **Reste à faire**, dit franchement : le tunnel
lui-même (import de configuration, `boringtun`, Wintun, routes/DNS, kill switch) et son service
compagnon — c'est le gros chantier suivant, pas un réglage.

**0.4.76 — tunnel intégré, étape 1 : la configuration (et la clé chiffrée).** Clément : « il faut que
ce soit intégré donc vas y ». Constat d'architecture, décisif : un tunnel WireGuard « classique » sur
Windows demande un **pilote** (Wintun), des routes, un DNS et un kill switch — donc des droits
administrateur, alors que Trame s'installe par utilisateur. Pour un **navigateur**, il existe une
voie sans pilote : WireGuard **en espace utilisateur** + un **proxy local** que Trame traverse
(`--proxy-server`, mesuré fonctionnel le 27/11 : le moteur passe par le SOCKS5 et lui donne le **nom**
d'hôte). C'est cette architecture qui est retenue ; elle se fait en deux étapes, et voici la première.
`src/tunnel.rs` : lecture d'un fichier `.conf` de n'importe quel fournisseur, avec des règles strictes
et des messages qui disent quoi corriger — **clés 32 octets en base64** (et une clé **nulle** est
refusée : c'est un refus explicite de chiffrer, pas une clé), `Endpoint` en `hote:port`,
`Address`/`DNS`/`AllowedIPs` en CIDR, commentaires et options inconnues tolérées comme le fait
WireGuard, `AllowedIPs` par défaut `0.0.0.0/0, ::/0` (sinon la configuration ne servirait à rien).
La **clé privée n'est jamais écrite en clair** : elle part chiffrée par **DPAPI** (compte Windows de
l'utilisateur, donc inutilisable sur une autre machine) dans `tunnel.cle`, et `tunnel.json` ne garde
que le public. Réglages → Confidentialité → **Tunnel intégré (WireGuard)** : « Importer… » (sélecteur
de Windows, ou un chemin imposé), le résumé (`10.66.0.2/32 → 193.32.126.5:51820`, clé **abrégée**),
et « Oublier » qui efface les deux fichiers. La configuration se relit au démarrage (déchiffrement
DPAPI). Preuves : `cargo test` **239/239** (8 nouveaux tests, dont l'aller-retour DPAPI et « le JSON
public ne contient pas la clé privée »), `tester-ui` **107/107**, et le banc
`outils/eprouver-tunnel-conf.mjs` sur le vrai binaire : import → `tunnel.json` **sans** la clé,
`tunnel.cle` de 262 octets **sans** la clé lisible, **redémarrage** → la configuration revient,
configuration cassée → **rien n'est écrasé**, « Oublier » → **les deux fichiers disparaissent**.
**Reste** : le tunnel lui-même (boringtun + pile TCP/IP en espace utilisateur + proxy local + DNS dans
le tunnel) — c'est l'étape suivante, et elle se mesurera avec deux pairs locaux, puis avec une vraie
configuration de fournisseur. Note : `tunnel.cle` n'est **pas** dans les sauvegardes, volontairement
(une clé DPAPI ne se déchiffre pas sur une autre machine : on réimporte le `.conf` après restauration).

**0.4.77 — le portier laisse passer le réseau local.** Clément a importé sa vraie configuration
Mullvad (`fr-par-wg-003`, `10.66.163.176/32 → 193.32.126.68:51820`, DNS `100.64.0.63`) et allumé le
portier : le journal a immédiatement montré le défaut —
`navigation refusee (tunnel absent) : https://192.168.1.5:8006/#v1:0:=node%2Fproxmox` puis
`https://www.amazon.fr/gp/css/order-history`. Bloquer un site public est voulu ; bloquer **Proxmox**
ne l'est pas : une adresse du réseau local ne traverse pas le tunnel, elle n'a rien à y faire.
`urls::interne_ou_locale` accepte donc désormais, en plus des pages internes et de la boucle locale :
`10/8`, `172.16/12`, `192.168/16`, `169.254/16`, l'IPv6 **ULA** `fc00::/7` et **lien-local** `fe80::/10`,
les noms en `.local`/`.lan`/`.home`/`.internal`/`.localdomain`, et les **noms sans point** (`proxmox`,
`nas`). Preuves : `cargo test` **239/239** (le test des adresses locales couvre maintenant les seize
cas), et `outils/eprouver-tunnel.mjs` sur le vrai binaire avec une étape de plus : portier allumé,
`http://10.255.255.1:9/` **n'est pas** remplacée par la page du tunnel. Au passage, la vraie
configuration importée confirme la chaîne : `tunnel.json` (303 octets) **sans** clé privée,
`tunnel.cle` (262 octets) chiffrée, et l'interface affiche le résumé.

**0.4.93 (2026-11-26) — le mandataire tient la charge : Google, YouTube, Amazon, GitHub s'ouvrent.**
Clément : « beaucoup de sites ne sont pas atteignables, comme Google » (tunnel Nord en OpenVPN). Le
journal montrait des milliers de `TCP SynSent` et des « le DNS du tunnel n'a pas répondu ». Trois
défauts dans `src/mandataire.rs` : (1) **toutes les connexions TCP partaient du port local 40000** :
deux connexions vers le même serveur avaient la même identité et une seule aboutissait ; (2) les
sockets DNS d'un même tour de boucle prenaient **le même port et le même identifiant**, et aucun
socket n'était jamais retiré : les réponses tombaient chez un autre client ou chez un socket mort ;
(3) le relais jetait des octets quand le navigateur était lent (`write_all` sans retenue) et ne
fermait jamais une connexion finie. Correctif : ports locaux distribués (`Ports`, 49152–65535, rendus
à la fermeture), identifiants DNS à la suite, une réponse d'un autre identifiant ignorée au lieu
d'échouer, cache DNS d'une minute, relais à retenue dans les deux sens, fermeture propre (FIN, puis
retrait des sockets ; RST après 10 s), abandon après 20 s sans poignée de main, et boucle qui
n'attend plus quand des octets circulent. Banc nouveau `outils/eprouver-mandataire-sites.mjs` (copie
de la configuration active, Trame caché, 45 requêtes HTTPS **en même temps** sur 15 sites, puis
30 Mo) : version 0.4.92 installée **14/45** (exactement une par site), 0.4.93 **90/90** en deux tours
et **30 000 000/30 000 000 octets à 66 Mbit/s**. `cargo test` 260/260.

**0.4.94 (2026-11-27) — plus de fuite par WebRTC, et un tunnel qui se remonte seul.** Bilan demandé
par Clément après la journée du 26. (1) **Fuite mesurée** : tunnel Nord monté, une page voyait
l'adresse du VPN par HTTP mais la **vraie adresse du poste** par WebRTC (candidat STUN `srflx`). Le
proxy ne couvre pas l'UDP de WebRTC. `app::arguments_navigateur_tunnel` pose désormais, avec le
proxy, `--webrtc-ip-handling-policy=disable_non_proxied_udp` — piège : WebView2 **ignore** le nom de
Chrome `--force-webrtc-ip-handling-policy` (essayé, fuite intacte, ligne de commande du moteur à
l'appui). Après : **0 candidat**. (2) **Le tunnel WireGuard mourait sur un paquet refusé** :
`NoCurrentSession` trois fois le 26 (15 h 29, 15 h 49, 17 h 14), le fil du mandataire s'arrêtait et
plus rien ne sortait, pastille toujours verte. `vpn.rs` jette maintenant ces datagrammes comme
WireGuard (compteur `refuses`, une ligne de journal par minute), relance une poignée de main sur
`ConnectionExpired`, ignore le `WSAECONNRESET` d'un ICMP. (3) **Reprise automatique** : le fil du
mandataire garde son écoute (le port est inscrit dans WebView2), refuse vite les connexions pendant
la coupure, rappelle la préparation (`Fn`, plus `FnOnce`) toutes les 2 → 30 s ;
`Mandataire::en_service()` fait passer la pastille au rouge et fermer le portier, messages « Tunnel
coupé » / « Tunnel rétabli ». Sonde `TRAME_SONDE_COUPURE_S=N` (une coupure, N s après le montage).
Banc nouveau `outils/eprouver-fuites.mjs` : HTTP ≠ vraie adresse, WebRTC sans la vraie adresse ni
candidat STUN, puis coupure → rien ne sort (59 ms), retour seul sur le même port, page rechargée
par le tunnel — **tout vert**. `cargo test` 262/262 (dont « un datagramme refusé ne fait pas
tomber le tunnel »), `tester-ui` 114/114, banc de charge 135/135 sur 0.4.93 comme sur 0.4.94.
**Reste à surveiller** : de rares requêtes figées jusqu'au délai de curl (3 sur 270 au banc de
charge, microsoft.com surtout), vues sur 0.4.94 mais pas expliquées par ses changements (le relais
n'a pas bougé) ; piste : l'assembleur de smoltcp (4 trous au plus) quand des paquets se perdent.
→ **Élucidé le 2026-11-27 : ce n'est pas Trame, c'est le serveur.** Sonde `TRAME_SONDE_RELAIS`
(trace des 60 derniers en-têtes TCP par connexion, connexions sans progrès depuis 8 s, fin de chaque
connexion avec sa raison). Sur 12 pages Amazon de 6 Mo en parallèle par le VPN Nord : sept fois sur
douze, Amazon **accuse réception du ClientHello en 58 ms puis raccroche à 3 s** sans un octet (curl
code 35), et une autre fois il répond à la requête **15 s** plus tard — bridage anti-robot d'une
adresse de VPN connue (sans VPN : douze fois 200 en 0,6 s). Aucune perte, aucun retard de notre côté
dans les traces : accusés immédiats, fenêtre ouverte (256 Ko). Contre-épreuve : 12 × 6 Mo en
parallèle chez Cloudflare par le même tunnel → **entiers en 3,5 s (~165 Mbit/s)**. Piège rencontré :
à force de bancs (Trame tués par `/F`, sessions jamais closes), Nord répond `AUTH_FAILED` — limite
d'appareils du compte ; espacer les bancs OpenVPN.

**Favoris : fenêtre de Ctrl+D, modification, dossiers (2026-11-27, après 0.4.94).** Bilan du
27/11 comparé à Chrome, Firefox et Opera : les favoris étaient en retard sur les accès rapides
(ni modifier, ni ranger) et un second Ctrl+D les retirait sans rien demander. Choix de Clément :
« 1, 2 et 3 ». (1) **Ctrl+D ou l'étoile** gardent la page puis ouvrent une fenêtre **sous l'étoile**
(`App::favori_de_la_page` → `window.trame.carteFavori`, dessinée dans la zone des commandes de la
barre) : Nom, Dossier, « Supprimer », « Terminé ». Entrée ou un clic ailleurs enregistre, Échap
referme sans toucher au nom ni au dossier ; un second Ctrl+D rouvre la fenêtre (« Modifier le
favori ») au lieu de retirer. (2) **« Modifier… »** au clic droit d'un favori du Nouvel onglet :
nom, adresse, dossier (`favoriEnregistrer`, refus d'une adresse déjà en favori). (3) **Dossiers** :
`Favori.dossier` (vide = hors dossier), `favoris-dossiers.json` pour les dossiers vides, rangée
« Tous / dossiers / + dossier » au-dessus des favoris (clic droit : renommer — vers un nom existant
= fusion — ou supprimer, les favoris restent), et choix du dossier au clic droit d'un favori.
**L'import HTML garde les dossiers** (pile `<H3>`/`</DL>`, chemin « Parent › Enfant », racines
d'office comme « Barre de favoris » écartées) et l'export écrit un `<H3>` par dossier. Preuves :
`cargo test` 263/263 (dont un import de Chrome à sous-dossiers et dossier vide), `tester-ui`
116/116 (deux cas : dossiers du Nouvel onglet, fenêtre de la barre), et une sonde sur le vrai
binaire caché : Ctrl+D tapé **dans la page** → fenêtre ouverte, barre agrandie à 296 px, nom et
dossier écrits dans `favoris.json`, second Ctrl+D = « Modifier », « Supprimer » éteint l'étoile.
Pas encore fait : une page de gestion des favoris (recherche, tri, glisser), ni le rangement à la
souris ; `favoris-dossiers.json` n'est pas dans la sauvegarde du profil (pas plus que
`acces-groupes.json`), mais les dossiers portés par les favoris y sont.

**0.4.96 (2026-11-27) — tunnel exigé mais absent : rien ne sort, plus d'application muette.**
Suite du bilan du 26. (1) **Fuite mesurée** : tunnel exigé vers un serveur injoignable, pages bien
bloquées par le portier, mais le journal réseau du moteur (`--log-net-log`) montrait
`api.bitwarden.com`, `fillassist.bitwarden.com` et `config.edge.skype.com` **en direct** — sans
mandataire, l'environnement WebView2 démarrait sans proxy. Désormais le mandataire **démarre même
si le premier montage échoue** : son écoute reste ouverte, le moteur passe par elle, et le fil
réessaie (2 → 30 s) ; message « Tunnel : démarrage impossible (…). Trame réessaie, rien ne sort en
attendant » (`Mandataire::a_deja_servi`). Sans configuration lisible du tout, le moteur est branché
sur un port fermé (`mandataire::port_ferme`, choisi UNE fois : toutes les webviews d'un
environnement doivent avoir les mêmes arguments). Après : **0 connexion vers l'extérieur** en 40 s.
(2) **Barre et rail impossibles à créer** (0x8007139F, 26/11 à 10:02 et 10:08, application muette) :
Trame relancé une seconde après sa fermeture, le moteur de l'ancienne instance tenait encore le
dossier. `profile::attendre_moteur_libre` attend (8 s au plus) qu'aucun `msedgewebview2.exe` n'ait
ce `--user-data-dir`, et le rail est retenté trois fois (1,5 s). Relance immédiate éprouvée : 180 ms
d'attente puis surfaces créées (l'échec d'origine n'a pas été reproduit, faute d'arguments
différents en 0.4.94 ; il le serait désormais, le proxy dépendant du tunnel). (3) OpenVPN : une
session fermée par le serveur (DISCONNECTED) se voit à la passe suivante (`pomper`, avant toujours
`Ok`), avec les dernières lignes du moteur dans la raison — les champs `journal` et `fin`, jusque-là
inutilisés, servent à ça. (4) Ménage : `entree` (vpn.rs) retiré, `age_handshake` réservé aux tests,
plus aucun avertissement de compilation ; `clippy` installé (37 remarques de style, aucune de
justesse). L'erreur « Identifier 'ipc' has already been declared » venait d'une très ancienne
construction (dossier `en-developpement`, chrome.js de 17 Ko) : rien à corriger. Banc
`eprouver-fuites.mjs --tunnel-absent` ajouté ; tout vert, comme le banc complet sur Mullvad.

**Liens vers un autre site : un onglet juste à côté (2026-11-27, choix de Clément).** Depuis 0.4.72,
tout lien `target="_blank"` remplaçait la page courante : cliquer un lien de la description d'une
vidéo YouTube faisait perdre la vidéo. Règle voulue : même site → même onglet ; autre site → nouvel
onglet à côté. La décision se prend **au clic** (`ui/raccourcis.js`, écoute en capture sur `window`),
pas sur la fenêtre demandée : couper le clic coupe aussi le `window.open` du site, donc pas de repli
et pas de doublon (le piège de 0.4.72 reste fermé). Message `lienACote` → `App::ouvrir_lien_a_cote` :
même fil, juste après l'origine (et après ses autres « enfants »), `parent` = l'origine ; fermer
l'onglet ramène à l'origine (`fermer_onglet` la préfère désormais). Exceptions : pages des moteurs de
recherche (le résultat reste dans l'onglet), pages de connexion (`accounts.`, `login.`…). Ctrl+clic
et clic du milieu ouvrent toujours à côté, en arrière-plan (avant : ils remplaçaient la page, effet
de bord de 0.4.72). Redirections `youtube.com/redirect`, `google.*/url`, `l.facebook.com` lues jusqu'à
la destination. Garde-fous : un onglet par 400 ms côté page, par 300 ms côté hôte. Bancs : cas
« liens » de `tester-ui`, `outils/eprouver-liens-a-cote.mjs` (vrai binaire, `127.0.0.1` ≠ `localhost`).

## 10 tervicies. 2026-11-27 — ranger les téléchargements à l'arrivée

Demande de Clément, en regardant FolderFresh (outil Windows qui range un dossier après coup) :
« c'est exactement ce que je veux, c'est exactement ce que mes enfants veulent ». Maquette
`docs/maquettes/rangement.html`, variante 3 choisie (types + règles + aperçu). Idées seulement :
FolderFresh est en C# et sous GPL-3, aucun code repris.

**Le principe : choisir le dossier AVANT le premier octet.** `DownloadStarting` donne déjà la main
sur le chemin (`SetResultFilePath`) : `src/rangement.rs` (module pur, testé seul) décide le
sous-dossier, `telechargements.rs` l'applique. Rien ne tourne en fond, rien n'est déplacé, et
Trame sait ce qu'un outil de dossier ignore : le site d'origine et le type MIME annoncé.
Ordre : règles de haut en bas (site, type, morceau du nom ; `{année}`, `{mois}`, `{date}`, `{nom}` ;
renommage qui garde l'extension) → six types par défaut → la racine. Chemins nettoyés
(jamais `..`, ni absolu, ni nom réservé de Windows). **Coupé par défaut.**

Réglages → nouvelle rubrique **Téléchargements** (`ui/rangement.js` : tuiles des types, règles en
phrases glissables, « Essayer » calculé par Trame lui-même, arborescence avec ce qui a été rangé).
Page des téléchargements : pastille « → dossier » (raison au survol) et « Remettre à la racine »
(le sous-dossier vidé est retiré). Fiche dans la Notice.

Preuves : `cargo test` 276/276 (13 nouveaux), `tester-ui` 118/118 (2 nouveaux), et
`outils/eprouver-rangement-telechargements.mjs` sur le vrai binaire caché : `photo.png` → `Images\`,
`facture-edf.pdf` → `Papiers\Factures\2026-11-27 facture-edf.pdf`, `plan.sh3d` à la racine,
fichier sans extension annoncé `image/webp` → `Images\`, puis « Remettre à la racine » suivi par la
liste. **Pas encore fait** : doublons, « demander à chaque fois », ranger ce qui est déjà en vrac
(aperçu puis déplacement annulable) — dessinés dans la maquette, laissés pour une suite.

## 11. L'état aujourd'hui

```
cargo test --release            204/204
node outils/tester-ui.mjs        87/87
node outils/verifier-app.mjs     sans reseau : 24/24 (dont les scenarios `z` et `z2`, sauvegarde)
node outils/verifier-setup.mjs   refuse (code 3) tant qu'une vraie installation existe — par conception
node outils/eprouver-prive.mjs   navigation privee : tout tient (cookie de session privee disparu)
node outils/eprouver-entrees.mjs entrees d'interface : tout repond (les clics, dans le vrai binaire)
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
- Pas de `webRequest` complet, pas de `nativeMessaging`, pas de synchronisation (décisions du
  2026-11-23 : une imitation partielle de `webRequest` serait un piège, cf. `FORMAT-EXTENSION.md`).
  La **navigation privée** et la **sauvegarde automatique**, elles, existent depuis la 0.4.51 —
  avec une réserve mesurée : Chromium peut recréer **vide** le dossier du profil privé qu'on vient
  de purger (il le recrée à chaque effacement), donc on ne promet pas « aucune trace de dossier »,
  mais « aucune donnée de la session privée » — c'est le cookie qui le prouve.
- Le **clavier** est testé dans Chromium (17 combinaisons), mais pas dans la fenêtre : piloter le
  clavier depuis un autre processus perd des frappes.
- Le dossier **est un dépôt git** (`main`), et **il est publié depuis le 2026-11-23** :
  `https://github.com/Domoteek/Trame-Navigateur-Rust` — `main` poussé avec ses 21 étiquettes
  `v0.4.x`, et une **release GitHub** pour la 0.4.50 portant les deux livrables signés.
  Avant cette date, l'histoire était locale : `origin/main` s'arrêtait à la 0.4.38.
- La **sauvegarde du profil** peut être planifiée depuis la 0.4.51 (dossier, intervalle, rotation) ;
  elle n'a pas encore été mesurée sur une longue durée (un cycle de 24 h ne s'éprouve pas en une
  session) : ce qui est prouvé, c'est l'écriture, la rotation et l'état relu.
- Les **dossiers de favoris** n'ont qu'un niveau à l'écran : les sous-dossiers importés deviennent
  un nom « Parent › Enfant ». Pas encore de page de gestion des favoris (recherche, tri, glisser).

## 12. Où lire quoi

| Question | Réponse |
|---|---|
| Pourquoi les onglets s'endorment ? | `docs/CONCEPT.md` § vitalité, et la mesure de 112 Mo/onglet |
| Comment l'interface est servie ? | `docs/ARCHITECTURE.md` § 5 (et pourquoi pas `file://`) |
| Quels pièges ont coûté du temps ? | `docs/ARCHITECTURE.md` § 8, 9, 14 |
| Comment écrire une extension Trame ? | `docs/FORMAT-EXTENSION.md` |
| Comment installer/gérer une extension Chrome ? | `README.md` § Installer une extension Chrome |
| Qu'est-ce qui est testé, et comment ? | `README.md` § Tester, et `_tools/rapports/2026-11-21-tests-trame.md` |
