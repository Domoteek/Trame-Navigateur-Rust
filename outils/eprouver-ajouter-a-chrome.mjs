#!/usr/bin/env node
/**
 * Banc d'essai du bouton « Ajouter à Chrome » de la VRAIE fiche du Chrome Web Store, sur le vrai
 * binaire, en fenêtre cachée (ne vole pas le focus).
 *
 * Constat de Clément (2026-11-28, Joko) : le clic menait WebView2 vers
 * `permanently-removed.invalid/?…x=id%3D<id>…`, téléchargement qui échoue toujours ; le gestionnaire
 * de téléchargements signalait l'échec sans reprendre l'installation par l'identifiant (0.4.100).
 *
 * Vérifie : clic de confiance (CDP Input) sur le bouton → l'extension arrive dans
 * `extensions-chrome\<id>\manifest.json` du profil jetable.
 *
 * Usage : node outils/eprouver-ajouter-a-chrome.mjs [identifiant]   (réseau requis)
 * Binaire : TRAME_BINAIRE, sinon target\release\trame.exe.
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, existsSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const ID = process.argv[2] || "jigflhhckdjdefdjmodlkomnmdonfbbn"; // Joko
const PORT_CDP = Number(process.env.TRAME_PORT_ESSAI || 19493);
const BINAIRE = process.env.TRAME_BINAIRE || join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-ajouter-a-chrome-"));
const FICHE = `https://chromewebstore.google.com/detail/${ID}?hl=fr`;

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

const enfant = spawn(BINAIRE, [], {
  env: {
    ...process.env,
    TRAME_PROFIL: profil,
    TRAME_CACHEE: "1",
    TRAME_NAV: FICHE,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP} --remote-debugging-address=127.0.0.1`,
  },
  stdio: "ignore",
});
const tuer = () => {
  try {
    execFileSync("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
};

const manifeste = join(profil, "extensions-chrome", ID, "manifest.json");
try {
  const cible = await attendreCible(PORT_CDP, "chromewebstore.google.com");
  const page = await connecter(cible.webSocketDebuggerUrl);
  // Le bouton apparaît après le chargement de la fiche (roue d'attente d'abord).
  const trouver = `(() => {
    const b = [...document.querySelectorAll("button")].find((e) => /^(ajouter|add).*chrome$/i.test((e.textContent || "").trim()));
    if (!b) return null;
    if (b.disabled || b.getAttribute("aria-disabled") === "true") return null;
    b.scrollIntoView({ block: "center" });
    const r = b.getBoundingClientRect();
    return r.width > 0 ? { x: r.x + r.width / 2, y: r.y + r.height / 2 } : null;
  })()`;
  // Profil neuf = bandeau de consentement de Google : on refuse (rien de non essentiel).
  const refuser = `(() => {
    const b = [...document.querySelectorAll("button")].find((e) => /^(tout refuser|reject all)$/i.test((e.textContent || "").trim()));
    if (b) b.click();
    return !!b;
  })()`;
  try {
    await page.attendre(`${refuser} || !!${trouver}`, { delai: 20000, quoi: "fiche ou bandeau" });
  } catch {
    /* rien a refuser */
  }
  await pause(3000);
  try {
    await page.attendre(`!!${trouver}`, { delai: 45000, quoi: "bouton « Ajouter à Chrome »" });
  } catch (e) {
    const vus = await page.evaluer(
      `[...document.querySelectorAll("button")].filter((b) => /chrome/i.test(b.textContent || "")).map((b) => ({ t: JSON.stringify(b.textContent), d: b.disabled, a: b.getAttribute("aria-disabled"), w: b.getBoundingClientRect().width }))`
    );
    throw new Error(`${e.message} — boutons vus : ${JSON.stringify(vus)}`);
  }
  await pause(500);
  const { x, y } = await page.evaluer(trouver);
  for (const type of ["mousePressed", "mouseReleased"]) {
    await page.envoyer("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
  }
  ok(true, `clic sur « Ajouter à Chrome » (${Math.round(x)}, ${Math.round(y)})`);
  page.fermer();

  const fin = Date.now() + 60000;
  while (!existsSync(manifeste) && Date.now() < fin) await pause(500);
  ok(existsSync(manifeste), `extension ${ID} installée dans le profil`);
} catch (e) {
  ok(false, e.message);
}

const journal = existsSync(join(profil, "trame.log")) ? readFileSync(join(profil, "trame.log"), "utf8") : "";
for (const l of journal.split("\n").filter((l) => /magasin|telechargement|installation|extension chrome/i.test(l)).slice(-12)) {
  console.log(`      ${l.trim()}`);
}
tuer();
await pause(1500);
try {
  rmSync(profil, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
} catch {
  console.log(`      (profil gardé : ${profil})`);
}
console.log(echecs ? `\n${echecs} échec(s)` : "\nTout passe.");
process.exit(echecs ? 1 : 0);
