# OpenVPN dans Trame

Demande de Clément (2026-11-28) : « il faut que ça marche avec OpenVPN ». Ce document dit ce que
c'est, ce que ça coûte à construire, et où en est l'intégration.

## Pourquoi un second moteur

WireGuard et OpenVPN n'ont rien en commun. WireGuard échange une clé en un paquet et chiffre avec
`ChaCha20-Poly1305` ; OpenVPN monte un **canal de contrôle TLS**, s'authentifie par certificat ou par
utilisateur/mot de passe, négocie un canal de données, et parle ensuite en `AES-256-CBC` ou `AES-GCM`.
Le moteur WireGuard de Trame (`boringtun`, dans `src/vpn.rs`) ne peut donc pas lire un `.ovpn` — c'est
mesuré : l'import d'un profil OpenVPN répond aujourd'hui
`ligne 1 : « client » n'est pas une affectation`, c'est-à-dire le refus du lecteur WireGuard.

## Le moteur retenu, et pourquoi il colle à Trame

**OpenVPN 3 Core** — la bibliothèque d'OpenVPN Connect — liée en C++ via le crate
[`openvpn-rust-bindings`](https://crates.io/crates/openvpn-rust-bindings) (0.1.0, MPL-2.0, publié le
2026-11-10). Le point qui compte, et qui a décidé du choix :

> Core tourne **dans notre processus**, avec un **tunnel en espace utilisateur**. Il ne crée aucun
> adaptateur réseau : les paquets IP qu'un adaptateur aurait portés sont remis à notre code.

C'est exactement l'architecture de Trame depuis la 0.4.78 — **aucun pilote, aucun droit
administrateur** — et la pile `smoltcp` + le mandataire SOCKS5 déjà écrits pour WireGuard servent tels
quels. Seul le moteur change.

## Ce que la compilation exige (mesuré sur ce poste, 2026-11-28)

OpenSSL, LZ4 et OpenVPN 3 Core sont compilés **depuis les sources** au premier `cargo build`.

| Prérequis | État mesuré | Rôle |
|---|---|---|
| Visual Studio 2022 (C++17) | présent | compiler Core et OpenSSL |
| **Strawberry Perl** | **absent** — installé en portable dans `%TEMP%\strawberry` pour mesurer | OpenSSL est construit par un script Perl |
| Perl livré avec Git | présent mais **inutilisable** (dit par la documentation du crate) | — |
| NASM | absent | optionnel, accélère OpenSSL |
| Droits administrateur **à l'exécution** | **non nécessaires** | — |

Le premier build a pris **16 min 42 s** (profil de test) ; les suivants sont incrémentaux.

Sur Windows, il faut pointer Perl explicitement :

```powershell
$env:OPENSSL_SRC_PERL = "C:\Strawberry\perl\bin\perl.exe"
cargo build --release
```

(ou `OPENSSL_NO_VENDOR=1` + `OPENSSL_DIR` vers un OpenSSL 3 déjà installé, ce qui évite Perl.)

## Où en est l'intégration

Fait, et vérifié par `cargo test` (**259 tests** au 2026-11-28) :

- `src/moteur.rs` : le trait **`Tunnel`** (`etiquette`, `mtu`, `pomper`, `prendre_paquet`,
  `envoyer_paquet`) et **`Parametres`** (adresse dans le tunnel avec son préfixe, DNS du
  fournisseur, MTU). Le mandataire ne connaît plus qu'eux : il ne sait pas quel moteur il porte.
- `src/ovpn.rs` : le moteur est **dans le binaire** ; `ovpn::valider(profil)` lit un `.ovpn` sans se
  connecter (serveur, port, protocole, identifiants nécessaires) ; `ovpn::Session::connecter` ouvre
  la session, attend le montage (`tun_up`) et rend les paramètres poussés par le serveur.
- `src/vpn.rs` : la session WireGuard implémente le même trait ; `demarrer_tunnel(preparer)` accepte
  n'importe quel moteur, la préparation tournant **dans le fil du mandataire**.
- **Rangement des deux genres** (`src/tunnel.rs`) : un profil OpenVPN se range **à côté** des
  configurations WireGuard dans `tunnels/<id>/`, avec `openvpn.json` (résumé public), `profil.cle`
  (le `.ovpn` **entier chiffré par DPAPI** — il peut porter la clé privée du client) et
  `identifiants.cle` (utilisateur et mot de passe, chiffrés). `lister()` rend le genre et le bon
  résumé, `charger_configuration(id)` et `charger_active()` rendent une `Configuration`
  (`WireGuard(Config)` ou `OpenVpn { profil, identifiants }`) : c'est ce que l'application donnera au
  bon moteur. Un fichier refusé par le moteur n'écrit **rien** sur le disque.
- Sonde `TRAME_TUNNEL_ESSAI=3` : mesurée sur un profil sans identifiants →
  `ce profil demande un utilisateur et un mot de passe` ; avec identifiants factices →
  `le tunnel n'a pas monté en 30 s — OpenVPN core 3.11.8 win x86_64 64-bit` (le vrai moteur tourne).

Reste à faire, dans cet ordre :

1. **Application** : lire `charger_active()`, préparer le bon moteur au démarrage (`vpn` ou `ovpn`),
   et publier dans l'état les deux genres (pastille, liste des configurations).
2. **Interface** : accepter un `.ovpn` dans **Importer…**, demander utilisateur et mot de passe quand
   le profil les exige, afficher serveur et protocole, garder la raison d'échec visible.
3. **Bancs** : un profil Nord réel, avec ses identifiants de service, et la mesure de sortie
   (`ipinfo.io`) comme pour Mullvad.

## Limites connues, dites d'avance

1. **Chiffrements obsolètes non supportés** : BF-CBC, DES simple, RC2. `AES-256-CBC` reste accepté
   (`allow_non_preferred_ciphers`), ce qui couvre les fournisseurs actuels.
2. **Compression coupée par défaut** (attaque VORACLE) ; activable seulement si un serveur l'exige.
3. **Nord exige des identifiants de service** (`auth-user-pass`) : ils seront stockés chiffrés, jamais
   dans un fichier de réglages.
4. Le tunnel OpenVPN, comme celui de WireGuard, **ne couvre que les pages de Trame** — pas un VPN
   système.

## Licences

Les bindings sont **MPL-2.0**. OpenVPN 3 Core est **MPL-2.0 _ou_ AGPL-3.0** — c'est la première qui
s'applique ici, donc pas de contamination copyleft de Trame. OpenSSL (Apache-2.0), LZ4 (BSD-2-Clause)
et asio (Boost Software License 1.0) sont compatibles.
