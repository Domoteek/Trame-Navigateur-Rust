#!/usr/bin/env node
/**
 * Banc d'essai du TUNNEL INTÉGRÉ — étape 1 : la configuration WireGuard.
 *
 * Décision de Clément (2026-11-27) : le VPN doit être intégré à Trame, et Trame s'installe par
 * utilisateur (aucun droit administrateur). La première étape, celle qui ne demande aucun droit, est
 * livrée ici : lire un fichier `.conf`, le valider, et garder la clé privée **chiffrée par Windows**
 * (DPAPI) — jamais dans un fichier de réglages.
 *
 * Vérifie, sur le vrai binaire :
 *   1. importer une configuration valide : l'interface l'affiche (adresses → endpoint, clé abrégée) ;
 *   2. `tunnel.json` ne contient NI la clé privée NI quoi que ce soit de secret, et `tunnel.cle`
 *      existe et ne contient pas la clé en clair ;
 *   3. la configuration survit à un redémarrage (la clé se déchiffre au lancement) ;
 *   4. « Oublier » efface les deux fichiers et l'affichage ;
 *   5. une configuration invalide est refusée, sans jamais écrire de fichier.
 *
 * Usage : node outils/eprouver-tunnel-conf.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT = 19487;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-tunnel-conf-"));
const dossier = mkdtempSync(join(tmpdir(), "trame-tunnel-fichiers-"));
const PAGE_REGLAGES = "trame://localhost/reglages.html";

// Deux clés de test (32 octets en base64, sans aucune valeur réelle).
const CLE_PRIVEE = "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=";
const CLE_PAIRE = "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=";
const ENDPOINT = "193.32.126.5:51820";

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

const conf = join(dossier, "mullvad-fr-01.conf");
writeFileSync(
  conf,
  `[Interface]\nPrivateKey = ${CLE_PRIVEE}\nAddress = 10.66.0.2/32\nDNS = 10.64.0.1\nMTU = 1380\n\n[Peer]\nPublicKey = ${CLE_PAIRE}\nEndpoint = ${ENDPOINT}\nAllowedIPs = 0.0.0.0/0, ::/0\nPersistentKeepalive = 25\n`,
  "utf8"
);
const confInvalide = join(dossier, "casse.conf");
writeFileSync(confInvalide, "[Interface]\nPrivateKey = trop-court\n\n[Peer]\nPublicKey = " + CLE_PAIRE + "\nEndpoint = 1.2.3.4:5\n", "utf8");

writeFileSync(join(profil, "reglages.json"), JSON.stringify({}), "utf8");

let enfant = null;
const lancer = async () => {
  enfant = spawn(BINAIRE, [], {
    env: { ...process.env, TRAME_PROFIL: profil, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
    stdio: "ignore",
  });
  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  const barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000);
  return { rail, barre };
};
const tuer = () => {
  try {
    execFileSync("taskkill", ["/PID", String(enfant.pid)], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
};
const fini = async () => {
  for (let i = 0; i < 60; i++) {
    try {
      process.kill(enfant.pid, 0);
    } catch {
      return true;
    }
    await pause(250);
  }
  return false;
};

/** Ouvre les réglages dans l'onglet courant (page interne : le portier ne la bloque jamais). */
async function ouvrirReglages(barre) {
  await barre.evaluer(`(() => {
    const c = document.getElementById("couture");
    c.focus();
    c.value = ${JSON.stringify(PAGE_REGLAGES)};
    c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
  })()`);
  await pause(2500);
  const cible = (await cibles(PORT)).find((t) => t.type === "page" && t.url.includes("reglages.html"));
  if (!cible) throw new Error("page des réglages introuvable");
  const page = await connecter(cible.webSocketDebuggerUrl);
  await page.envoyer("Runtime.enable");
  await pause(800);
  return page;
}

/** Envoie un message à l'hôte depuis la page des réglages (comme le ferait un bouton). */
const envoyer = (page, message) =>
  page.evaluer(`window.ipc.postMessage(${JSON.stringify(JSON.stringify(message))}); true`);

