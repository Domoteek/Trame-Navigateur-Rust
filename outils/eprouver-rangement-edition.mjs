#!/usr/bin/env node
/**
 * Banc d'essai de l'ÉDITION du rangement (Réglages → Téléchargements), sur le vrai binaire, caché.
 *
 * Constat de Clément (2026-11-27) : après « Enregistrer » ou Entrée sur un type de fichier, la tuile
 * restait ouverte sur les anciennes valeurs, comme si rien n'était enregistré. Ce banc fait le geste
 * pour de vrai (clics et frappes CDP) dans la vraie page des Réglages, puis vérifie l'écran ET le
 * fichier `reglages.json` du profil :
 *   1. type Images : dossier « Photos », 25 extensions, Entrée ;
 *   2. type Documents : dossier « Papiers », bouton Enregistrer ;
 *   3. nouvelle règle « ameli.fr → Santé », Entrée ;
 *   4. Trame relancé : tout est encore là.
 *
 * Usage : node outils/eprouver-rangement-edition.mjs   (TRAME_BINAIRE / TRAME_PORT_ESSAI au besoin)
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT_CDP = Number(process.env.TRAME_PORT_ESSAI || 19499);
const BINAIRE = process.env.TRAME_BINAIRE || join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-rg-edition-"));

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};
const reglages = () => JSON.parse(readFileSync(join(profil, "reglages.json"), "utf8").replace(/^﻿/, "")).rangement;

let enfant = null;
const lancer = () => {
  enfant = spawn(BINAIRE, [], {
    env: {
      ...process.env,
      TRAME_PROFIL: profil,
      TRAME_CACHEE: "1",
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP} --remote-debugging-address=127.0.0.1`,
    },
    stdio: "ignore",
  });
};
const tuer = async () => {
  try {
    execFileSync("taskkill", ["/F", "/T", "/PID", String(enfant.pid)], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
  await pause(1500);
};

async function ouvrirReglages() {
  const barre = await connecter((await attendreCible(PORT_CDP, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(2500);
  await barre.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "raccourci", action: "reglages" })), true`);
  for (let i = 0; i < 60; i++) {
    const c = (await cibles(PORT_CDP)).find((t) => t.type === "page" && /reglages/.test(t.url) && !t.url.includes("surface="));
    if (c) {
      const p = await connecter(c.webSocketDebuggerUrl);
      await p.envoyer("Runtime.enable");
      await pause(1500);
      // La rubrique Téléchargements, comme un clic dans la colonne.
      await p.evaluer(`(() => { const b = [...document.querySelectorAll("[data-rubrique]")].find((x) => x.dataset.rubrique === "telechargements"); if (b) b.click(); return !!b; })()`);
      await pause(500);
      barre.fermer();
      return p;
    }
    await pause(400);
  }
  throw new Error("page des Réglages introuvable");
}

async function cliquer(p, selecteur) {
  const pt = await p.evaluer(`(() => {
    const el = document.querySelector(${JSON.stringify(selecteur)});
    if (!el) return null;
    el.scrollIntoView({ block: "center" });
    const r = el.getBoundingClientRect();
    return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
  })()`);
  if (!pt) throw new Error(`absent : ${selecteur}`);
  await p.envoyer("Input.dispatchMouseEvent", { type: "mouseMoved", x: pt.x, y: pt.y });
  for (const type of ["mousePressed", "mouseReleased"]) {
    await p.envoyer("Input.dispatchMouseEvent", { type, x: pt.x, y: pt.y, button: "left", clickCount: 1 });
    await pause(80);
  }
  await pause(300);
}
/** Vide le champ et tape le texte, comme au clavier. */
async function taper(p, selecteur, texte) {
  await cliquer(p, selecteur);
  await p.evaluer(`(() => { const el = document.querySelector(${JSON.stringify(selecteur)}); el.focus(); el.select(); })()`);
  await p.envoyer("Input.insertText", { text: texte });
  await pause(150);
}
async function entree(p) {
  for (const type of ["keyDown", "keyUp"]) {
    await p.envoyer("Input.dispatchKeyEvent", { type, key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  }
  await pause(1200);
}
const tuile = (n) => `.rg-cat:nth-child(${n})`;
const vueTuile = (p, n) =>
  p.evaluer(`(() => {
    const t = document.querySelector(${JSON.stringify(tuile(n))});
    return { ouverte: t.querySelectorAll("input").length > 0, dossier: (t.querySelector(".vers code") || {}).textContent || "", plus: (t.querySelector(".rg-puce.plus") || {}).textContent || "" };
  })()`);

try {
  lancer();
  let p = await ouvrirReglages();
  const extensions = Array.from({ length: 25 }, (_, i) => "x" + i).join(", ");

  console.log("1. type Images, Entrée");
  await cliquer(p, `${tuile(1)} .modifier`);
  await taper(p, `${tuile(1)} input:nth-of-type(1)`, "Photos");
  await taper(p, `${tuile(1)} input:nth-of-type(2)`, extensions);
  await entree(p);
  let v = await vueTuile(p, 1);
  ok(!v.ouverte, "la tuile se referme");
  ok(v.dossier === "Photos\\", `l'écran montre le nouveau dossier (${v.dossier})`);
  ok(v.plus === "+21", `l'écran compte les 25 extensions (${v.plus})`);
  let r = reglages();
  ok(r.categories[0].dossier === "Photos", `reglages.json : dossier ${r.categories[0].dossier}`);
  ok(r.categories[0].extensions.length === 25, `reglages.json : ${r.categories[0].extensions.length} extensions`);

  console.log("2. type Vidéos, bouton Enregistrer");
  await cliquer(p, `${tuile(2)} .modifier`);
  await taper(p, `${tuile(2)} input:nth-of-type(1)`, "Films");
  await cliquer(p, `${tuile(2)} button.principal`);
  await pause(900);
  v = await vueTuile(p, 2);
  ok(!v.ouverte, "la tuile se referme");
  ok(v.dossier === "Films\\", `l'écran montre le nouveau dossier (${v.dossier})`);
  ok(reglages().categories[1].dossier === "Films", `reglages.json : dossier ${reglages().categories[1].dossier}`);

  console.log("3. nouvelle règle, Entrée");
  await cliquer(p, "#rg-ajouter");
  await taper(p, ".rg-edition input:nth-of-type(1)", "ameli.fr");
  const quatrieme = await p.evaluer(`(() => { const i = document.querySelectorAll(".rg-edition input")[3]; i.id = "__banc_dossier"; return true; })()`);
  ok(quatrieme, "champ du dossier trouvé");
  await taper(p, "#__banc_dossier", "Santé");
  await entree(p);
  const phrases = await p.evaluer(`[...document.querySelectorAll(".rg-regle .phrase")].map((x) => x.textContent)`);
  ok((await p.evaluer(`document.querySelectorAll(".rg-edition").length`)) === 0, "le formulaire se referme");
  ok(phrases.some((x) => x.includes("ameli.fr") && x.includes("Santé")), `la règle s'affiche (${JSON.stringify(phrases)})`);
  ok(reglages().regles.some((g) => g.sites.includes("ameli.fr") && g.dossier === "Santé"), "reglages.json : règle enregistrée");
  p.fermer();

  console.log("4. après relance");
  await tuer();
  lancer();
  p = await ouvrirReglages();
  ok((await vueTuile(p, 1)).dossier === "Photos\\" && (await vueTuile(p, 2)).dossier === "Films\\", "les deux types gardent leur dossier");
  ok((await p.evaluer(`[...document.querySelectorAll(".rg-regle .phrase")].some((x) => x.textContent.includes("Santé"))`)) === true, "la règle est toujours là");
  p.fermer();
} finally {
  if (enfant) await tuer();
  try {
    rmSync(profil, { recursive: true, force: true });
  } catch {
    /* le ménage de Windows reprendra le dossier */
  }
}

console.log(echecs === 0 ? "\nÉdition du rangement : enregistrée, affichée, et gardée après relance." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
