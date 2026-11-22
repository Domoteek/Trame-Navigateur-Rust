# Optimisations — ce qui a été mesuré, ce qui a été fait, ce qui a été refusé

**Date** : 2026-11-21 · **Outil de mesure** : `node outils/mesurer-ressources.mjs`

Un navigateur se juge **au repos** : une page ouverte qu'on ne touche pas. Toutes les mesures
ci-dessous sont prises sur un **profil neuf** (une seule page, `newtab.html`), fenêtre visible,
machine à 16 cœurs, et distinguent deux choses :

- **la coquille** — `trame.exe`, notre code Rust ;
- **l'arbre** — la coquille *et* ses descendants WebView2 (le moteur et les rendus).

La distinction est le point central : sans elle, on optimise du bruit de moteur.

## 1. Ce qui a été fait

### 1.1 La mesure de mémoire ne relit plus tout le système toutes les 4 s

`src/probe.rs` créait un `System` neuf **à chaque mesure** et refaisait un balayage complet de
tous les processus (~400 sur ce poste) toutes les 4 secondes. Le `System` est désormais
persistant, la lecture est **ciblée** sur les PID connus, et un balayage complet ne revient que
toutes les 5 mesures — ou immédiatement si un processus a disparu (l'arbre a changé).

| | Avant | Après |
|---|---|---|
| CPU de la coquille au repos | **0,78 %** d'un cœur | **0,27 %** |
| CPU de l'arbre entier | 2,03 % | 0,94 % |
| Mémoire de l'arbre | 509 Mo | 501 Mo |
| Démarrage (lancement → dernières surfaces) | 673 ms | 754 ms |

**Ce qu'on peut en conclure** : la coquille consomme **65 % de moins au repos**, soit 0,27 % d'un
cœur — 0,017 % de la machine.

**Ce qu'on ne peut pas en conclure** : la mémoire et le démarrage ne bougent pas de façon
significative (l'écart de démarrage, ±80 ms, est du bruit ; la mémoire varie de quelques Mo
selon les rendus). L'arbre entier passe de 2,03 % à 0,94 %, mais une partie de cet écart est du
bruit de WebView2 : **seul le chiffre de la coquille est attribuable au code**.

**Pourquoi seulement 65 % et pas 95 %** : sous Windows, `NtQuerySystemInformation` renvoie **tous**
les processus même pour une lecture ciblée — sysinfo filtre après. Le gain vient donc surtout de
la suppression des `System::new()` (une table vide à reconstruire) et de la lecture ciblée des
propriétés, pas d'un balayage évité. C'est mesuré, pas supposé.

### 1.2 Le rail ne se reconstruit plus à chaque poussee d'état

`ui/chrome.js` reconstruisait tout le rail — dizaines de nœuds, favicons rechargés — à **chaque**
poussée d'état, y compris celles qui ne changent rien (un défilement de page en produit une
toutes les 400 ms). Une **signature de rendu** (fils, titres, états, paliers, favicons) est
maintenant comparée : si rien de visible n'a changé, le DOM n'est pas touché, seuls le budget et
le volet sont rafraîchis.

Vérifié par un test (`outils/tester-ui.mjs`, « une poussée d'état identique ne reconstruit pas le
DOM ») : les nœuds marqués survivent à trois poussées identiques, et un titre qui change
reconstruit bien.

Même traitement pour les puces d'extensions de la barre.

### 1.3 Sauvegarde de session moins gourmande

La session était écrite **toutes les 3 s** quand l'état était modifié, avec un `fsync`
(écriture atomique). Or un défilement marque l'état modifié : pendant une lecture, cela faisait
un `fsync` toutes les 3 s pour une position de lecture. L'intervalle passe à **8 s** ; la session
reste sauvegardée à la fermeture, et le pire cas d'un plantage est de perdre 8 s de position —
pas des onglets.

### 1.4 Mémoire basse pour les onglets ouverts mais cachés (2026-11-21, 18 h 40)

Un palier entre « ouvert » et « en veille ». Un onglet réveillé mais caché depuis **20 s** passe
en `MemoryUsageTargetLevel = Low` (`ICoreWebView2_19`) : WebView2 rend au système ce qu'il peut
de sa mémoire, la page reste ouverte avec sa position et ses scripts. L'onglet affiché est
toujours en niveau normal, et un onglet qu'on affiche y revient **au clic** (mesuré : même
seconde dans le journal). Fenêtre réduite : tous les onglets sont cachés, donc tous y passent.
Réglage « Mémoire basse pour les onglets cachés », allumé par défaut.

Mesure A/B, **même binaire**, réglage coupé puis allumé : session de démonstration de 11
onglets, budget 6 (donc 6 ouverts dont 5 cachés), veille automatique coupée, relevé à 90 s.

| | Coupé | Allumé | Écart |
|---|---|---|---|
| RAM occupée (somme des `WorkingSet`), essai 1 | 1 028,5 Mo | 836,6 Mo | **−191,9 Mo** |
| RAM occupée, essai 2 | 947,9 Mo | 853,8 Mo | **−94,1 Mo** |
| Mémoire privée réservée, essai 1 | 533,2 Mo | 570,6 Mo | +37,4 Mo |
| Mémoire privée réservée, essai 2 | 499,1 Mo | 595,9 Mo | +96,8 Mo |

