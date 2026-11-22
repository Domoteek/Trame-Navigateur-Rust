# Trame — concept

> Ce document dit **pourquoi** Trame existe, ce qu'il fait de différent, et surtout ce qu'il
> **ne prétend pas** inventer. Il a été écrit après une revue de l'état de l'art (novembre 2026)
> dont les sources sont citées ; ce qui n'a pas pu être vérifié est signalé comme tel.

---

## 1. Le problème, tel qu'il est mesuré

Un onglet n'est pas un document : c'est un **engagement d'attention**. Les navigateurs le
traitent comme un rectangle dans une liste, et cette abstraction fuit de trois façons.

**a) Les onglets servent de mémoire de rappel.** Chang, Hahn, Kim, Coupland, Breneisen, Kim,
Hwong & Kittur, *When the Tab Comes Due* (CHI 2021, DOI 10.1145/3411764.3445585) suivent
10 travailleurs pendant deux semaines : les onglets tiennent lieu de liste de tâches, et les
fermer fait peur — « effet trou noir ». Environ un quart des personnes interrogées rapportent un
plantage du navigateur ou de la machine par excès d'onglets.
→ [synthèse CMU](https://web-preview.cs.cmu.edu/news/2021/overcoming-tab-overload)

**b) Les outils d'organisation peuvent aggraver le problème.** Ma, Lassila, Nurgalieva &
Lindqvist (CHI 2023, DOI 10.1145/3544548.3580690) : un utilisateur sur quatre se dit submergé, et
réorganiser sans nettoyer *entretient* la surcharge.
→ [communiqué Aalto](https://www.eurekalert.org/news-releases/987256)

**c) Reprendre coûte, et le coût dépend de l'artefact de reprise.** Monk, Trafton &
Boehm-Davis (2008, *J. Exp. Psychol.: Applied*) : le *resumption lag* croît avec la durée et la
demande de l'interruption. Un titre d'onglet seul est un très pauvre artefact de reprise.

Deux chiffres à manier avec précaution, parce qu'ils sont souvent cités faux : la durée moyenne
d'attention sur un écran est de **47 s** (Mark, 2023 — et non « 8 secondes », chiffre sans source
primaire), et « 23 minutes pour reprendre » est la **moyenne d'une étude de terrain** de 2005,
pas un invariant.

**Ce que fait un navigateur aujourd'hui**, en une phrase : soit il vous laisse fermer vous-même
(Chrome, Firefox), soit il endort sur un minuteur (Edge : 2 h ; Chrome : défausse sous pression
mémoire), soit il range dans des espaces et des groupes (Arc, Zen, Vivaldi, Opera). Aucun ne dit
*pourquoi* il agit, ni *combien* une décision coûte.

---

## 2. État de l'art vérifié (2025-2026)

| Navigateur | Ce qu'il fait | Mécanisme |
|---|---|---|
| **Arc** | Espaces, onglets verticaux, archivage auto (12 h par défaut), split view jusqu'à 4 | plus maintenu ; arrêt annoncé au 22/12/2026, migration vers Dia |
| **Zen** | Onglets verticaux, workspaces, *glance* (Alt+clic), split view | zéro IA revendiqué |
| **Vivaldi** | Piles d'onglets à 2 niveaux, mosaïque (*tiling*), sessions, hibernation, panneau d'onglets | le plus riche en fonctions |
| **Edge** | Onglets en veille (gel Chromium, 2 h, exceptions audio/visio/intranet), Energy Saver (30 min / 5 min), Workspaces | distingue **gel** et **défausse** |
| **Chrome** | Groupes manuels, Memory Saver à 3 niveaux (défausse), Tab Search (Ctrl+Maj+A) | pas de regroupement 100 % auto documenté (une suggestion IA existe) |
| **Opera One** | « Tab islands » : un onglet ouvert depuis une page rejoint son îlot, pliable | **seul cas vérifié de regroupement automatique par parenté** |
| **Firefox** | Conteneurs, groupes d'onglets natifs (depuis 137), Tree Style Tab (extension) | arbre seulement par extension |
| **Extensions** | OneTab, Toby/Workona, Tab Session Manager, TabFS (onglets montés en système de fichiers) | archives, tableaux, sessions |

