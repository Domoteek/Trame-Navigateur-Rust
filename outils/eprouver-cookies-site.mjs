#!/usr/bin/env node
/**
 * Banc d'essai de « Effacer les cookies de ce site » (menu de la page), sur le vrai binaire, en
 * fenêtre cachée (ne vole pas le focus), sans réseau.
 *
 * Demande de Clément (2026-11-28) : refaire son choix de cookies sur AliExpress sans toucher aux
 * autres sites. Deux sites locaux (`*.localhost` = 127.0.0.1 pour Chromium) posent des cookies et
 * du stockage local ; on efface ceux de `essai.localhost` depuis `www.essai.localhost`.
 *
 * Vérifie : cookies du site (hôte ET domaine parent) partis, cookie de l'autre site gardé,
 * stockage local de la page vidé, page rechargée, journal de Trame.
 *
 * Usage : node outils/eprouver-cookies-site.mjs   (TRAME_BINAIRE, TRAME_PORT_ESSAI)
 */
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, existsSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT_CDP = Number(process.env.TRAME_PORT_ESSAI || 19501);
const PORT_HTTP = 19503;
const BINAIRE = process.env.TRAME_BINAIRE || join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-cookies-site-"));
const SITE = `http://www.essai.localhost:${PORT_HTTP}/`;
const AUTRE = `http://autre.localhost:${PORT_HTTP}/`;

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

// Chaque site pose ses cookies par en-tête À SA PREMIÈRE VISITE seulement (le rechargement qui
// suit l'effacement ne doit pas les reposer), et note ses visites dans le stockage local.
const servis = new Set();
const serveur = createServer((req, res) => {
  const hote = (req.headers.host || "").split(":")[0];
  const premiere = !servis.has(hote);
  servis.add(hote);
  const cookies = !premiere ? [] :
    hote === "www.essai.localhost"
      ? ["hote=1; Path=/", "parent=2; Domain=essai.localhost; Path=/", "choix=refus; Path=/; Max-Age=86400"]
      : ["autre=3; Path=/; Max-Age=86400"];
  res.writeHead(200, { "Content-Type": "text/html; charset=utf-8", "Set-Cookie": cookies });
  res.end(`<!doctype html><title>${hote}</title><script>localStorage.setItem("vu", (Number(localStorage.getItem("vu")) || 0) + 1)</script><p>${hote}</p>`);
});
await new Promise((r) => serveur.listen(PORT_HTTP, "127.0.0.1", r));

const enfant = spawn(BINAIRE, [], {
  env: {
    ...process.env,
    TRAME_PROFIL: profil,
    TRAME_CACHEE: "1",
    TRAME_NAV: AUTRE,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP} --remote-debugging-address=127.0.0.1`,
  },
  stdio: "ignore",
});

const noms = (liste) => liste.map((c) => `${c.name}@${c.domain}`).sort();
try {
  const cible = await attendreCible(PORT_CDP, "autre.localhost");
  const page = await connecter(cible.webSocketDebuggerUrl);
  await page.attendre(`document.readyState === "complete"`, { delai: 15000 });
  await page.envoyer("Page.navigate", { url: SITE });
  await page.attendre(`location.hostname === "www.essai.localhost" && document.readyState === "complete"`, { delai: 15000 });
  await pause(1000);
  const avant = (await page.envoyer("Storage.getCookies", {})).cookies;
  console.log(`      avant : ${noms(avant).join(", ")}`);
  ok(avant.length >= 4, "les deux sites ont posé leurs cookies");
  ok((await page.evaluer(`localStorage.getItem("vu")`)) === "1", "stockage local posé");

  const barre = await connecter((await attendreCible(PORT_CDP, "surface=barre")).webSocketDebuggerUrl);
  await barre.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "effacerCookiesSite" })), true`);
  barre.fermer();

  const journal = () => (existsSync(join(profil, "trame.log")) ? readFileSync(join(profil, "trame.log"), "utf8") : "");
  const fin = Date.now() + 15000;
  while (!/cookie\(s\) de essai\.localhost effaces|effacement refuse/.test(journal()) && Date.now() < fin) await pause(300);
  ok(/cookie\(s\) de essai\.localhost effaces/.test(journal()), "Trame annonce l'effacement");
  await pause(1500); // la page est rechargée

  const pagePlus = await connecter((await attendreCible(PORT_CDP, "essai.localhost")).webSocketDebuggerUrl);
  await pagePlus.attendre(`document.readyState === "complete"`, { delai: 15000 });
  ok((await pagePlus.evaluer(`localStorage.getItem("vu")`)) === "1", "stockage local vidé (la page rechargée repart de 1)");
  const vide = (await pagePlus.envoyer("Storage.getCookies", {})).cookies;
  console.log(`      après : ${noms(vide).join(", ")}`);
  ok(!vide.some((c) => /essai\.localhost$/.test(c.domain.replace(/^\./, ""))), "plus aucun cookie de essai.localhost (hôte et domaine parent)");
  ok(vide.some((c) => c.name === "autre"), "autre.localhost garde son cookie");
  pagePlus.fermer();
  page.fermer();
} catch (e) {
  ok(false, e.message);
}

const journal = existsSync(join(profil, "trame.log")) ? readFileSync(join(profil, "trame.log"), "utf8") : "";
for (const l of journal.split("\n").filter((l) => /cookie/i.test(l)).slice(-6)) console.log(`      ${l.trim()}`);
try {
  execFileSync("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
} catch {
  /* deja parti */
}
serveur.close();
await pause(1500);
try {
  rmSync(profil, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
} catch {
  console.log(`      (profil gardé : ${profil})`);
}
console.log(echecs ? `\n${echecs} échec(s)` : "\nTout passe.");
process.exit(echecs ? 1 : 0);
