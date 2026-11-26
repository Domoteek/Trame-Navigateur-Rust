#!/usr/bin/env node
/**
 * Banc d'essai du BOUTON RETOUR à l'ouverture d'un onglet, sur le vrai binaire.
 *
 * Constat de Clément (2026-11-26) : « j'ouvre un raccourci YouTube, je vois déjà la flèche de
 * retour active ; j'ouvre une vidéo, je fais retour → l'accueil YouTube, et encore retour →
 * about:blank ». Cause : la webview naît sur `about:blank` (ordre voulu, voir
 * `creer_onglet_webview`) et la première page était chargée par un `navigate`, donc elle
 * s'ajoutait APRÈS la page blanche au lieu de la remplacer.
 *
 * Vérifie, par l'historique réel de la page (CDP `Page.getNavigationHistory`) :
 *   1. un onglet ouvert sur une adresse n'a QU'UNE entrée, l'adresse elle-même ;
 *   2. le bouton Retour est donc inactif à l'ouverture ;
 *   3. après une deuxième page, retour ramène à la première (jamais sur `about:blank`).
 *
 * Usage : node outils/eprouver-retour-onglet.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT = 19475;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-retour-onglet-"));
const ADRESSE = "https://example.com";

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

const enfant = spawn(BINAIRE, [], {
  env: { ...process.env, TRAME_PROFIL: profil, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
  stdio: "ignore",
});
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

/** La session CDP de la page dont l'adresse contient `contient`. */
async function pageDe(contient) {
  for (let i = 0; i < 90; i++) {
    const c = (await cibles(PORT)).find((t) => t.type === "page" && t.url.includes(contient));
    if (c) {
      const cdp = await connecter(c.webSocketDebuggerUrl);
      await cdp.envoyer("Page.enable");
      return cdp;
    }
    await pause(400);
  }
  throw new Error(`aucune page dont l'adresse contient ${contient}`);
}

/** L'historique reel de la page : entrees et position courante. */
async function historique(cdp) {
  const h = await cdp.envoyer("Page.getNavigationHistory");
  return { entrees: h.entries.map((e) => e.url), position: h.currentIndex };
}

try {
  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  const barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000);

  // Comme Clément : une adresse tapée dans la barre, Alt+Entrée = nouvel onglet du fil.
  const saisir = async (valeur, alt) => {
    await barre.evaluer(`(() => {
      const c = document.getElementById("couture");
      c.focus();
      c.value = ${JSON.stringify(valeur)};
      c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: ${alt ? "true" : "false"}, bubbles: true, cancelable: true }));
      return c.value;
    })()`);
    await pause(600);
  };

  await saisir(ADRESSE, true);
  const page = await pageDe("example.com");
  await pause(2500); // la page finit de charger, l'historique se stabilise

  // 1. Une seule entree : l'adresse demandee.
  const h1 = await historique(page);
  console.log("1. historique :", JSON.stringify(h1));
  ok(
    h1.entrees.length === 1 && h1.entrees[0].startsWith(ADRESSE),
    `une seule entrée, l'adresse ouverte (${JSON.stringify(h1.entrees)})`
  );
  ok(!h1.entrees.some((u) => u === "about:blank"), "aucun about:blank dans l'historique");
  ok(h1.position === 0, `la position courante est la première (${h1.position})`);

  // 2. Le bouton Retour est donc inactif, et la page est VRAIMENT chargée (pas une page inerte :
  //    c'est le piège que `creer_onglet_webview` évite en ne chargeant rien à la création).
  const retour = await barre.evaluer(`document.getElementById("btn-retour").disabled`);
  ok(retour === true, `le bouton Retour est inactif à l'ouverture (disabled = ${retour})`);
  const document = await page.evaluer(`({ pret: document.readyState, titre: document.title })`);
  console.log("2. document :", JSON.stringify(document));
  ok(document.pret === "complete", `la page a fini de charger (${document.pret})`);
  ok(document.titre === "Example Domain", `le document est le vrai (titre « ${document.titre} »)`);

  // 3. Une deuxieme page, puis retour : on revient sur la premiere, pas sur une page blanche.
  await saisir(ADRESSE + "/?deux", false);
  await pause(2500);
  const h2 = await historique(page);
  console.log("3. historique :", JSON.stringify(h2));
  ok(h2.entrees.length === 2, `deux entrées après la deuxième page (${h2.entrees.length})`);
  ok(!h2.entrees.some((u) => u === "about:blank"), "toujours aucun about:blank");
  const actif = await barre.evaluer(`document.getElementById("btn-retour").disabled`);
  ok(actif === false, `le bouton Retour s'active quand il y a quelque chose derrière (disabled = ${actif})`);
  await barre.evaluer(`document.getElementById("btn-retour").click()`);
  await pause(2000);
  const h3 = await historique(page);
  console.log("3. après retour :", JSON.stringify(h3));
  ok(
    h3.entrees[h3.position].startsWith(ADRESSE) && !h3.entrees[h3.position].includes("?deux"),
    `le retour ramène sur la première adresse (${h3.entrees[h3.position]})`
  );
  const apres = await barre.evaluer(`document.getElementById("btn-retour").disabled`);
  ok(apres === true, `revenu à la première page : Retour redevient inactif (disabled = ${apres})`);

  page.fermer();
  barre.fermer();
  rail.fermer();
} finally {
  tuer();
  await fini();
  try {
    rmSync(profil, { recursive: true, force: true });
  } catch {
    /* le ménage de Windows reprendra le dossier */
  }
}

console.log(echecs === 0 ? "\nRetour d'onglet : aucune page blanche derrière." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
