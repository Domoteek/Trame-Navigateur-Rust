#!/usr/bin/env node
/**
 * SONDE — cliquer un résultat de recherche : combien d'onglets, et lesquels.
 *
 * Constat de Clément (2026-11-26) : sur une recherche Google, cliquer un résultat crée un onglet
 * de plus (qui garde la recherche) et envoie l'onglet d'origine sur la bonne page.
 *
 * Cette sonde MESURE, sur le vrai binaire :
 *   1. ce que fait `window.open(<adresse>)` dans une page (le chemin « nouvelle fenêtre ») ;
 *   2. ce que fait `window.open()` sans adresse ;
 *   3. ce que fait un VRAI clic (CDP Input) sur le premier résultat de la recherche.
 * Pour chaque cas : les pages réellement ouvertes (cibles CDP), les onglets du rail, et les lignes
 * de journal qui parlent de nouvelle fenêtre.
 *
 * Usage : node outils/sonder-google-onglet.mjs [url-de-recherche]
 *
 * LIMITE CONSTATÉE (2026-11-26) : le clic synthétique sur un résultat Google n'aboutit pas — les
 * résultats sont couverts par une couche maison (`div.jw8mI`) qui avale les clics CDP. Les cas 1 et
 * 2 (`window.open`) mesurent donc le MÉCANISME, et c'est `outils/eprouver-onglet-fenetre.mjs`, sur
 * une page de mesure locale, qui vérifie le comportement attendu.
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT = 19476;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-sonde-google-"));
const RECHERCHE = process.argv[2] || "https://www.google.com/search?q=qr+code&hl=fr";

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

/** Les pages ouvertes (hors surfaces), telles que le moteur les voit. */
const pages = async () =>
  (await cibles(PORT))
    .filter((t) => t.type === "page" && !t.url.includes("surface=") && !t.url.startsWith("devtools"))
    .map((t) => t.url);

let rail;
const onglets = () =>
  rail.evaluer(`Array.from(document.querySelectorAll("#fils .fil .onglet")).map((el) => ({
    id: Number(el.dataset.onglet),
    actif: el.classList.contains("actif"),
    titre: (el.querySelector(".titre") || {}).textContent || "",
    domaine: (el.querySelector(".domaine") || {}).textContent || "",
  }))`);

const journal = () => {
  try {
    return readFileSync(join(profil, "trame.log"), "utf8")
      .split(/\r?\n/)
      .filter((l) => /fenetre|onglet|navigation/i.test(l))
      .slice(-12);
  } catch {
    return ["(pas de journal)"];
  }
};

try {
  rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  const barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000);

  // On tape la recherche dans la barre comme un utilisateur (Entrée simple : même onglet).
  await barre.evaluer(`(() => {
    const c = document.getElementById("couture");
    c.focus();
    c.value = ${JSON.stringify(RECHERCHE)};
    c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
  })()`);
  await pause(6000);
  const cible = (await cibles(PORT)).find((t) => t.type === "page" && /google\.|duckduckgo\./.test(t.url));
  if (!cible) throw new Error(`pas de page de recherche : ${JSON.stringify(await pages())}`);
  console.log("page de recherche :", cible.url);
  const page = await connecter(cible.webSocketDebuggerUrl);
  await page.envoyer("Runtime.enable");
  await page.envoyer("Page.enable");
  const titre = await page.evaluer("document.title");
  const liens = await page.evaluer(
    `Array.from(document.querySelectorAll("a[href^='http']")).filter((a) => a.getBoundingClientRect().width > 40).length`
  );
  console.log(`titre : « ${titre} », ${liens} lien(s) visibles`);
  console.log("onglets avant :", JSON.stringify(await onglets()));
  console.log("pages avant   :", JSON.stringify(await pages()));

  // 1. window.open(<adresse>) depuis la page.
  await page.evaluer(`window.open("https://example.com/?popup-adresse")`);
  await pause(4000);
  console.log("\n1. window.open(adresse)");
  console.log("   onglets :", JSON.stringify(await onglets()));
  console.log("   pages   :", JSON.stringify(await pages()));

  // 2. window.open() sans adresse.
  await page.evaluer(`window.open()`);
  await pause(4000);
  console.log("\n2. window.open() sans adresse");
  console.log("   onglets :", JSON.stringify(await onglets()));
  console.log("   pages   :", JSON.stringify(await pages()));

  // 3. Un VRAI clic sur le premier résultat (CDP Input : le plus fidèle au geste).
  const point = await page.evaluer(`(() => {
    const liens = Array.from(document.querySelectorAll("a[href]")).filter((x) => {
      const r = x.getBoundingClientRect();
      const t = (x.textContent || "").trim();
      return r.width > 120 && r.height > 12 && r.top > 0 && t.length > 8;
    });
    // Les résultats de Google passent par une redirection google.com/goto?url=… (ou /url?q=…) :
    // ce sont de vrais résultats, ne pas les exclure parce que l'hôte est google.com.
    const a =
      liens.find((x) => /\\/goto\\?url=|\\/url\\?q=/.test(x.href)) ||
      liens.find((x) => !x.href.includes("policies.google") && !x.href.includes("google.com/intl"));
    if (!a) return { aucun: liens.slice(0, 5).map((x) => ({ href: x.href, texte: (x.textContent || "").slice(0, 30) })) };
    a.scrollIntoView({ block: "center" });
    // Le point de clic : le milieu du TEXTE de l'ancre (une ancre de résultat englobe souvent une
    // icône et des marges ; viser la boîte de l'ancre tombait à côté).
    const r = a.getBoundingClientRect();
    let x = Math.round(r.left + Math.min(60, r.width / 2));
    let y = Math.round(r.top + r.height / 2);
    const texte = a.querySelector("h3") || a;
    const rt = texte.getBoundingClientRect();
    if (rt.width > 20) {
      x = Math.round(rt.left + Math.min(40, rt.width / 2));
      y = Math.round(rt.top + rt.height / 2);
    }
    const dessus = document.elementFromPoint(x, y);
    return {
      x,
      y,
      href: a.href.slice(0, 80),
      texte: (a.textContent || "").trim().slice(0, 50),
      cible: a.target,
      rel: a.rel,
      dessus: dessus ? (dessus.tagName + "." + (dessus.className || "").toString().slice(0, 40)) : null,
      dansAncre: !!(dessus && (a.contains(dessus) || dessus.closest("a") === a)),
    };
  })()`);
  console.log("\n3. résultat visé :", JSON.stringify(point));
  if (point && point.x) {
    const avant = (await onglets()).length;
    await page.envoyer("Input.dispatchMouseEvent", { type: "mouseMoved", x: point.x, y: point.y });
    await pause(150);
    for (const type of ["mousePressed", "mouseReleased"]) {
      await page.envoyer("Input.dispatchMouseEvent", { type, x: point.x, y: point.y, button: "left", clickCount: 1 });
      await pause(120);
    }
    await pause(5000);
    const apres = await onglets();
    console.log("   onglets :", JSON.stringify(apres));
    console.log("   pages   :", JSON.stringify(await pages()));
    console.log(`   ${avant} onglet(s) avant, ${apres.length} après`);
  }

  console.log("\nJournal :");
  for (const l of journal()) console.log("   " + l);

  page.fermer();
  barre.fermer();
  rail.fermer();
} finally {
  tuer();
  await pause(1500);
  try {
    rmSync(profil, { recursive: true, force: true });
  } catch {
    /* le ménage de Windows reprendra le dossier */
  }
}
