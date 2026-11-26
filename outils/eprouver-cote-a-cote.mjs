#!/usr/bin/env node
/**
 * Banc d'essai du CÔTÉ À CÔTE, sur le vrai binaire.
 *
 * Constat de Clément (2026-11-26) : « dès que je mets un côte à côte ça la met à droite, et si je
 * change, ça remplace celle de droite mais celle de gauche ne se remplace jamais ». La sonde du
 * même jour a montré pourquoi : l'hôte recalculait la paire comme `(onglet actif, nouveau venu)`,
 * donc « gauche » n'était pas un endroit mais « la page où j'étais » — et lire à droite faisait
 * sauter la paire de place.
 *
 * Règle vérifiée ici (`model::placer_cote`) :
 *   1. sans paire, la page neuve va du côté demandé (défaut : à droite) ;
 *   2. avec une paire, la page neuve prend la place de celle qu'on NE lit pas — la page lue ne
 *      change donc jamais de côté ;
 *   3. un côté demandé explicitement l'emporte, et remplacer la page de GAUCHE est possible
 *      (clic droit sur un onglet du rail) ;
 *   4. la session écrite garde la paire.
 *
 * Usage : node outils/eprouver-cote-a-cote.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT = 19474;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-cote-a-cote-"));

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
  const barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000); // etat + premier onglet

  // Les onglets du rail : identifiant, actif, marque « gauche » / « droite ».
  const etat = () =>
    rail.evaluer(`Array.from(document.querySelectorAll("#fils .fil .onglet")).map((el) => {
      const b = el.querySelector(".marque-mini.cote");
      return { id: Number(el.dataset.onglet), actif: el.classList.contains("actif"), cote: b ? b.textContent.trim() : null };
    })`);
  const paire = async () => {
    const e = await etat();
    const g = e.find((o) => o.cote === "gauche");
    const d = e.find((o) => o.cote === "droite");
    return { gauche: g && g.id, droite: d && d.id, actif: (e.find((o) => o.actif) || {}).id, marques: e.filter((o) => o.cote).length };
  };
  const clic = (id) => rail.evaluer(`document.querySelector('.onglet[data-onglet="${id}"]').click()`);
  const clicMaj = (id) =>
    rail.evaluer(`document.querySelector('.onglet[data-onglet="${id}"]').dispatchEvent(
      new MouseEvent("click", { bubbles: true, cancelable: true, view: window, shiftKey: true }))`);
  const clicDroit = (id) =>
    rail.evaluer(`document.querySelector('.onglet[data-onglet="${id}"]').dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, cancelable: true, view: window, clientY: 120 }))`);
  // Le menu de la page est dessine dans la barre ; ses lignes ecoutent `pointerdown`.
  const ligneMenu = (titre) =>
    barre.evaluer(`(() => {
      const b = document.querySelector('#menu-page .menu-principal[title="${titre}"]');
      if (!b) return Array.from(document.querySelectorAll("#menu-page .menu-ligne .n")).map((e) => e.textContent);
      b.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, button: 0 }));
      return "clique";
    })()`);

  // Deux onglets de plus, dans le même fil.
  await rail.evaluer(`document.getElementById("btn-nouvel").click()`);
  await pause(1200);
  await rail.evaluer(`document.getElementById("btn-nouvel").click()`);
  await pause(1500);

  const base = await etat();
  console.log("onglets :", JSON.stringify(base));
  if (base.length < 3) throw new Error(`il faut 3 onglets, ${base.length} trouvé(s)`);
  const [a, b, c] = base.map((o) => o.id);

  // 1. Sans paire : la page affichée reste à gauche, la neuve arrive à droite.
  await clic(a);
  await pause(800);
  await clicMaj(c);
  await pause(1500);
  let p = await paire();
  console.log("1.", JSON.stringify(p));
  ok(p.gauche === a && p.droite === c, `Maj+clic : « ${a} » à gauche, « ${c} » à droite`);

  // 2. Depuis la page de GAUCHE : c'est la droite qui change, la page lue ne bouge pas.
  await clicMaj(b);
  await pause(1500);
  p = await paire();
  console.log("2.", JSON.stringify(p));
  ok(p.gauche === a && p.droite === b, `depuis la gauche : « ${a} » reste, « ${b} » prend la droite`);

  // 3. Depuis la page de DROITE : la gauche change, et la page lue RESTE à droite (le défaut
  //    corrigé : avant, la paire (a, b) devenait (b, c) et la page lue sautait à gauche).
  await clic(b);
  await pause(1200);
  await clicMaj(c);
  await pause(1500);
  p = await paire();
  console.log("3.", JSON.stringify(p));
  ok(p.gauche === c && p.droite === b, `depuis la droite : « ${b} » reste à droite, « ${c} » prend la gauche`);
  ok(p.actif === b, `la page qu'on lisait reste celle qu'on lit (actif = ${p.actif})`);

  // 4. Remplacer la page de DROITE par un onglet hors paire, par le menu du rail.
  await clicDroit(a);
  await pause(900);
  const clique = await ligneMenu("Remplacer la page de droite");
  ok(clique === "clique", `le menu de l'onglet ${a} propose « Remplacer la page de droite » (${JSON.stringify(clique)})`);
  await pause(1500);
  p = await paire();
  console.log("4.", JSON.stringify(p));
  ok(p.gauche === c && p.droite === a, `« ${a} » remplace la page de droite : (${c}, ${a})`);
  ok(p.actif === a, `la page arrivée devient celle qu'on lit (actif = ${p.actif})`);

  // 5. La session écrite garde la paire.
  tuer();
  ok(await fini(), "le processus a quitté");
  const session = JSON.parse(readFileSync(join(profil, "session.json"), "utf8").replace(/^\uFEFF/, ""));
  console.log(`écrit : cote = ${JSON.stringify(session.cote)}, ratio = ${session.ratio_cote}`);
  ok(
    Array.isArray(session.cote) && session.cote[0] === c && session.cote[1] === a,
    "la session écrite garde la paire (gauche, droite)"
  );
  // `ratio_cote` vaut 0 dans une session neuve : la lecture le borne a 0,5 (moitie). Ce qui compte
  // est qu'il soit ECRIT, pour que la largeur choisie survive au redemarrage.
  ok(typeof session.ratio_cote === "number", "la largeur des deux pages est écrite (0 = moitié par défaut)");

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

console.log(echecs === 0 ? "\nCôte à côte : la destination est choisie." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
