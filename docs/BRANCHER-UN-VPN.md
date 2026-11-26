# Brancher un VPN dans Trame

Ce document explique comment faire passer **toutes les pages de Trame** dans un VPN, en partant d'un
compte NordVPN. La même recette fonctionne avec Mullvad, ProtonVPN, AirVPN… : Trame ne demande qu'un
fichier **WireGuard `.conf`**.

> **Ce que ce n'est pas.** Trame n'est pas un VPN système : seules **ses propres pages** passent dans
> le tunnel. Une autre application (jeu, mises à jour, navigateur concurrent) sort par la connexion
> normale. Pour couvrir tout le PC, c'est l'application du fournisseur qu'il faut installer.

> **Le fichier `.conf` contient une clé privée.** À traiter comme un mot de passe : ne pas l'envoyer
> par courriel, par message, ni le laisser dans un dossier partagé. Trame la range ensuite
> **chiffrée** pour le compte Windows de la machine (DPAPI), jamais en clair dans les réglages.

---

## 1. Installer Trame (aucun droit administrateur)

1. Aller sur la page des versions :
   <https://github.com/Domoteek/Trame-Navigateur-Rust/releases/latest>
2. Télécharger `Trame-<version>-setup.exe`, l'exécuter, suivre l'assistant.
   L'installation vit dans `%LOCALAPPDATA%\Programs\Trame` (l'utilisateur seul), crée un raccourci dans
   le menu Démarrer et une entrée « Applications ».
3. Windows 11 et Windows 10 à jour contiennent déjà **WebView2**, le moteur de rendu dont Trame se
   sert : rien d'autre à installer.

Si l'installation est signée et que Windows demande confirmation : l'éditeur est
`CAHAGNE CLEMENT` (signature Azure Trusted Signing, horodatée).

## 2. Télécharger le fichier WireGuard chez Nord

Chemin **mesuré le 2026-11-28** sur un vrai compte (pas supposé). Attention : **l'interface du
compte Nord est en anglais**, ce qui fait chercher « Configurer manuellement » sans le trouver — le
lien s'appelle **« Set up NordVPN manually »**, et il est tout en bas de la page NordVPN, pas dans
l'accueil du compte.

1. Se connecter sur <https://my.nordaccount.com> → cliquer **NordVPN** dans la barre de gauche.
2. Descendre **tout en bas** de cette page : bloc **Advanced settings** → **Manual setup** → lien
   **Set up NordVPN manually**.
   Adresse directe, si on préfère y aller d'un coup :
   <https://my.nordaccount.com/dashboard/nordvpn/manual-configuration/server-recommends>
3. La page **Manual setup** s'ouvre sur l'onglet **Server recommendation**, avec le pays déjà rempli.
   Prendre un serveur qui porte le badge vert **Verified** (le recommandé convient, par exemple
   `fr1130.nordvpn.com`).
4. Cliquer **Get setup configuration** → la fenêtre demande le protocole : laisser **WireGuard** →
   cliquer **Download config**.
5. **Nord envoie un code à 6 chiffres par courriel** (« Verify your email ») avant de livrer le
   fichier. Le chercher dans la boîte du compte (ou les indésirables), le saisir dans les six cases ;
   le téléchargement démarre ensuite. Le lien **Resend** en renvoie un autre au besoin.

Le navigateur enregistre un fichier du genre `NordVPN_France_<serveur>.conf`.

