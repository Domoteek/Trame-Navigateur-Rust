# Extensions Chrome (moteur de WebView2)

Ces extensions ne sont pas au format Trame : ce sont de **vraies extensions Chrome**
(manifeste v2/v3), chargées par le moteur de Chromium via l'API WebView2
`AddBrowserExtension`.

## Comment en installer une

Quatre voies, toutes dans Trame (`Ctrl+,` → section **Extensions Chrome**) :

1. **Magasin de Chrome** : collez l'identifiant (32 caractères) ou le lien de la fiche, puis
   **Installer** — Trame télécharge le `.crx`, retire l'en-tête de signature, décompresse et range
   l'extension sous son identifiant, dans `%LOCALAPPDATA%\Trame\extensions-chrome\`.
2. **Fiche du magasin ouverte dans Trame** : cliquez « Ajouter à Chrome ». WebView2 ne sait pas
   installer depuis une page, il télécharge le `.crx` — Trame le reprend et l'installe.
3. **Fichier** : « Installer un fichier .crx / .zip… ».
4. **Import** : « Chercher dans Chrome / Edge », puis « Importer » un dossier déjà décompressé.

**Redémarrer Trame ensuite** : l'installation a lieu au démarrage, une seule fois pour tout le
profil. Un `.crx` téléchargé depuis un site **ordinaire** n'est pas installé en silence : il reste
un fichier dans `telechargements\`, et la voie 3 existe pour ça.

## Ce qui marche, ce qui ne marche pas

| | |
|---|---|
| ✅ | scripts de contenu (`content_scripts`), CSS injecté, `run_at`, `matches` |
| ✅ | service worker d'arrière-plan, page d'action, API `chrome.*` fournies par Chromium |
| ✅ | stockage de l'extension, `chrome.runtime.id` |
| ✅ | installation depuis le Chrome Web Store (identifiant, fichier `.crx`, « Ajouter à Chrome ») |
| ❌ | les extensions qui s'appuient sur une API que WebView2 n'expose pas (liste des pages bloquées, etc.) resteront inertes |
| ⚠️ | elles s'appliquent **aussi aux surfaces d'interface** (rail, barre), parce que ce sont des pages web — une bannière en position fixe peut donc les recouvrir |

## L'identifiant d'une extension, et la clé `key`

Chromium calcule l'identifiant d'une extension chargée **depuis un dossier** à partir de la clé
publique `key` du manifeste — et **du chemin du dossier** quand cette clé manque. Mesuré le
2026-11-21 : un dossier `mon-dossier` sans clé reçoit `chrome.runtime.id` =
`cfmnfkohahigcphgcoimlckammkneabk`, et le même dossier avec la clé du magasin reçoit l'identifiant
du magasin.

Les `.crx` du magasin ne portent pas `key` dans leur manifeste : la clé voyage dans l'en-tête du
CRX. Trame l'y récupère et l'inscrit dans le manifeste installé, donc le dossier porte
l'identifiant du magasin **et** Chromium lui donne cet identifiant — c'est ce qui rend
`chrome-extension://<id>/…` (bouton **Interface**/**Panneau**) utilisable. Un dossier importé
depuis Chrome a déjà sa clé ; un dossier fabriqué à la main n'en a pas, et son interface ne
s'ouvrira pas.

## `preuve-chrome/`

Extension de contrôle : elle injecte une bannière verte avec son `chrome.runtime.id`.
Si la bannière apparaît, c'est bien Chromium qui a exécuté le fichier, pas l'injecteur
maison de Trame (qui ne lit que des manifestes au format Trame, cf.
[`../docs/FORMAT-EXTENSION.md`](../docs/FORMAT-EXTENSION.md)).

Installée et vérifiée le 2026-11-21 : bannière affichée avec `id pfpakjkl…`, donc le moteur
d'extensions fonctionne sous WebView2 157.

## Obtenir le dossier d'une extension du magasin

Le plus rapide est désormais dans Trame : la voie **Magasin de Chrome** (identifiant ou lien) ou
le bouton « Ajouter à Chrome » sur la fiche. Restent deux autres voies, utiles quand on veut la
version exacte d'une autre machine :

- **Importer une extension déjà installée dans Chrome ou Edge.** Réglages (`Ctrl+,`) → section
  **Extensions Chrome** → « Chercher dans Chrome / Edge ». Trame scanne les profils des navigateurs
  Chromium de la machine (Chrome, Edge, Brave), liste ce qu'il trouve avec son nom réel (les
  manifestes contiennent souvent `__MSG_extName__`, résolu via `_locales/`), et « Importer » copie
  le dossier. Vérifié le 2026-11-21 sur ce poste : Bitwarden
  (`nngceckbapebfimnlniiiahkandclblb`, v2026.7.0, manifeste v3, 189 fichiers) importé depuis le
  profil « Profile 1 » de Chrome.
- **À la main** : un `.crx` est une archive ZIP précédée d'un en-tête — le renommer en `.zip` et
  l'extraire suffit souvent ; les extensions installées dans Chrome sont dans
  `%LOCALAPPDATA%\Google\Chrome\User Data\<profil>\Extensions\<id>\<version>\`, déjà décompressées.

## Se connecter et régler une extension importée

Il n'y a **pas de barre d'outils d'extension** dans Trame : une extension n'a pas d'icône
cliquable comme dans Chrome. Il y a en revanche une porte d'entrée :

**Réglages (`Ctrl+,`) → Extensions Chrome → bouton « Interface »** (ou « Options »).
Trame ouvre la fenêtre d'action de l'extension dans un onglet
(`chrome-extension://<id>/popup/index.html`) — l'origine est servie par WebView2 dès que
l'extension est chargée. C'est là qu'on se connecte.

Vérifié le 2026-11-21 avec Bitwarden : la page de connexion s'affiche dans un onglet, et une fois
connecté l'état du coffre reste dans le profil.

**Détail qui a coûté un aller-retour** : une page `chrome-extension://` **ne doit pas être l'onglet
actif au démarrage**. L'installation des extensions est asynchrone, la page se charge avant elles
et WebView2 renvoie `ERR_INVALID_URL`. Trame choisit donc un autre onglet au démarrage ; ouvrir
l'interface après coup fonctionne.

## Ce que l'import ne garantit pas

L'import copie les fichiers ; il ne garantit pas que l'extension **fonctionne**. Elle peut
demander des API que WebView2 n'expose pas (magasin, `sidePanel`, `webRequest` complet,
`nativeMessaging`, menus contextuels). wry ne remonte pas non plus le résultat de
`AddBrowserExtension` : si une extension ne fait rien, le journal ne le dira pas. Le seul
contrôle fiable reste visuel.
