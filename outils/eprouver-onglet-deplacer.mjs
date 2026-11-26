#!/usr/bin/env node
/**
 * Banc d'essai du DEPLACEMENT D'UN ONGLET dans son fil, sur le vrai binaire.
 *
 * Demande de Clement (2026-11-26) : « je peux deplacer les groupes, il faudrait aussi pouvoir
 * deplacer les onglets dans les groupes ». L'ordre du rail est celui de `session.onglets` : ce banc
 * verifie les trois maillons — le message deplace l'onglet, le rail le montre, et l'ordre ECRIT
 * (donc relu au demarrage) le garde.
 *
 * Usage : node outils/eprouver-onglet-deplacer.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT = 19472;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-deplacer-"));

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

const enfant = spawn(BINAIRE, [], {
  env: { ...process.env, TRAME_PROFIL: profil, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
  stdio: "ignore",
});

const fermer = () => {
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
  await pause(2500); // laisser l'etat arriver et le premier onglet naitre

  // Trois onglets de plus, donc quatre dans le meme fil.
  for (let i = 0; i < 3; i++) {
    await rail.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "nouvelOnglet" }))`);
    await pause(700);
  }
  const ordreRail = () =>
    rail.evaluer(`Array.from(document.querySelectorAll("#fils .onglet")).map((n) => Number(n.dataset.onglet))`);
  const avant = await ordreRail();
  console.log("ordre du rail avant :", JSON.stringify(avant));
  ok(avant.length >= 3, `au moins trois onglets dans le rail (${avant.length})`);

  // Le dernier passe devant le premier, dans son fil.
  const dernier = avant[avant.length - 1];
  const premier = avant[0];
  const fil = await rail.evaluer(`(() => {
    const bloc = document.querySelector("#fils .fil");
    return bloc ? Number(bloc.dataset.fil) : 1;
  })()`);
  await rail.evaluer(
    `window.ipc.postMessage(JSON.stringify({ m: "ongletDeplacer", id: ${dernier}, avant: ${premier}, fil: ${fil} }))`
  );
  await pause(1200);
  const apres = await ordreRail();
  console.log("ordre du rail apres :", JSON.stringify(apres));
  ok(apres[0] === dernier, `l'onglet ${dernier} est passe devant ${premier} dans le rail`);
  ok(apres.length === avant.length, "aucun onglet perdu au passage");

  // Et l'ordre ECRIT : on ferme proprement, la session s'ecrit, on la relit.
  fermer();
  ok(await fini(), "le processus a quitte");
  const session = JSON.parse(readFileSync(join(profil, "session.json"), "utf8").replace(/^\uFEFF/, ""));
  const ecrit = session.onglets.map((o) => o.id).filter((id) => avant.includes(id));
  console.log("ordre ecrit         :", JSON.stringify(ecrit));
  ok(ecrit[0] === dernier, "l'ordre ecrit commence par l'onglet deplace (il sera relu au demarrage)");

  rail.fermer();
} finally {
  fermer();
  await fini();
  try {
    rmSync(profil, { recursive: true, force: true });
  } catch {
    /* le menage de Windows reprendra le dossier */
  }
}

console.log(echecs === 0 ? "\nDeplacement d'onglet : tout tient." : `\n${echecs} verification(s) en echec.`);
process.exitCode = echecs === 0 ? 0 : 1;