**Ce qu'on peut en conclure** : la RAM physique occupée — ce que montre le Gestionnaire des
tâches, et ce qui ralentit les autres applications quand elle manque — baisse de 94 à 192 Mo
avec 5 onglets cachés.

**Ce qu'on ne peut pas en conclure** : la mémoire *réservée* ne baisse pas, elle monte même un
peu. Le niveau bas fait sortir des pages de la RAM (vers la mémoire de réserve ou le fichier
d'échange) ; il ne les libère pas. Seule la veille (webview détruite) libère vraiment. Deux essais
seulement, avec un écart notable entre eux : l'ordre de grandeur est sûr, le chiffre exact non.

Preuve automatique : scénario `s` de `outils/verifier-app.mjs` (5 onglets cachés en mémoire
basse, jamais l'onglet visible ; aucun avec le réglage coupé).

### 1.5 Le nombre d'onglets ouverts suit la mémoire libre du PC (2026-11-21, 19 h 30)

Le budget choisi (6 par défaut) devient un **plafond**. À chaque mesure (toutes les 4 s), la
sonde lit aussi la mémoire disponible de la machine ; si elle passe sous une réserve de
**1 500 Mo**, Trame abaisse le nombre d'onglets ouverts de ce qui manque, à raison de **120 Mo par
onglet** (mesure du 2026-11-20 : 112 Mo), et endort les plus froids. Jamais au-dessus du plafond,
jamais sous 2, jamais l'onglet affiché. Réglage « Onglets ouverts selon la mémoire libre », allumé
par défaut ; le pied du rail passe en ambre (« mémoire juste ») et dit pourquoi dans son infobulle.

Le calcul (`model::budget_adapte`) donne un nombre **absolu**, fixé au moment de la mesure : le
recalculer à chaque battement depuis une mémoire libre périmée endormirait des onglets en rafale.

Preuve, scénario `u` : mémoire libre simulée à 1 200 Mo (`TRAME_MEMOIRE_LIBRE`), 6 onglets
ouverts → 3 au plus, puis le plancher de 2 tant que la mémoire ne revient pas ; réglage coupé →
le plafond seul s'applique.

### 1.6 Ce que coûte chaque extension, mesuré (2026-11-21, 19 h 30)

Chromium lance chaque extension dans un processus marqué `--extension-process` (constaté sur le
profil réel : Bitwarden, **184 Mo**). La ligne de commande ne dit pas *quelle* extension il porte ;
d'où trois chiffres, chacun dit pour ce qu'il est :

| Chiffre | Comment | Où |
|---|---|---|
| Total des extensions | somme exacte des processus `--extension-process` | infobulle du pied du rail, tête du puzzle |
| Coût d'une extension, direct | une seule extension active et un seul processus d'extension | puzzle, réglages (« 184 Mo mémoire ») |
| Coût d'une extension, par différence | mémoire des extensions avant, puis 12 s après son allumage ou son extinction ; noté dans `reglages.json` | puzzle (« mesuré à l'allumage »), réglages |

Une extension sans processus propre (scripts de contenu seulement) ne laisse pas d'écart : rien
n'est noté plutôt qu'un chiffre inventé. Preuve, scénario `v` (réseau) : uBlock Origin Lite
installée, son coût mesuré et écrit dans `reglages.json`.

## 2. Ce qui a été essayé et **refusé**

| Essai | Résultat | Décision |
|---|---|---|
| `lto = "fat"` + `codegen-units = 1` | 2 000 384 octets contre 2 124 472 (**−124 Ko, −5,8 %**), mais **82 s** de compilation au lieu de 8 s | refusé |
| `lto = "thin"` + `codegen-units = 1` | 2 060 800 octets (**−63 Ko**), **59 s** de compilation | refusé |
| `panic = "abort"` | gagnerait quelques dizaines de Ko | refusé : sur panique, plus de déroulement, donc plus de `Drop` — le verrou de profil ne serait plus retiré |

Le binaire fait 2 Mo et l'essentiel du poids est ailleurs (interface, extensions) : diviser par
dix la vitesse de compilation pour 6 % de taille est un mauvais échange, et la compilation
rapide sert tous les jours. La mesure est gardée en commentaire dans `Cargo.toml`.

## 3. Ce qui reste, et qui n'est pas optimisable ici

| Poste | Réalité |
|---|---|
| **Mémoire par onglet** (~112 Mo mesurés) | c'est WebView2 : un processus de rendu par origine, plus le processus navigateur. Aucune API ne donne la mémoire d'un onglet ; la seule action possible est de **dormir** les onglets, ce que fait déjà la veille |
| **CPU des pages** | une page qui anime consomme, quoi qu'on fasse ; le budget de webviews vivantes borne le nombre de pages actives |
| **Démarrage** (~700 ms) | dominé par la création des webviews WebView2 (4 surfaces + l'onglet), pas par le code Rust |
| **Taille du binaire** | 2 Mo, dont l'essentiel est wry/tao/serde ; voir § 2 |

## 4. Refair les mesures

```powershell
node outils/mesurer-ressources.mjs --demarrer --duree 40   # profil jetable, 1 onglet
node outils/mesurer-ressources.mjs --duree 40             # instance en cours
```

L'outil affiche : processus, mémoire (arbre et coquille), CPU (arbre et coquille), taille du
binaire et délai de démarrage. C'est ce qui a produit les chiffres de ce document.
