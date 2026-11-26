#!/usr/bin/env node
/**
 * Banc d'essai du NOUVEL ONGLET DANS UN GROUPE, sur le vrai binaire.
 *
 * Constat de Clement (2026-11-26) : « quand on fait ajouter en haut, ca cree un groupe et un
 * onglet ». Un onglet neuf n'a aucun contenu, donc le tissage ne pouvait pas le placer et
 * fabriquait un fil de plus. Nouvelle regle : le « + » d'un groupe ouvre dans CE groupe, et le
 * « + » de la tete du rail (comme Ctrl+T) ouvre dans le groupe de l'onglet AFFICHE.
 *
 * Verifie :
 *   1. le « + » d'un groupe ajoute un onglet dans ce groupe, et ne cree aucun fil ;
 *   2. le « + » de la tete du rail fait pareil, dans le groupe affiche ;
 *   3. apres fermeture, la session ECRITE met bien ces onglets dans le meme fil.
 *
 * Usage : node outils/eprouver-nouvel-onglet-groupe.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT = 19473;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-nouveau-groupe-"));

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

try {
  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  await pause(3000); // etat + premier onglet

  // Compte les groupes, et les onglets par groupe (lus dans le rail).
  const structure = () =>
    rail.evaluer(`Array.from(document.querySelectorAll("#fils .fil")).map((f) => ({
      fil: Number(f.dataset.fil),
      onglets: f.querySelectorAll(".onglet").length,
    }))`);

  const avant = await structure();
  console.log("avant :", JSON.stringify(avant));
  ok(avant.length >= 1 && avant[0].onglets >= 1, "un groupe avec au moins un onglet");

  // 1. le « + » du groupe
  await rail.evaluer(`document.querySelector("#fils .fil .ajouter-fil").click()`);
  await pause(1500);
  const apresGroupe = await structure();
  console.log("apres « + » du groupe :", JSON.stringify(apresGroupe));
  ok(apresGroupe.length === avant.length, `aucun groupe cree (${avant.length} -> ${apresGroupe.length})`);
  ok(
    apresGroupe[0].onglets === avant[0].onglets + 1,
    `un onglet de plus DANS le groupe (${avant[0].onglets} -> ${apresGroupe[0].onglets})`
  );

  // 2. le « + » de la tete du rail : dans le groupe de l'onglet affiche (le premier)
  await rail.evaluer(`document.getElementById("btn-nouvel").click()`);
  await pause(1500);
  const apresTete = await structure();
  console.log("apres « + » de la tete :", JSON.stringify(apresTete));
  ok(apresTete.length === apresGroupe.length, `aucun groupe cree non plus (${apresTete.length})`);
  ok(
    apresTete[0].onglets === apresGroupe[0].onglets + 1,
    `le nouvel onglet rejoint le groupe affiche (${apresGroupe[0].onglets} -> ${apresTete[0].onglets})`
  );

  // 3. l'ordre ECRIT garde le meme fil
  tuer();
  ok(await fini(), "le processus a quitte");
  const session = JSON.parse(readFileSync(join(profil, "session.json"), "utf8").replace(/^\uFEFF/, ""));
  const fil = session.fils[0] && session.fils[0].id;
  const dedans = session.onglets.filter((o) => o.fil === fil).length;
  console.log(`ecrit : ${session.onglets.length} onglet(s), ${dedans} dans le fil ${fil}`);
  ok(dedans === apresTete[0].onglets, "la session ecrite met bien ces onglets dans le meme fil");

  rail.fermer();
} finally {
  tuer();
  await fini();
  try {
    rmSync(profil, { recursive: true, force: true });
  } catch {
    /* le menage de Windows reprendra le dossier */
  }
}

console.log(echecs === 0 ? "\nNouvel onglet dans un groupe : tout tient." : `\n${echecs} verification(s) en echec.`);
process.exitCode = echecs === 0 ? 0 : 1;
