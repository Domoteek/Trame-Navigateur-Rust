#!/usr/bin/env node
/**
 * Banc d'essai du TUNNEL INTÉGRÉ — les configurations WireGuard, plusieurs à la fois.
 *
 * Décision de Clément (2026-11-27) : le VPN doit être intégré à Trame, et Trame s'installe par
 * utilisateur (aucun droit administrateur). Demande du 2026-11-28 : **plusieurs fichiers `.conf`**
 * (un par pays) et un basculement rapide de l'un à l'autre.
 *
 * Vérifie, sur le vrai binaire et par les messages de l'interface :
 *   1. importer une configuration valide : une rangée apparaît, elle est active, et les fichiers
 *      sont rangés dans `tunnels/<id>/` ;
 *   2. `tunnel.json` ne contient NI la clé privée NI quoi que ce soit de secret, et `tunnel.cle`
 *      existe et ne contient pas la clé en clair ;
 *   3. importer une SECONDE configuration : deux rangées, la nouvelle devient active ;
 *   4. « Utiliser » rebascule sur la première, et le choix survit à un redémarrage (DPAPI) ;
 *   5. une configuration invalide est refusée sans rien écraser ;
 *   6. « Supprimer » n'efface que la configuration visée ;
 *   7. l'ancien rangement (deux fichiers à la racine du profil) est repris au démarrage suivant.
 *
 * Usage : node outils/eprouver-tunnel-conf.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT = 19487;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-tunnel-conf-"));
const dossier = mkdtempSync(join(tmpdir(), "trame-tunnel-fichiers-"));
const PAGE_REGLAGES = "trame://localhost/reglages.html#confidentialite";

// Deux clés de test (32 octets en base64, sans aucune valeur réelle).
const CLE_PRIVEE = "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=";
const CLE_PAIRE = "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=";
const ENDPOINT = "193.32.126.5:51820";
const ENDPOINT_B = "185.61.156.20:51820";

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};
const egalite = (obtenu, attendu, quoi) =>
  ok(JSON.stringify(obtenu) === JSON.stringify(attendu), `${quoi} (${JSON.stringify(obtenu)})`);

const conf = join(dossier, "mullvad-fr-01.conf");
writeFileSync(
  conf,
  `[Interface]\nPrivateKey = ${CLE_PRIVEE}\nAddress = 10.66.0.2/32\nDNS = 10.64.0.1\nMTU = 1380\n\n[Peer]\nPublicKey = ${CLE_PAIRE}\nEndpoint = ${ENDPOINT}\nAllowedIPs = 0.0.0.0/0, ::/0\nPersistentKeepalive = 25\n`,
  "utf8"
);
const confB = join(dossier, "nord-fr-02.conf");
writeFileSync(
  confB,
  `[Interface]\nPrivateKey = ${CLE_PRIVEE}\nAddress = 10.5.0.2/16\nDNS = 103.86.96.100\n\n[Peer]\nPublicKey = ${CLE_PAIRE}\nEndpoint = ${ENDPOINT_B}\nAllowedIPs = 0.0.0.0/0\nPersistentKeepalive = 25\n`,
  "utf8"
);
const confInvalide = join(dossier, "casse.conf");
writeFileSync(
  confInvalide,
  "[Interface]\nPrivateKey = trop-court\n\n[Peer]\nPublicKey = " + CLE_PAIRE + "\nEndpoint = 1.2.3.4:5\n",
  "utf8"
);

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

/** L'état affiché : une ligne par configuration, et l'aide quand il n'y en a aucune. */
const affichage = (page) =>
  page.evaluer(`({
    rangees: [...document.querySelectorAll("#liste-tunnels .tunnel-rangee")].map((n) => ({
      texte: n.textContent.replace(/\\s+/g, " ").trim(),
      actif: n.classList.contains("actif"),
    })),
    aide: !document.getElementById("txt-tunnel-conf").hidden,
  })`);

/** Le contenu du dossier `tunnels/` : un sous-dossier par configuration, et `actif.txt`. */
const rangees = () => {
  const d = join(profil, "tunnels");
  if (!existsSync(d)) return [];
  return readdirSync(d, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => e.name)
    .sort();
};
const actifNote = () => {
  const f = join(profil, "tunnels", "actif.txt");
  return existsSync(f) ? readFileSync(f, "utf8").trim() : "";
};