**Déjà courant** (ne rien revendiquer ici) : onglets verticaux, groupes nommés/colorés, veille ou
défausse des onglets inactifs, split view, espaces, sauvegarde de session, recherche d'onglet au
clavier, onglets épinglés, panneau latéral.

**Rare ou absent dans le corpus étudié** (degré de confiance indiqué) :

- Vue **arborescente native** : absente de Chrome, Edge, Firefox, Arc, Zen — *vérifié absent*
  (seulement Vivaldi et des extensions).
- **Regroupement automatique par parenté d'ouverture** : Opera seul — *vérifié unique*.
- **Budget mémoire par onglet visible en continu** : pas trouvé ; Edge propose un détecteur
  *a posteriori* — *probablement absent*.
- **Rétention par domaine** (« archiver après X jours pour ce site ») : non trouvé.
- **Onglets comme système de fichiers** : TabFS, expérimental, aucun navigateur — *vérifié absent*.
- **Vue tissée temporelle** (fils × temps, avec provenance) : non trouvée. Les vues existantes
  sont des listes, des grilles de vignettes ou des arbres.

Sources principales : [Edge — Performances](https://support.microsoft.com/en-us/edge/learn-about-performance-features-in-microsoft-edge) ·
[Chrome — Memory Saver](https://developer.chrome.com/blog/memory-and-energy-saver-mode) ·
[Opera — Tab Islands](https://blogs.opera.com/desktop/2023/06/opera-tab-islands/) ·
[Vivaldi — Onglets](https://vivaldi.com/features/tab-management/) ·
[Arc — fin du développement](https://www.theregister.com/2025/05/27/arc_browser_development_ends/) ·
[Zen](https://zen-browser.app/welcome/) · [Firefox 137](https://www.firefox.com/en-US/firefox/137.0/releasenotes/) ·
[Skeema / Tabs.do (UIST 2021, DOI 10.1145/3472749.3474777)](https://doi.org/10.1145/3472749.3474777) ·
[TabFS](https://github.com/osnr/TabFS)

---

## 3. La thèse

> **Un onglet n'est pas un état (ouvert/fermé) mais une quantité d'attention qui décroît.**
> Le travail du navigateur est de **tisser**, **mesurer** et **expliquer** cette attention —
> puis d'endormir ce qui a refroidi, sans jamais fermer.

Trois conséquences de méthode :

1. **Ne pas optimiser le nombre d'onglets, optimiser la peur de perdre.** L'archive doit être
   réversible et visible. Trame n'a pas de « fermer les onglets froids » : il a *endormir*, et un
   clic rend la page à sa position de lecture.
2. **Stabilité spatiale d'abord.** Une position acquise ne bouge plus. La vitalité se voit
   (couleur, taille du nœud), elle ne **réordonne jamais** le rail. C'est l'enseignement de
   *Data Mountain* (Robertson & al., UIST 1998, DOI 10.1145/288392.288596) et des transitions
   animées de Bederson & Boltman (1998) : la mémoire spatiale bat la liste retriée.
3. **Suggérer, jamais réorganiser d'autorité.** Ma & al. 2023 montre que le réarrangement
   automatique entretient le désordre. Trame propose (« ces deux fils se ressemblent — les
   tisser ? ») et laisse la main.

---

## 4. Les mécanismes

### 4.1 Le fil (weave.rs)

Un fil est un ensemble ordonné d'onglets, avec un nom, une date, et une vie. Un nouvel onglet
est rattaché par un score à quatre termes :

```
score(fil) = 0.78 + 0.12·part_domaine + 0.10·fraicheur      si l'onglet parent est dans ce fil
score(fil) = 0.22·part_domaine + 0.34·lexical + 0.14·fraicheur + 0.10   sinon
seuil de tissage = 0.42
```

- **Provenance** : le signal le plus fiable — c'est ce que fait Opera, et c'est le seul cas de
  regroupement automatique vérifié dans l'état de l'art.
- **Domaine** : grossier mais robuste quand le parent est inconnu.
- **Lexical** : Jaccard des mots significatifs des titres. C'est ce qui distingue **une tâche**
  d'**un site** — exactement ce que Tabs.do (UIST 2021) obtient par apprentissage, ici par une
  heuristique explicable et locale.
- **Fraîcheur** : on rejoint plus volontiers un fil vivant.

Le nom est dérivé des mots les plus fréquents des titres (jamais d'un appel réseau), et
l'utilisateur peut le remplacer d'un double-clic — auquel cas Trame cesse de le renommer.

Deux fils qui se ressemblent (> 0.52) produisent une **suggestion**, pas une fusion.

### 4.2 La vitalité (vitality.rs)

Score dans [0,1], demi-vie **8 minutes** (40 min pour un onglet mis de côté, infinie s'il est
ancré). Il monte au retour (`+0.22 + 0.04·ln(1+nb_focus)`), il se recharge doucement quand
l'onglet est sous les yeux, et il subit une pénalité supplémentaire s'il n'a **jamais** été
regardé après cinq minutes — transposition de l'abandon de « patch » de la théorie du fourrage
d'information (Pirolli & Card, *Psychological Review* 1999) : un onglet quitté sans y entrer a
très peu de chances d'être rouvert.

Quatre paliers, affichés : **vif** (≥0.62), **tiède** (≥0.34), **froid** (≥0.16), **dormant**.
Seuil de veille : 0.16.

### 4.3 La veille — et pourquoi elle est mesurée

Sous le seuil (ou au-delà du plafond de 6 webviews vivantes), Trame **détruit la webview**.
Pas de gel : `wry` détruit le contrôle WebView2, le processus de rendu disparaît, la mémoire
revient. L'onglet garde son URL, son titre, sa position de lecture — et le clic suivant le
réveille à l'endroit exact où il était.

Mesuré sur le poste de Clément le 2026-11-20 (11 onglets en session) :

| État | Mémoire (processus + descendants) | Processus |
|---|---|---|
| 1 onglet réveillé | 660,7 Mo | 10 |
| 6 onglets réveillés | 1 218,8 Mo | 15 |

≈ **112 Mo par onglet réveillé**. C'est la seule justification acceptable de la veille : un
chiffre, pas une intention.

Honêteté sur la mesure : WebView2 partage les processus de rendu **par origine** entre les
webviews d'un même profil. Il n'existe **aucune** API donnant « la mémoire de cet onglet ».
Trame affiche donc le total réel de ses propres processus et le nombre de webviews vivantes ;
il n'invente pas de répartition par onglet.

### 4.4 La trame (UI, Ctrl+Espace)

Une vue tissée, pas une liste : chaque fil est une bande, le temps court de gauche à droite,
chaque onglet est un nœud dont la **taille** dit la vitalité et la **couleur** le palier ; un
trait relie un onglet à celui d'où il vient (provenance). Glisser un nœud sur une bande le
tisse dans ce fil ; un clic l'active.

C'est le seul endroit où la question « qu'est-ce que je faisais ? » se lit d'un coup d'œil au
lieu de se reconstruire mentalement.

### 4.5 La Couture (Ctrl+K)

Une seule saisie : onglets, fils, historique, commandes (`>`), adresse. Le classement est

```
score = 0.62·pertinence + 0.26·vitalité + 0.12·fraîcheur
```

pour les onglets — une liste triée à la seule pertinence textuelle renvoie souvent un onglet
mort. La correspondance est exacte, puis préfixe, puis sous-chaîne en début de mot, puis
sous-séquence.

### 4.6 Les capsules

Un fil scellé en JSON : URL, titres, positions de lecture, mise de côté, vitalité au moment du
scellement. Réouvrir une capsule reconstitue le fil **en veille** — donc sans coût mémoire
immédiat, et avec sa charge d'attention d'origine.

### 4.7 Les extensions

`manifest.json`, motifs d'URL (`*://*.exemple.fr/chemin/*`), CSS et scripts injectés en fin de
chargement, et une API `chrome` **réduite mais réelle** : `storage.local` (persisté par extension),
`runtime.id`, `runtime.getManifest`, et un `dom.style` de commodité. Pas de page d'arrière-plan,
pas de `webRequest`, pas d'API `tabs` : une petite API honnête vaut mieux qu'une imitation qui
ment sur ses capacités.

---

## 5. Ce que Trame ne prétend pas inventer

| Affirmation évitée | Pourquoi |
|---|---|
| « regrouper les onglets automatiquement » | Opera le fait par parenté depuis 2023 ; Tabs.do l'a étudié en 2021. Trame ajoute l'**explicabilité** et la **persistance** du fil, pas le principe. |
| « onglets verticaux / espaces / sessions » | Courant partout (Zen, Arc, Vivaldi, Edge, Wavebox). |
| « mise en veille des onglets » | Edge (gel, 2 h) et Chrome (défausse) le font. La différence est **le déclencheur** (un score d'attention, pas un minuteur) et **la destruction effective** de la webview. |
| « palette de commandes » | Chrome, Arc, Vivaldi en ont une. La différence est le **classement par vitalité**. |
| « vue arborescente » | Tree Style Tab existe depuis longtemps. La trame est autre chose : un graphe temporel, pas un arbre. |
| « navigation privée, blocage de pubs, synchronisation » | Absents. Ce sont des chantiers, pas des promesses. |

Ce qui reste raisonnablement défendable comme **propre à Trame**, au vu de cette revue :

1. **Un score d'attention visible et explicable qui pilote la mémoire**, au lieu d'un minuteur —
   et le budget mémoire réel affiché en continu (absent du corpus étudié).
2. **Un fil persistant, nommé, hérité de la provenance, avec explication et fusion suggérée** —
   là où Opera crée des îlots éphémères par parenté.
3. **Une vue tissée temporelle avec provenance**, là où les navigateurs montrent des listes,
   des grilles ou des arbres.
4. **La capsule de fil avec position de lecture**, comme artefact de reprise (Monk & al. 2008).

---

## 6. Huit principes de conception, et leur source

1. **Rendre l'archivage réversible et visible** — sinon on garde tout ouvert (Chang & al. 2021).
2. **Regrouper par tâche, pas par domaine** — Tabs.do, UIST 2021.
3. **Suggérer, ne pas réorganiser d'autorité** — Ma & al. 2023.
4. **Stabilité spatiale : la position acquise ne bouge plus** — Data Mountain 1998 ; Bederson & Boltman 1998.
5. **Réduire le coût de reprise par un artefact** (provenance, position, capsule) — Monk & al. 2008.
6. **Séparer « en cours » et « à lire plus tard »** — Chang & al. 2021 (d'où le marqueur *à lire* et sa demi-vie distincte).
7. **Endormir sans perdre l'état : suspension ≠ fermeture** — Edge 2020/2022, Chrome 2022.
8. **S'instrumenter soi-même** — aucun chiffre officiel de coût par onglet côté Chromium : Trame mesure ses propres processus.

---

## 7. Questions ouvertes

- **Le seuil de tissage (0.42) est-il juste ?** Il vient de quatre tests unitaires et de
  l'usage, pas d'une étude. Un journal des rattachements refusés permettrait de l'ajuster sur
  des données réelles.
- **La demi-vie de 8 minutes** est un choix de conception, pas une mesure. Elle devrait être
  réglable et observée (combien de réveils par jour ? combien de réveils ratés ?).
- **Le plafond de 6 webviews** est arbitraire ; il devrait suivre la mémoire disponible.
- **Les fils survivent-ils au temps long ?** Au bout d'une semaine, un fil de 40 onglets
  endormis vaut-il mieux qu'une archive ? Non tranché — et c'est précisément le genre de question
  qu'un journal local permettrait de trancher au lieu de la deviner.
