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

const PORT = 19462;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
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

  rail.fermer();
  barre.fermer();
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

console.log(echecs === 0 ? "\nMenu d'un onglet : il tient sous les clics repetes." : `\n${echecs} verification(s) en echec.`);
process.exitCode = echecs === 0 ? 0 : 1;