const affichage = (page) =>
  page.evaluer(`({
    texte: document.getElementById("txt-tunnel-conf").textContent.trim(),
    oublier: !document.getElementById("btn-tunnel-oublier").hidden,
  })`);

try {
  let { rail, barre } = await lancer();
  let page = await ouvrirReglages(barre);
  const avant = await affichage(page);
  console.log("\navant import :", JSON.stringify(avant));
  ok(avant.texte.includes("Aucune configuration"), "aucune configuration au départ");
  ok(!avant.oublier, "« Oublier » masqué quand il n'y a rien");

  // 1. Import d'une configuration valide.
  await envoyer(page, { m: "tunnelImporter", chemin: conf });
  await pause(2000);
  const apres = await affichage(page);
  console.log("après import :", JSON.stringify(apres));
  ok(apres.texte.includes(ENDPOINT), `l'endpoint est affiché (${apres.texte})`);
  ok(apres.texte.includes("10.66.0.2/32"), "l'adresse de l'interface est affichée");
  ok(apres.oublier, "« Oublier » apparaît une fois la configuration importée");

  // 2. Ce qui est écrit sur le disque.
  const fichierPublic = join(profil, "tunnel.json");
  const fichierCle = join(profil, "tunnel.cle");
  const publicTexte = existsSync(fichierPublic) ? readFileSync(fichierPublic, "utf8") : "";
  const cleBrute = existsSync(fichierCle) ? readFileSync(fichierCle) : Buffer.alloc(0);
  console.log(`tunnel.json : ${publicTexte.length} caractères · tunnel.cle : ${cleBrute.length} octets`);
  ok(publicTexte.length > 0, "tunnel.json écrit");
  ok(!publicTexte.includes(CLE_PRIVEE), "tunnel.json ne contient PAS la clé privée");
  ok(publicTexte.includes(ENDPOINT), "tunnel.json garde de quoi reconnaître la configuration");
  ok(cleBrute.length > 0, "tunnel.cle écrit");
  ok(!cleBrute.includes(Buffer.from(CLE_PRIVEE)), "tunnel.cle ne contient pas la clé en clair");
  ok(!cleBrute.toString("latin1").includes(CLE_PRIVEE), "ni en texte lisible");

  // 3. Redémarrage : la configuration revient (la clé se déchiffre au lancement).
  tuer();
  ok(await fini(), "premier processus quitté");
  await pause(500);
  ({ rail, barre } = await lancer());
  page = await ouvrirReglages(barre);
  const relance = await affichage(page);
  console.log("après redémarrage :", JSON.stringify(relance));
  ok(relance.texte.includes(ENDPOINT), "la configuration survit au redémarrage (DPAPI)");

  // 4. Une configuration invalide est refusée, et n'écrase rien.
  await envoyer(page, { m: "tunnelImporter", chemin: confInvalide });
  await pause(2000);
  const apresCasse = await affichage(page);
  ok(apresCasse.texte.includes(ENDPOINT), "une configuration invalide n'écrase pas la bonne");

  // 5. « Oublier » efface tout.
  await envoyer(page, { m: "tunnelOublier" });
  await pause(2000);
  const oublie = await affichage(page);
  console.log("après oubli :", JSON.stringify(oublie));
  ok(oublie.texte.includes("Aucune configuration"), "l'affichage revient au départ");
  ok(!existsSync(fichierPublic) && !existsSync(fichierCle), "les deux fichiers sont effacés");

  page.fermer();
  barre.fermer();
  rail.fermer();
} finally {
  tuer();
  await fini();
  try {
    rmSync(profil, { recursive: true, force: true });
    rmSync(dossier, { recursive: true, force: true });
  } catch {
    /* le ménage de Windows reprendra */
  }
}

console.log(echecs === 0 ? "\nTunnel intégré — configuration : lue, validée, clé chiffrée." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
