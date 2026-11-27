#!/usr/bin/env node
/**
 * Banc d'essai des LIENS VERS UN AUTRE SITE, sur le vrai binaire (choix de Clément, 2026-11-27).
 *
 * Règle vérifiée :
 *   - un lien du même site s'ouvre dans l'onglet (aucun onglet de plus) ;
 *   - un lien vers un autre site ouvre UN nouvel onglet, juste après celui d'origine, qui reste
 *     sur sa page — y compris quand le site fait `window.open(...) || location.href = ...` (le
 *     motif qui faisait deux pages avant 0.4.72) ;
 *   - fermer l'onglet ouvert à côté ramène à l'onglet d'origine.
 *
 * Deux « sites » sans sortir de la machine : un petit serveur sur 127.0.0.1, joint aussi par
 * `localhost` — pour Trame, ce sont deux domaines différents.
 *
 * Usage : node outils/eprouver-liens-a-cote.mjs   (TRAME_BINAIRE=… pour un autre binaire)
 */
import http from "node:http";
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT_CDP = Number(process.env.TRAME_PORT_ESSAI || 19478);
const BINAIRE = process.env.TRAME_BINAIRE || join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-liens-a-cote-"));

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

// ---------------------------------------------------------------- les deux sites
const serveur = http.createServer((req, res) => {
  const u = new URL(req.url, "http://x");
  const nom = u.pathname.slice(1) || "banc";
  res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
  if (nom !== "banc") {
    res.end(`<!doctype html><title>page ${nom}</title><p>${nom}</p>`);
    return;
  }
  const port = serveur.address().port;
  res.end(`<!doctype html><title>banc liens</title>
<style>a{display:block;margin:16px;padding:12px;font:16px sans-serif}</style>
<a id="meme" href="http://127.0.0.1:${port}/meme-site">même site</a>
<a id="autre" href="http://localhost:${port}/autre-site">autre site</a>
<a id="repli" href="http://localhost:${port}/autre-repli" target="_blank">autre site, le site ouvre lui-même</a>
<script>
  // Le motif des moteurs et des boutiques : demander une fenêtre, et naviguer si elle est refusée.
  document.getElementById("repli").addEventListener("click", (e) => {
    e.preventDefault();
    const w = window.open(e.currentTarget.href);
    if (!w) location.href = e.currentTarget.href;
  });
</script>`);
});
await new Promise((r) => serveur.listen(0, "127.0.0.1", r));
const BANC = `http://127.0.0.1:${serveur.address().port}/banc`;

