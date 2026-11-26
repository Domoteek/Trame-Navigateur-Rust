#!/usr/bin/env node
/**
 * SONDE du CÔTE À CÔTE, sur le vrai binaire — pas un banc de non-régression, une mesure.
 *
 * Question de Clément (2026-11-26) : « dès que je mets un côte à côte ça la met à droite, et si je
 * change, ça remplace celle de droite mais celle de gauche ne se remplace jamais ».
 *
 * On mesure, sans rien changer au produit :
 *   1. après un Maj+clic, quel onglet porte la marque « gauche » et quel onglet « droite » ;
 *   2. ce que fait un clic simple sur un TROISIÈME onglet pendant le côte à côte ;
 *   3. ce que fait un nouveau Maj+clic quand la page active est celle de GAUCHE, puis quand c'est
 *      celle de DROITE — c'est là que se joue « la gauche ne se remplace jamais ».
 *
 * Usage : node outils/sonder-cote-a-cote.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT = 19474;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-sonde-cote-"));

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

try {
  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  await pause(3000);

  // Les onglets du rail : identifiant, actif, et la marque « gauche » / « droite ».
  const etat = () =>
    rail.evaluer(`Array.from(document.querySelectorAll("#fils .fil .onglet")).map((el) => {
      const b = el.querySelector(".marque-mini.cote");
      return {
        id: Number(el.dataset.onglet),
        actif: el.classList.contains("actif"),
        cote: b ? b.textContent.trim() : null,
      };
    })`);

  // Le menu de la page (barre) : « Inverser » / « Quitter » n'y figurent que si la paire est
  // reellement AFFICHEE (`carnet.cote.affiche`). C'est la seule facon de mesurer cet etat.
  const barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  const menuPage = async () => {
    await barre.evaluer(`document.getElementById("menu-page").hidden ? document.getElementById("btn-page").click() : 0`);
    await pause(700);
    // Tout le texte du menu : les entrées du cote a cote sont rendues par `puces`, pas par
    // `lignePage` — un selecteur sur `.menu-ligne .n` les manquait (erreur de sonde, pas de produit).
    const texte = await barre.evaluer(`document.getElementById("menu-page").textContent.replace(/\\s+/g, " ").trim()`);
    await barre.evaluer(`document.getElementById("btn-page").click()`);
    await pause(400);
    return { inverser: texte.includes("Inverser"), quitter: texte.includes("Quitter"), aCote: texte.includes("À côté de") };
  };

  const clic = (id) => rail.evaluer(`document.querySelector('.onglet[data-onglet="' + ${id} + '"]').click()`);
  const clicMaj = (id) =>
    rail.evaluer(`document.querySelector('.onglet[data-onglet="' + ${id} + '"]').dispatchEvent(
      new MouseEvent("click", { bubbles: true, cancelable: true, view: window, shiftKey: true }))`);

  // Deux onglets de plus, dans le meme fil (le « + » de la tete du rail).
  await rail.evaluer(`document.getElementById("btn-nouvel").click()`);
  await pause(1200);
  await rail.evaluer(`document.getElementById("btn-nouvel").click()`);
  await pause(1500);

  const base = await etat();
  console.log("onglets :", JSON.stringify(base));
  if (base.length < 3) throw new Error(`il faut 3 onglets, ${base.length} trouve(s)`);

  const [a, b, c] = base.map((o) => o.id);

  // 1. côte à côte : a affiché, puis Maj+clic sur c.
  await clic(a);
  await pause(800);
  await clicMaj(c);
  await pause(1500);
  console.log("\n1. Maj+clic sur", c, "depuis", a, ":", JSON.stringify(await etat()));
  console.log("   menu de la page :", JSON.stringify(await menuPage()));

  // 2. clic simple sur un TROISIEME onglet pendant le côte à côte.
  await clic(b);
  await pause(1500);
  console.log("2. clic simple sur", b, "pendant le côte à côte :", JSON.stringify(await etat()));
  console.log("   menu de la page :", JSON.stringify(await menuPage()));

  // 3a. on revient sur la page de GAUCHE du côte à côte, puis Maj+clic sur b.
  await clic(a);
  await pause(1200);
  console.log("3a. retour sur la page de gauche", a, ":", JSON.stringify(await etat()));
  await clicMaj(b);
  await pause(1500);
  const apresGauche = await etat();
  console.log("3a. Maj+clic sur", b, "depuis la GAUCHE :", JSON.stringify(apresGauche));

  // 3b. on clique la page de DROITE, puis Maj+clic sur un autre onglet libre.
  const droite = (apresGauche.find((o) => o.cote === "droite") || {}).id;
  const libre = base.map((o) => o.id).find((id) => id !== droite && id !== apresGauche.find((o) => o.cote === "gauche")?.id);
  await clic(droite);
  await pause(1200);
  console.log("3b. clic sur la page de DROITE", droite, ":", JSON.stringify(await etat()));
  await clicMaj(libre);
  await pause(1500);
  console.log("3b. Maj+clic sur", libre, "depuis la DROITE :", JSON.stringify(await etat()));

  barre.fermer();
  rail.fermer();
} finally {
  tuer();
  await pause(1200);
  try {
    rmSync(profil, { recursive: true, force: true });
  } catch {
    /* le menage de Windows reprendra le dossier */
  }
}
