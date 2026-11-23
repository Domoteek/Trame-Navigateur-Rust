#!/usr/bin/env node
/**
 * Banc d'essai des ENTREES d'interface, sur le vrai binaire : le clic part-il, et l'hote
 * le traite-t-il ?
 *
 * Ce banc existe a cause d'un defaut reel (2026-11-23) : les messages `prive` et
 * `sauvegardeAuto*` etaient ecrits dans une fonction que seules les commandes de telechargement
 * atteignent. L'interface les envoyait donc parfaitement — et l'hote les ignorait EN SILENCE.
 * Les tests d'interface ne voyaient rien, puisqu'ils ne regardent que l'ENVOI. Ici, on clique
 * dans le vrai binaire et on regarde l'EFFET (un fil prive, un onglet, un reglage ecrit).
 *
 * Verifie :
 *   1. le bouton « Privé » du pied du rail cree le fil prive ;
 *   2. l'entree « Navigation privée » du Nouvel onglet ouvre un onglet ;
 *   3. l'interrupteur et l'intervalle de la sauvegarde automatique s'ecrivent dans reglages.json.
 *
 * Usage : node outils/eprouver-entrees.mjs
 * Code de retour : 0 si les trois repondent, 1 sinon.
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT = 19453;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-entrees-"));
const journal = join(profil, "trame.log");
const fichierReglages = join(profil, "reglages.json");

let echecs = 0;
const ok = (condition, quoi) => {
  console.log(`  ${condition ? "ok  " : "ECHEC"} ${quoi}`);
  if (!condition) echecs += 1;
};

const lireJournal = () => {
  try {
    return readFileSync(journal, "utf8");
  } catch {
    return "";
  }
};
const filsPrives = () => (lireJournal().match(/navigation privee : fil/g) || []).length;
const lireReglages = () => {
  try {
    return JSON.parse(readFileSync(fichierReglages, "utf8"));
  } catch {
    return {};
  }
};
const ongletsOuverts = async () =>
  (await cibles(PORT)).filter((t) => t.type === "page" && t.url.includes("newtab.html")).length;

if (!lireJournal && !BINAIRE) {
  console.error(`binaire introuvable : ${BINAIRE} (lancer : cargo build --release)`);
  process.exit(1);
}

const enfant = spawn(BINAIRE, [], {
  env: {
    ...process.env,
    TRAME_PROFIL: profil,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
  },
  stdio: "ignore",
});

try {
  // ---- 1. le bouton du pied du rail
  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  const avant = filsPrives();
  await rail.evaluer(`document.getElementById("btn-prive").click()`);
  await pause(1500);
  ok(filsPrives() > avant, "le bouton « Privé » du pied du rail ouvre le fil privé");

  // ---- 2. l'entree du Nouvel onglet (on compte les onglets : le fil privé est REUTILISE)
  let cible = null;
  for (let i = 0; i < 60 && !cible; i++) {
    cible = (await cibles(PORT)).find((t) => t.type === "page" && t.url.includes("newtab.html"));
    if (!cible) await pause(400);
  }
  const page = await connecter(cible.webSocketDebuggerUrl);
  await page.envoyer("Runtime.enable");
  const avantOnglets = await ongletsOuverts();
  await page.evaluer(`document.getElementById("btn-prive-newtab").click()`);
  await pause(2000);
  ok((await ongletsOuverts()) > avantOnglets, "l'entree du Nouvel onglet ouvre un onglet privé");

  // ---- 3. les reglages de la sauvegarde automatique
  let reglages = null;
  for (let i = 0; i < 40 && !reglages; i++) {
    reglages = (await cibles(PORT)).find((t) => t.type === "page" && t.url.includes("reglages.html"));
    if (!reglages) {
      await rail.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "ouvrirReglages" }))`);
      await pause(500);
    }
  }
  const rgl = await connecter(reglages.webSocketDebuggerUrl);
  await rgl.envoyer("Runtime.enable");
  await pause(1200);
  const avantReg = lireReglages();
  await rgl.evaluer(`document.getElementById("bascule-auto").click()`);
  await rgl.evaluer(
    `(() => { const s = document.getElementById("auto-heures"); s.value = "3"; s.dispatchEvent(new Event("change")); })()`
  );
  await pause(1500);
  const apresReg = lireReglages();
  ok(
    apresReg.sauvegarde_auto === !avantReg.sauvegarde_auto,
    `l'interrupteur de la sauvegarde automatique s'écrit (${avantReg.sauvegarde_auto} -> ${apresReg.sauvegarde_auto})`
  );
  ok(apresReg.sauvegarde_heures === 3, `l'intervalle choisi s'écrit (${apresReg.sauvegarde_heures} h)`);

  rail.fermer();
  page.fermer();
  rgl.fermer();
} finally {
  try {
    execFileSync("taskkill", ["/PID", String(enfant.pid)], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
  await pause(800);
  try {
    rmSync(profil, { recursive: true, force: true });
  } catch {
    /* le menage de Windows reprendra le dossier d'essai */
  }
}

console.log(echecs === 0 ? "\nEntrees d'interface : tout repond." : `\n${echecs} verification(s) en echec.`);
process.exitCode = echecs === 0 ? 0 : 1;
