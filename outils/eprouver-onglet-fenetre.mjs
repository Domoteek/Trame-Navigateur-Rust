#!/usr/bin/env node
/**
 * Banc d'essai des FENÊTRES DEMANDÉES PAR UNE PAGE, sur le vrai binaire.
 *
 * Deux constats de Clément (2026-11-26), la même famille :
 *   - un site qui demande une fenêtre VIDE (`window.open()`, donc `about:blank`) laissait un onglet
 *     blanc qui ne servait à rien ;
 *   - un site qui demande une fenêtre AVEC adresse puis retombe sur la navigation de la page
 *     courante (le motif des moteurs de recherche et des boutiques) donnait DEUX onglets pour la
 *     même page, celui d'origine ayant en plus l'historique.
 *
 * Règle vérifiée ici : une fenêtre demandée par une page ne crée pas d'onglet — elle s'ouvre dans
 * l'onglet d'où vient la demande (et une demande sans adresse ne fait rien). Page de mesure :
 * `outils/banc-onglets.html`, clics réels par CDP.
 *
 * Usage : node outils/eprouver-onglet-fenetre.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT = 19477;
const BINAIRE = process.env.TRAME_BINAIRE || join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-onglet-fenetre-"));
const PAGE = pathToFileURL(resolve("outils/banc-onglets.html")).href;

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

/** Les pages ouvertes (hors surfaces et hors devtools). */
const pages = async () =>
  (await cibles(PORT))
    .filter((t) => t.type === "page" && !t.url.includes("surface=") && !t.url.startsWith("devtools"))
    .map((t) => t.url);

/** Session CDP de la page dont l'adresse contient `contient`. */
async function pageDe(contient, essais = 90) {
  for (let i = 0; i < essais; i++) {
    const c = (await cibles(PORT)).find((t) => t.type === "page" && t.url.includes(contient));
    if (c) {
      const cdp = await connecter(c.webSocketDebuggerUrl);
      await cdp.envoyer("Page.enable");
      await cdp.envoyer("Runtime.enable");
      return cdp;
    }
    await pause(400);
  }
  throw new Error(`aucune page dont l'adresse contient ${contient}`);
}

/** Un vrai clic au centre de l'élément (CDP Input, pas un `.click()` synthétique). */
async function cliquerLaPage(page, selecteur) {
  const point = await page.evaluer(`(() => {
    const el = document.querySelector(${JSON.stringify(selecteur)});
    if (!el) return null;
    el.scrollIntoView({ block: "center" });
    const r = el.getBoundingClientRect();
    return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
  })()`);
  if (!point) throw new Error(`élément absent : ${selecteur}`);
  await page.envoyer("Input.dispatchMouseEvent", { type: "mouseMoved", x: point.x, y: point.y });
  await pause(120);
  for (const type of ["mousePressed", "mouseReleased"]) {
    await page.envoyer("Input.dispatchMouseEvent", { type, x: point.x, y: point.y, button: "left", clickCount: 1 });
    await pause(120);
  }
}

let barre;
/** Retour sur la page de mesure, par la barre d'adresse (comme un utilisateur). */
async function retourAuBanc() {
  await barre.evaluer(`(() => {
    const c = document.getElementById("couture");
    c.focus();
    c.value = ${JSON.stringify(PAGE)};
    c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
  })()`);
  const page = await pageDe("banc-onglets.html");
  await pause(2000);
  return page;
}

try {
  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000);

  const nbOnglets = () => rail.evaluer(`document.querySelectorAll("#fils .fil .onglet").length`);
  const page = await retourAuBanc();
  console.log("pages au départ :", JSON.stringify(await pages()));

  // 1. Fenêtre VIDE : aucun onglet, et la page courante navigue (repli du site).
  const o1 = await nbOnglets();
  await cliquerLaPage(page, "#popup-vide");
  await pause(5000);
  const apres1 = await pages();
  console.log("\n1. window.open() sans adresse");
  console.log("   pages :", JSON.stringify(apres1));
  ok((await nbOnglets()) === o1, `aucun onglet créé (${o1} -> ${await nbOnglets()})`);
  ok(apres1.some((u) => u.includes("?repli")), `la page courante a navigué vers le repli (${JSON.stringify(apres1)})`);

  // 2. Lien target=_blank : la page s'ouvre dans l'onglet courant, sans onglet de plus.
  const page2 = await retourAuBanc();
  const o2 = await nbOnglets();
  await cliquerLaPage(page2, "#lien-blank");
  await pause(5000);
  const apres2 = await pages();
  console.log("\n2. lien target=_blank");
  console.log("   pages :", JSON.stringify(apres2));
  ok((await nbOnglets()) === o2, `aucun onglet de plus (${o2} -> ${await nbOnglets()})`);
  ok(apres2.some((u) => u.includes("?lien-blank")), "la page du lien est ouverte");
  ok(!apres2.some((u) => u.includes("banc-onglets.html")), "la page de mesure a bien laissé sa place");

  // 3. window.open(adresse) : même règle.
  const page3 = await retourAuBanc();
  const o3 = await nbOnglets();
  await cliquerLaPage(page3, "#popup-adresse");
  await pause(5000);
  const apres3 = await pages();
  console.log("\n3. window.open(adresse)");
  console.log("   pages :", JSON.stringify(apres3));
  ok((await nbOnglets()) === o3, `aucun onglet de plus (${o3} -> ${await nbOnglets()})`);
  ok(apres3.some((u) => u.includes("?fenetre-adresse")), "la page demandée est ouverte");

  // 4. Fenêtre AVEC adresse puis repli (le motif Google / boutique) : le doublon constaté.
  const page4 = await retourAuBanc();
  const o4 = await nbOnglets();
  await cliquerLaPage(page4, "#popup-repli");
  await pause(5000);
  const apres4 = await pages();
  console.log("\n4. window.open(adresse) + repli sur la page courante");
  console.log("   pages :", JSON.stringify(apres4));
  ok((await nbOnglets()) === o4, `aucun onglet de plus (${o4} -> ${await nbOnglets()})`);
  ok(
    apres4.filter((u) => u.includes("?fenetre-puis-repli")).length === 1,
    `une seule page pour l'adresse demandée (${apres4.filter((u) => u.includes("?fenetre-puis-repli")).length})`
  );

  // La session écrite ne doit contenir ni onglet blanc ni doublon.
  tuer();
  ok(await fini(), "le processus a quitté");
  const session = JSON.parse(readFileSync(join(profil, "session.json"), "utf8").replace(/^\uFEFF/, ""));
  const url = session.onglets.map((o) => o.url);
  console.log(`\nsession écrite : ${session.onglets.length} onglet(s)`);
  console.log("  " + JSON.stringify(url));
  ok(session.onglets.length === 1, `un seul onglet en session (${session.onglets.length})`);
  ok(!url.some((u) => !u || u === "about:blank"), "aucun onglet blanc");
  ok(!(url.length > 1 && new Set(url).size < url.length), "aucune page en double");

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

console.log(echecs === 0 ? "\nFenêtres demandées : aucun onglet de trop, aucun doublon." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
