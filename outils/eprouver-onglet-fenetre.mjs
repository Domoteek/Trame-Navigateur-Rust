#!/usr/bin/env node
/**
 * Banc d'essai des FENÊTRES DEMANDÉES PAR UNE PAGE, sur le vrai binaire.
 *
 * Constat de Clément (2026-11-26) : cliquer un résultat d'une recherche Google créait un onglet de
 * plus, sans besoin. Un site qui demande une fenêtre VIDE (`window.open()`, donc `about:blank`) puis
 * écrit dedans retombe, quand la fenêtre est refusée, sur la navigation de la page courante : Trame
 * créait malgré tout un onglet blanc, qui ne sert à rien.
 *
 * La page de mesure est `outils/banc-onglets.html` (ouverte en `file:`), et les clics sont de VRAIS
 * clics CDP. Vérifie :
 *   1. une fenêtre demandée SANS adresse ne crée aucun onglet, et la page courante navigue ;
 *   2. un lien `target=_blank` crée un onglet, avec SON adresse (comme Chrome) ;
 *   3. `window.open(adresse)` crée un onglet, avec cette adresse.
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
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
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
async function pageDe(contient) {
  for (let i = 0; i < 90; i++) {
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
async function cliquerLespace(page, selecteur) {
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

try {
  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  const barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000);

  // On ouvre la page de mesure dans l'onglet courant, comme une adresse tapée.
  await barre.evaluer(`(() => {
    const c = document.getElementById("couture");
    c.focus();
    c.value = ${JSON.stringify(PAGE)};
    c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
  })()`);
  const page = await pageDe("banc-onglets.html");
  await pause(2500);
  const depart = await pages();
  console.log("pages au départ :", JSON.stringify(depart));

  // 1. Fenêtre VIDE : aucun onglet ne doit naître, et la page courante doit naviguer.
  const avant1 = (await pages()).length;
  await cliquerLespace(page, "#popup-vide");
  await pause(5000);
  const apres1 = await pages();
  console.log("\n1. window.open() sans adresse");
  console.log("   pages :", JSON.stringify(apres1));
  ok(apres1.length === avant1, `aucun onglet créé (${avant1} -> ${apres1.length})`);
  ok(
    apres1.some((u) => u.includes("?repli")),
    `la page courante a navigué vers le repli (${JSON.stringify(apres1)})`
  );

  // On revient sur la page de mesure pour la suite.
  await barre.evaluer(`(() => {
    const c = document.getElementById("couture");
    c.focus();
    c.value = ${JSON.stringify(PAGE)};
    c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
  })()`);
  await pause(2500);
  const page2 = await pageDe("banc-onglets.html");

  // 2. Lien target=_blank : un onglet, avec son adresse (comportement de Chrome).
  const avant2 = (await pages()).length;
  await cliquerLespace(page2, "#lien-blank");
  await pause(5000);
  const apres2 = await pages();
  console.log("\n2. lien target=_blank");
  console.log("   pages :", JSON.stringify(apres2));
  ok(apres2.length === avant2 + 1, `un onglet de plus (${avant2} -> ${apres2.length})`);
  ok(apres2.some((u) => u.includes("?lien-blank")), "l'onglet porte bien l'adresse du lien");

  // 3. window.open(adresse) : un onglet, avec cette adresse.
  const page3 = await pageDe("banc-onglets.html");
  const avant3 = (await pages()).length;
  await cliquerLespace(page3, "#popup-adresse");
  await pause(5000);
  const apres3 = await pages();
  console.log("\n3. window.open(adresse)");
  console.log("   pages :", JSON.stringify(apres3));
  ok(apres3.length === avant3 + 1, `un onglet de plus (${avant3} -> ${apres3.length})`);
  ok(apres3.some((u) => u.includes("?fenetre-adresse")), "l'onglet porte l'adresse demandée");

  // La session écrite ne doit pas contenir d'onglet blanc non plus.
  tuer();
  ok(await fini(), "le processus a quitté");
  const session = JSON.parse(readFileSync(join(profil, "session.json"), "utf8").replace(/^\uFEFF/, ""));
  const blancs = session.onglets.filter((o) => !o.url || o.url === "about:blank");
  console.log(`\nsession écrite : ${session.onglets.length} onglet(s), ${blancs.length} blanc(s)`);
  console.log("  " + JSON.stringify(session.onglets.map((o) => o.url)));
  ok(blancs.length === 0, "aucun onglet blanc écrit dans la session");

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

console.log(echecs === 0 ? "\nFenêtres demandées : aucun onglet de trop." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