// ---------------------------------------------------------------- Trame, caché
const enfant = spawn(BINAIRE, [], {
  env: {
    ...process.env,
    TRAME_PROFIL: profil,
    TRAME_CACHEE: "1",
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP}`,
  },
  stdio: "ignore",
});
// Un Trame caché ignore `taskkill` sans /F : il resterait vivant avec son port de débogage.
const tuer = () => {
  try {
    execFileSync("taskkill", ["/F", "/PID", String(enfant.pid)], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
};

const pages = async () =>
  (await cibles(PORT_CDP))
    .filter((t) => t.type === "page" && !t.url.includes("surface=") && !t.url.startsWith("devtools"))
    .map((t) => t.url);

async function pageDe(contient, essais = 90) {
  for (let i = 0; i < essais; i++) {
    const c = (await cibles(PORT_CDP)).find((t) => t.type === "page" && t.url.includes(contient));
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

/** Un vrai clic (CDP Input), pas un `.click()` synthétique. */
async function cliquer(page, selecteur) {
  const point = await page.evaluer(`(() => {
    const el = document.querySelector(${JSON.stringify(selecteur)});
    if (!el) return null;
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

let barre, rail;
async function allerAuBanc() {
  await barre.evaluer(`(() => {
    const c = document.getElementById("couture");
    c.focus();
    c.value = ${JSON.stringify(BANC)};
    c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
  })()`);
  const page = await pageDe("/banc");
  await pause(1500);
  return page;
}
const nbOnglets = () => rail.evaluer(`document.querySelectorAll("#fils .fil .onglet").length`);
const titresRail = () => rail.evaluer(`[...document.querySelectorAll("#fils .fil .onglet")].map((o) => (o.classList.contains("actif") ? "*" : "") + o.textContent.trim())`);

try {
  rail = await connecter((await attendreCible(PORT_CDP, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  barre = await connecter((await attendreCible(PORT_CDP, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000);

  // 1. Même site : dans l'onglet.
  let banc = await allerAuBanc();
  const o1 = await nbOnglets();
  await cliquer(banc, "#meme");
  await pause(3000);
  const apres1 = await pages();
  console.log("\n1. lien du même site");
  console.log("   pages :", JSON.stringify(apres1));
  ok((await nbOnglets()) === o1, `aucun onglet de plus (${o1} -> ${await nbOnglets()})`);
  ok(apres1.some((u) => u.includes("/meme-site")) && !apres1.some((u) => u.endsWith("/banc")), "la page a navigué dans l'onglet");

  // 2. Autre site : un onglet à côté, l'origine reste.
  banc = await allerAuBanc();
  const o2 = await nbOnglets();
  await cliquer(banc, "#autre");
  await pause(3500);
  const apres2 = await pages();
  console.log("\n2. lien vers un autre site");
  console.log("   pages :", JSON.stringify(apres2), " rail :", JSON.stringify(await titresRail()));
  ok((await nbOnglets()) === o2 + 1, `un onglet de plus (${o2} -> ${await nbOnglets()})`);
  ok(apres2.some((u) => u.endsWith("/banc")), "l'onglet d'origine garde sa page");
  ok(apres2.filter((u) => u.includes("/autre-site")).length === 1, "une seule page pour le lien");
  const rail2 = await titresRail();
  const iBanc = rail2.findIndex((t) => t.includes("banc liens"));
  ok(iBanc >= 0 && rail2[iBanc + 1] && rail2[iBanc + 1].startsWith("*") && rail2[iBanc + 1].includes("autre-site"), `le nouvel onglet est juste après l'origine, et affiché (${JSON.stringify(rail2)})`);

  // 3. Fermer l'onglet ouvert à côté ramène à l'origine.
  const nouvelle = await pageDe("/autre-site");
  await nouvelle.envoyer("Input.dispatchKeyEvent", { type: "keyDown", key: "w", code: "KeyW", windowsVirtualKeyCode: 87, modifiers: 2 });
  await nouvelle.envoyer("Input.dispatchKeyEvent", { type: "keyUp", key: "w", code: "KeyW", windowsVirtualKeyCode: 87, modifiers: 2 });
  await pause(2000);
  const rail3 = await titresRail();
  console.log("\n3. fermer l'onglet ouvert à côté");
  console.log("   rail :", JSON.stringify(rail3));
  ok(rail3.some((t) => t.startsWith("*") && t.includes("banc liens")), "l'onglet d'origine est de nouveau affiché");

  // 4. Le site ouvre lui-même (window.open puis repli) : toujours un seul onglet de plus.
  banc = await pageDe("/banc");
  const o4 = await nbOnglets();
  await cliquer(banc, "#repli");
  await pause(3500);
  const apres4 = await pages();
  console.log("\n4. autre site, le site fait window.open puis repli");
  console.log("   pages :", JSON.stringify(apres4));
  ok((await nbOnglets()) === o4 + 1, `un seul onglet de plus (${o4} -> ${await nbOnglets()})`);
  ok(apres4.filter((u) => u.includes("/autre-repli")).length === 1, "une seule page pour le lien, pas de doublon");
  ok(apres4.some((u) => u.endsWith("/banc")), "l'onglet d'origine garde sa page");

  // La session écrite : l'onglet ouvert à côté connaît son origine.
  tuer();
  await pause(1500);
  const session = JSON.parse(readFileSync(join(profil, "session.json"), "utf8").replace(/^﻿/, ""));
  console.log("\nsession :", JSON.stringify(session.onglets.map((o) => [o.id, o.url, o.parent ?? null])));
  const origine = session.onglets.find((o) => o.url.endsWith("/banc"));
  const fille = session.onglets.find((o) => o.url.includes("/autre-repli"));
  ok(!!origine && !!fille && fille.parent === origine.id && fille.fil === origine.fil, "l'onglet ouvert à côté est rangé dans le fil de son origine");
} finally {
  tuer();
  serveur.close();
  await pause(800);
  try {
    rmSync(profil, { recursive: true, force: true });
  } catch {
    /* le ménage de Windows reprendra le dossier */
  }
}

console.log(echecs === 0 ? "\nLiens vers un autre site : à côté, sans doublon, et retour à l'origine." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