try {
  let { rail, barre } = await lancer();
  let page = await ouvrirReglages(barre);

  const avant = await affichage(page);
  console.log("\navant import :", JSON.stringify(avant));
  ok(avant.rangees.length === 0, "aucune configuration au départ");
  ok(avant.aide, "l'aide s'affiche quand il n'y a rien");

  // 1. Import d'une configuration valide.
  await envoyer(page, { m: "tunnelImporter", chemin: conf });
  await pause(2000);
  const apres = await affichage(page);
  console.log("après import :", JSON.stringify(apres));
  ok(apres.rangees.length === 1, "une configuration rangée");
  ok(apres.rangees[0].texte.includes(ENDPOINT), `l'endpoint est affiché (${apres.rangees[0].texte})`);
  ok(apres.rangees[0].texte.includes("10.66.0.2/32"), "l'adresse de l'interface est affichée");
  ok(apres.rangees[0].actif, "elle est active tout de suite");
  ok(!apres.aide, "l'aide disparaît dès qu'il y a une configuration");
  egalite(await rangees(), ["mullvad-fr-01"], "rangée dans tunnels/<id>/");
  ok(actifNote() === "mullvad-fr-01", "actif.txt la désigne");

  // 2. Ce qui est écrit sur le disque.
  const id = "mullvad-fr-01";
  const fichierPublic = join(profil, "tunnels", id, "tunnel.json");
  const fichierCle = join(profil, "tunnels", id, "tunnel.cle");
  const publicTexte = existsSync(fichierPublic) ? readFileSync(fichierPublic, "utf8") : "";
  const cleBrute = existsSync(fichierCle) ? readFileSync(fichierCle) : Buffer.alloc(0);
  console.log(`tunnel.json : ${publicTexte.length} caractères · tunnel.cle : ${cleBrute.length} octets`);
  ok(publicTexte.length > 0, "tunnel.json écrit");
  ok(!publicTexte.includes(CLE_PRIVEE), "tunnel.json ne contient PAS la clé privée");
  ok(publicTexte.includes(ENDPOINT), "tunnel.json garde de quoi reconnaître la configuration");
  ok(cleBrute.length > 0, "tunnel.cle écrit");
  ok(!cleBrute.includes(Buffer.from(CLE_PRIVEE)), "tunnel.cle ne contient pas la clé en clair");
  ok(!cleBrute.toString("latin1").includes(CLE_PRIVEE), "ni en texte lisible");

  // 3. Une SECONDE configuration : deux rangées, la nouvelle devient active.
  await envoyer(page, { m: "tunnelImporter", chemin: confB });
  await pause(2000);
  const deux = await affichage(page);
  console.log("après second import :", JSON.stringify(deux));
  ok(deux.rangees.length === 2, "deux configurations rangées côte à côte");
  const ligneB = deux.rangees.find((r) => r.texte.includes(ENDPOINT_B)) || { actif: false };
  ok(deux.rangees.some((r) => r.texte.includes(ENDPOINT_B)), "la seconde est listée");
  ok(ligneB.actif, "la dernière importée devient active");
  ok(deux.rangees.filter((r) => r.actif).length === 1, "une seule active à la fois");
  egalite(await rangees(), ["mullvad-fr-01", "nord-fr-02"], "chacune a son dossier");

  // 4. « Utiliser » rebascule sur la première.
  await page.evaluer(`document.querySelector('[data-utiliser="mullvad-fr-01"]').click(), true`);
  await pause(1500);
  const bascule = await affichage(page);
  console.log("après bascule :", JSON.stringify(bascule));
  ok((bascule.rangees.find((r) => r.texte.includes(ENDPOINT)) || {}).actif, "la première est redevenue active");
  ok(actifNote() === "mullvad-fr-01", "actif.txt suit le choix");

  // 5. Redémarrage : le choix et la clé chiffrée (DPAPI) survivent.
  tuer();
  ok(await fini(), "premier processus quitté");
  await pause(500);
  ({ rail, barre } = await lancer());
  page = await ouvrirReglages(barre);
  const relance = await affichage(page);
  console.log("après redémarrage :", JSON.stringify(relance));
  ok(relance.rangees.length === 2, "les deux configurations reviennent après redémarrage");
  ok((relance.rangees.find((r) => r.texte.includes(ENDPOINT)) || {}).actif, "celle qui était active est toujours active");

  // 6. Une configuration invalide est refusée, et n'écrase rien.
  await envoyer(page, { m: "tunnelImporter", chemin: confInvalide });
  await pause(2000);
  const apresCasse = await affichage(page);
  ok(apresCasse.rangees.length === 2, "une configuration invalide n'ajoute rien");
  ok(apresCasse.rangees.some((r) => r.texte.includes(ENDPOINT)), "la bonne configuration est intacte");

  // 7. « Supprimer » n'efface que la configuration visée.
  await page.evaluer(`window.confirm = () => true, true`);
  await page.evaluer(`document.querySelector('[data-supprimer="nord-fr-02"]').click(), true`);
  await pause(2000);
  const apresSuppression = await affichage(page);
  console.log("après suppression :", JSON.stringify(apresSuppression));
  ok(apresSuppression.rangees.length === 1, "il n'en reste qu'une");
  ok(!apresSuppression.rangees.some((r) => r.texte.includes(ENDPOINT_B)), "c'est bien celle visée qui est partie");
  ok(!existsSync(join(profil, "tunnels", "nord-fr-02")), "son dossier a disparu");
  ok(existsSync(join(profil, "tunnels", "mullvad-fr-01", "tunnel.cle")), "la clé des autres est intacte");

  // 8. L'ancien rangement (deux fichiers à la racine) est repris au démarrage suivant.
  cpSync(join(profil, "tunnels", "mullvad-fr-01", "tunnel.json"), join(profil, "tunnel.json"));
  cpSync(join(profil, "tunnels", "mullvad-fr-01", "tunnel.cle"), join(profil, "tunnel.cle"));
  tuer();
  ok(await fini(), "processus quitté avant la reprise");
  await pause(500);
  ({ rail, barre } = await lancer());
  page = await ouvrirReglages(barre);
  const repris = await affichage(page);
  console.log("après reprise de l'ancien rangement :", JSON.stringify(repris));
  ok(repris.rangees.length === 2, "l'ancienne configuration est reprise dans le nouveau rangement");
  ok(
    !existsSync(join(profil, "tunnel.json")) && !existsSync(join(profil, "tunnel.cle")),
    "l'ancien emplacement est nettoyé"
  );

  page.fermer();
  barre.fermer();
  rail.fermer();
} catch (e) {
  console.error(`\nECHEC du banc : ${e.message}`);
  echecs += 1;
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

console.log(
  echecs === 0
    ? "\nTunnel intégré — configurations multiples : rangées, basculées, supprimées."
    : `\n${echecs} vérification(s) en échec.`
);
process.exitCode = echecs === 0 ? 0 : 1;
