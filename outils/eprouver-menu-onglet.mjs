#!/usr/bin/env node
/**
 * Banc d'essai du MENU D'UN ONGLET, sur le vrai binaire : plusieurs clics droits de suite.
 *
 * Le 2026-11-23, Clement a signale qu'apres plusieurs clics droits le menu se dessinait ecrase :
 * fond absent, coupe au milieu des lignes. La mesure a donne la cause : le clic droit vient du
 * RAIL, et la regle « fermer le panneau des qu'on agit ailleurs que dans la barre » le refermait
 * — la barre retombait de 886 a 76 px, puis le menu s'y dessinait dans 76 px.
 *
 * Ce banc ouvre le menu TROIS fois (trois hauteurs de clic differentes) et exige :
 *   1. la barre ne se referme pas entre deux clics (sa hauteur reste celle du menu) ;
 *   2. le menu ne deborde jamais de la webview qui le porte ;
 *   3. le menu reste visible a chaque clic.
 *
 * Usage : node outils/eprouver-menu-onglet.mjs
 * Code de retour : 0 si les trois tiennent, 1 sinon.
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT = Number(process.env.TRAME_PORT_ESSAI || 19462);
// TRAME_BINAIRE : un autre binaire (construit ailleurs quand target\release est verrouille).
const BINAIRE = process.env.TRAME_BINAIRE || join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-menu-onglet-"));

let echecs = 0;
const ok = (condition, quoi) => {
  console.log(`  ${condition ? "ok  " : "ECHEC"} ${quoi}`);
  if (!condition) echecs += 1;
};

const enfant = spawn(BINAIRE, [], {
  env: {
    ...process.env,
    TRAME_PROFIL: profil,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
  },
  stdio: "ignore",
});

try {
  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  const barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");

  const mesurer = () =>
    barre.evaluer(`(() => {
      const zone = document.getElementById("menu-page");
      const col = zone && zone.querySelector(".menu-colonne");
      const r = col ? col.getBoundingClientRect() : null;
      return {
        viewport: window.innerHeight,
        visible: !!(zone && !zone.hidden),
        haut: r ? Math.round(r.height) : 0,
        bas: r ? Math.round(r.bottom) : 0,
      };
    })()`);

  await pause(1500);
  const hauteurs = [];
  for (const y of [600, 650, 300]) {
    await rail.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "menuPage", id: 1, y: ${y} }))`);
    await pause(900);
    const m = await mesurer();
    hauteurs.push(m.viewport);
    ok(m.visible, `clic à y=${y} : le menu est visible`);
    ok(m.haut > 100, `clic à y=${y} : le menu a une hauteur réelle (${m.haut} px)`);
    ok(m.bas <= m.viewport, `clic à y=${y} : le menu tient dans la barre (${m.bas} ≤ ${m.viewport})`);
  }
  // Le coeur du defaut : la barre se refermait entre deux clics (886 -> 76).
  ok(
    hauteurs.every((h) => h === hauteurs[0] && h > 200),
    `la barre garde sa hauteur d'un clic à l'autre (${hauteurs.join(" → ")} px)`
  );

  // ---- fermer puis rouvrir le MEME menu (defaut du 2026-11-23, 0.4.57 chez Clement) : le
  // message d'ouverture sans zone effacait la zone cote hote, et la mesure identique a celle
  // d'avant la fermeture n'etait jamais renvoyee -> barre plafonnee a 72 % de la fenetre, menu
  // coupe net a mi-hauteur.
  for (let essai = 1; essai <= 2; essai += 1) {
    // Comme un clic dans la page : l'hote referme le panneau, l'etat pousse le dit a la barre.
    await barre.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "panneau", open: false }))`);
    await pause(700);
    await rail.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "menuPage", id: 1, y: 300 }))`);
    await pause(1000);
    const m = await mesurer();
    ok(
      m.visible && m.bas <= m.viewport && m.viewport === hauteurs[0],
      `réouverture ${essai} : le menu tient dans la barre (${m.bas} ≤ ${m.viewport}, attendu ${hauteurs[0]})`
    );
  }

  // ---- la hauteur choisie a la poignee : ni un clic ne l'enregistre, ni une valeur perimee
  // ne doit plafonner le menu (defaut du 2026-11-23 : 250 px pour un contenu de 893).
  const lireHauteur = () =>
    barre.evaluer(`(() => { const c = document.querySelector("#menu-page .menu-colonne");
      return { choix: localStorage.getItem("trame.menuPage.hauteur.v2"),
               ancienne: localStorage.getItem("trame.menuPage.hauteur"),
               haut: c ? Math.round(c.getBoundingClientRect().height) : 0 }; })()`);

  // Une valeur sous l'ANCIENNE cle (celle des versions d'avant la 0.4.57) doit etre ignoree.
  await barre.evaluer(`localStorage.setItem("trame.menuPage.hauteur", "250")`);
  await barre.evaluer(`document.getElementById("menu-page").hidden = true`);
  await rail.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "menuPage", id: 1, y: 300 }))`);
  await pause(1000);
  const apresAncienne = await lireHauteur();
  ok(
    apresAncienne.haut > 500,
    `une hauteur enregistrée sous l'ancienne clé est ignorée (menu ${apresAncienne.haut} px)`
  );

  // Un CLIC sur la poignee — sans glisser — ne doit rien enregistrer.
  const pt = await barre.evaluer(`(() => {
    const p = document.querySelector("#menu-page .menu-poignee");
    const r = p.getBoundingClientRect();
    return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + 4) };
  })()`);
  await barre.envoyer("Input.dispatchMouseEvent", { type: "mousePressed", x: pt.x, y: pt.y, button: "left", buttons: 1, clickCount: 1 });
  await barre.envoyer("Input.dispatchMouseEvent", { type: "mouseReleased", x: pt.x, y: pt.y, button: "left", buttons: 0, clickCount: 1 });
  await pause(500);
  const apresClic = await lireHauteur();
  ok(apresClic.choix === null, `un clic sur la poignée n'enregistre rien (valeur : ${apresClic.choix})`);

  // Une vraie traction, elle, doit enregistrer la hauteur choisie.
  await barre.envoyer("Input.dispatchMouseEvent", { type: "mousePressed", x: pt.x, y: pt.y, button: "left", buttons: 1, clickCount: 1 });
  for (let dy = 10; dy <= 120; dy += 10) {
    await barre.envoyer("Input.dispatchMouseEvent", { type: "mouseMoved", x: pt.x, y: pt.y - dy, button: "left", buttons: 1 });
    await pause(30);
  }
  await barre.envoyer("Input.dispatchMouseEvent", { type: "mouseReleased", x: pt.x, y: pt.y - 120, button: "left", buttons: 0, clickCount: 1 });
  await pause(700);
  const apresTraction = await lireHauteur();
  ok(
    apresTraction.choix !== null && apresTraction.haut < 795,
    `une traction de 120 px enregistre la hauteur (${apresTraction.choix} px, menu ${apresTraction.haut} px)`
  );

  rail.fermer();
  barre.fermer();
} finally {
  try {
    // /F : une fenetre cachee (TRAME_CACHEE) ignore la demande polie et restait en vie.
    execFileSync("taskkill", ["/F", "/PID", String(enfant.pid)], { stdio: "ignore" });
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

console.log(echecs === 0 ? "\nMenu d'un onglet : il tient sous les clics repetes." : `\n${echecs} verification(s) en echec.`);
process.exitCode = echecs === 0 ? 0 : 1;