Deux choses à savoir (lues dans la [documentation Nord](https://support.nord-help.com/hc/fr/articles/19476515228305) le 2026-11-28) :

- **Dix appareils au plus en même temps** sur un compte Nord. Et sur un **même serveur**, deux
  appareils doivent utiliser des protocoles différents : un serveur n'accepte donc **qu'un seul
  appareil en WireGuard/NordLynx** à la fois. Comme le serveur est figé dans le fichier, **chaque PC
  doit prendre son propre serveur** (par exemple deux villes ou deux numéros différents) si les deux
  doivent fonctionner en même temps.
- **Le serveur est figé dans le fichier.** Il n'y a pas de rotation automatique : pour changer de
  pays, retélécharger un `.conf` et refaire l'étape 4.

## 3. Importer le fichier dans Trame

1. Ouvrir Trame → menu **Réglages** (ou `Ctrl+,`).
2. Colonne de gauche : **Confidentialité**.
3. Groupe **Tunnel (VPN)** → carte **Tunnel intégré (WireGuard)** → bouton **Importer…**
4. Choisir le `.conf` téléchargé.

La configuration apparaît dans la liste, avec son résumé (`10.5.0.2/16 → 185.61.156.20:51820`) et la
mention **« active, montée au démarrage »** : c'est la preuve qu'elle est lue et gardée.

**On peut en garder plusieurs** — un fichier par pays. Chacune a sa ligne, avec deux boutons :

| Bouton | Effet |
|---|---|
| **Utiliser** | cette configuration devient celle qui se monte au démarrage ; Trame propose de redémarrer (le mandataire est posé à la création de la page, rien ne change à chaud) |
| **Supprimer** | efface **cette** configuration et sa clé, sans toucher aux autres |

Une seule configuration est active à la fois. La dernière importée le devient automatiquement.

Si Trame refuse un fichier, le message dit **quoi corriger** (clé invalide, `Endpoint` mal formé…) et
ne montre jamais la clé. Rien n'est enregistré, et les configurations déjà là ne bougent pas.

## 4. Exiger le tunnel, et redémarrer quand Trame le propose

1. Toujours dans **Confidentialité**, allumer **Exiger un tunnel avant de naviguer**.
2. Trame propose : « **Redémarrer Trame maintenant ?** » → **Oui**.

Ce redémarrage est obligatoire et c'est normal : le mandataire du tunnel est posé sur la page quand
elle est créée, il ne peut pas changer à chaud. Le redémarrage est propre — la session est écrite
avant la sortie, donc **les onglets reviennent**.

Une fois armé :

- **Tunnel monté** → la pastille VPN de la barre est **verte** ; sa bulle dit
  « VPN actif : toutes les pages de Trame passent par… ».
- **Tunnel exigé mais absent** → la pastille est **rouge** et Trame **refuse d'ouvrir un site** : il
  affiche une page qui explique, avec « Réessayer ». Il n'y a jamais de fuite en clair.
- Les pages internes (`trame:`) et **tout le réseau local** (10.x, 172.16-31.x, 192.168.x,
  `localhost`, `.local`, noms sans point comme `proxmox`) restent accessibles : sinon on ne pourrait
  plus ni se connecter à sa box ni revenir en arrière.

## 5. Vérifier que ça passe vraiment par le VPN

Ouvrir dans Trame : <https://ipinfo.io/json>

- `"ip"` doit être celle du serveur VPN, **différente de l'adresse de la maison** ;
- `"country"` doit être le pays du serveur choisi ;
- `"org"` nomme l'hébergeur du serveur.

Comparer avec un navigateur sans VPN : l'adresse n'est pas la même. C'est la seule preuve qui compte
— un « VPN actif » affiché quelque part ne prouve rien.

## 6. En cas de problème

| Ce qu'on voit | Ce qu'il faut faire |
|---|---|
| « Tunnel refusé : … » à l'import | Le fichier n'est pas une configuration WireGuard exploitable (souvent une configuration **OpenVPN** téléchargée par erreur). Recommencer l'étape 2 en choisissant bien **WireGuard®**. |
| Pastille **rouge**, sites bloqués | Le tunnel n'a pas pu se monter : pas de réseau, endpoint filtré, ou configuration oubliée. Réglages → Confidentialité → **Relire**, puis consulter le journal (voir plus bas). |
| Les pages tournent mais rien ne s'ouvre | Le tunnel est monté mais la sortie ne répond pas : la configuration a pu être révoquée côté fournisseur (une nouvelle configuration en génère une autre, l'ancienne cesse alors de marcher). Retélécharger un `.conf` et le réimporter. |
| Un site précis refuse de s'ouvrir | Certains services bloquent les adresses de VPN. Changer de serveur (nouveau `.conf`). |
| On veut revenir en arrière | D'abord **éteindre** « Exiger un tunnel » (Trame propose de redémarrer : accepter, les pages repassent alors en direct), **puis** **Supprimer** les configurations de la liste pour effacer leurs clés. Dans cet ordre : la proposition de redémarrage n'apparaît que s'il reste une configuration. |

Le journal de Trame est ici : `%LOCALAPPDATA%\Trame\trame.log`. Les lignes utiles commencent par
`tunnel :` (montage, poignée de main, proxy).

## 7. Ce que le tunnel ne fait pas (limites connues, dites plutôt que cachées)

1. **Il ne couvre que Trame**, pas les autres applications du PC.
2. **Un seul tunnel monté à la fois** — la configuration active. Les autres attendent leur tour
   (voir la limite 5).
3. **WebRTC (voix et visioconférence) n'est pas encore forcé dans le tunnel** : un site peut, dans
   certains cas, apprendre l'adresse réelle. Pour les appels sensibles, préférer l'application du
   fournisseur.
4. **UDP en dehors du DNS n'est pas relayé** : les jeux en UDP ne passent pas par ce tunnel.
5. **Les configurations ne permutent pas toutes seules** : on garde autant de pays qu'on veut, mais
   c'est **Utiliser** (et un redémarrage) qui bascule de l'un à l'autre. Il n'y a pas de bascule
   automatique selon le site.
6. **La clé est chiffrée pour le compte Windows de la machine.** Copier le profil sur un autre PC ne
   transporterait pas la clé : il faut refaire l'import là-bas.

---

*Document écrit le 2026-11-28 pour Trame 0.4.84. La procédure Nord a été vérifiée sur la
documentation officielle le même jour ; l'import et la mesure de sortie, eux, se vérifient machine
par machine — c'est l'étape 5.*
