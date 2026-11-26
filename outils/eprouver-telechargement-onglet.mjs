#!/usr/bin/env node
/**
 * Banc d'essai des TÉLÉCHARGEMENTS et de l'ONGLET QUI LES LANCE, sur le vrai binaire.
 *
 * Constat de Clément (2026-11-26) : un lien de téléchargement (fichier .exe sur Discord) ouvert par
 * Trame — navigateur par défaut — donnait un onglet NOIR qui ne se chargeait jamais, et qui
 * relançait le téléchargement à chaque redémarrage (« premier chargement sans fin apres 12 s » dans
 * le journal). Cause : la navigation EST le téléchargement, donc aucune page n'arrive jamais.
 *
 * Mesure préalable (`outils/sonder-telechargement-onglet.mjs`) : le fichier survit à la fermeture de
 * l'onglet (24 Mo complets). D'où la règle vérifiée ici :
 *   1. un onglet ouvert POUR un fichier (lien externe) se referme seul, et le fichier arrive entier ;
 *   2. la session écrite ne garde pas cet onglet (donc plus de re-téléchargement au démarrage) ;
 *   3. une PAGE qui navigue vers un fichier n'est pas fermée : elle revient en arrière.
 *
 * Usage : node outils/eprouver-telechargement-onglet.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, writeFileSync, statSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT_CDP = 19479;
const PORT_HTTP = 19481;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-telechargement-"));
const dossier = mkdtempSync(join(tmpdir(), "trame-telechargement-fichiers-"));
const TAILLE = 16 * 1024 * 1024; // 16 Mo, servis lentement : le temps de voir l'onglet partir
const FICHIER = `http://127.0.0.1:${PORT_HTTP}/gros.exe`;

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

writeFileSync(join(profil, "reglages.json"), JSON.stringify({ dossier_telechargements: dossier }), "utf8");

const serveur = createServer((req, res) => {
  if (!req.url.startsWith("/gros.exe")) {
    res.writeHead(404).end("rien");
    return;
  }
  res.writeHead(200, {
    "Content-Type": "application/octet-stream",
    "Content-Disposition": 'attachment; filename="gros.exe"',
    "Content-Length": String(TAILLE),
  });
  const morceau = Buffer.alloc(256 * 1024, 0x41);
  let envoye = 0;
  const pousse = () => {
    if (envoye >= TAILLE) {
      res.end();
      return;
    }
    res.write(morceau);
    envoye += morceau.length;
    setTimeout(pousse, 100);
  };
  pousse();
});
await new Promise((r) => serveur.listen(PORT_HTTP, "127.0.0.1", r));

const enfant = spawn(BINAIRE, [], {
  env: { ...process.env, TRAME_PROFIL: profil, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP}` },
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

const fichiers = () => {
  try {
    return readdirSync(dossier).map((n) => ({ nom: n, taille: statSync(join(dossier, n)).size }));
  } catch {
    return [];
  }
};
const pages = async () =>
  (await cibles(PORT_CDP))
    .filter((t) => t.type === "page" && !t.url.includes("surface=") && !t.url.startsWith("devtools"))
    .map((t) => t.url);

let rail;
let barre;
const nbOnglets = () => rail.evaluer(`document.querySelectorAll("#fils .fil .onglet").length`);
const saisir = async (valeur) => {
  await barre.evaluer(`(() => {
    const c = document.getElementById("couture");
    c.focus();
    c.value = ${JSON.stringify(valeur)};
    c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
  })()`);
  await pause(1200);
};

/** Attend que le fichier cesse de grossir (ou soit complet). */
async function attendreFinFichier() {
  let dernier = 0;
  let stable = 0;
  for (let i = 0; i < 120; i++) {
    await pause(500);
    const f = fichiers()[0];
    const taille = f ? f.taille : 0;
    stable = taille === dernier && taille > 0 ? stable + 1 : 0;
    dernier = taille;
    if (taille >= TAILLE) return taille;
    if (stable >= 8) return taille;
  }
  return dernier;
}

try {
  rail = await connecter((await attendreCible(PORT_CDP, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  barre = await connecter((await attendreCible(PORT_CDP, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000);

  // 1. Le lien EXTERNE, comme Windows le transmet quand Trame est le navigateur par defaut : une
  //    demande deposee dans la boite `a-ouvrir` du profil (c'est le fichier `.url` que lit Trame).
  const avant = await nbOnglets();
  const boite = join(profil, "a-ouvrir");
  mkdtempSync; // no-op pour garder l'import explicite
  writeFileSync(join(profil, ".boite-test"), "", "utf8");
  try {
    (await import("node:fs")).mkdirSync(boite, { recursive: true });
  } catch {
    /* deja la */
  }
  writeFileSync(join(boite, `${Date.now()}-1.url`), FICHIER, "utf8");
  console.log("lien externe déposé :", FICHIER);

  // L'onglet du fichier doit apparaître puis disparaître tout seul, sans jamais rester noir.
  let vu = 0;
  let reste = 0;
  for (let i = 0; i < 40; i++) {
    await pause(500);
    const n = await nbOnglets();
    if (n > avant) vu = n;
    if (vu && n === avant) {
      reste = n;
      break;
    }
  }
  console.log(`onglets : ${avant} avant, ${vu} pendant, ${reste || (await nbOnglets())} après`);
  ok(vu > avant, `l'onglet du fichier a bien existé (${avant} -> ${vu})`);
  ok(reste === avant, `l'onglet du fichier s'est refermé seul (${reste})`);

  const taille = await attendreFinFichier();
  console.log(`fichier : ${taille} / ${TAILLE} octets`);
  ok(taille >= TAILLE, "le fichier est arrivé ENTIER malgré la fermeture de l'onglet");

  // 2. Une PAGE qui navigue vers un fichier n'est pas fermée : elle revient en arrière.
  const page = pathToFileURL(resolve("outils/banc-onglets.html")).href + `?fichier=${encodeURIComponent(FICHIER)}`;
  await saisir(page);
  const avant2 = await nbOnglets();
  const cdp = await (async () => {
    const c = (await cibles(PORT_CDP)).find((t) => t.type === "page" && t.url.includes("banc-onglets.html"));
    if (!c) throw new Error("page de mesure absente");
    const k = await connecter(c.webSocketDebuggerUrl);
    await k.envoyer("Page.enable");
    await k.envoyer("Runtime.enable");
    return k;
  })();
  // Clic réel sur le lien vers le fichier.
  const point = await cdp.evaluer(`(() => {
    const el = document.getElementById("lien-fichier");
    el.scrollIntoView({ block: "center" });
    const r = el.getBoundingClientRect();
    return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
  })()`);
  for (const type of ["mouseMoved", "mousePressed", "mouseReleased"]) {
    await cdp.envoyer("Input.dispatchMouseEvent", { type, x: point.x, y: point.y, button: "left", clickCount: 1 });
    await pause(120);
  }
  await pause(6000);
  const apres2 = await nbOnglets();
  const urls = await pages();
  console.log("après le clic sur un fichier :", JSON.stringify(urls));
  ok(apres2 === avant2, `l'onglet n'est pas fermé (${avant2} -> ${apres2})`);
  ok(
    urls.some((u) => u.includes("banc-onglets.html")),
    `la page est revenue (${JSON.stringify(urls)})`
  );

  // 3. La session écrite ne garde pas d'onglet de téléchargement (l'adresse du FICHIER, pas la
  //    page de mesure qui la cite en paramètre).
  tuer();
  ok(await fini(), "le processus a quitté");
  try {
    const lignes = readFileSync(join(profil, "trame.log"), "utf8")
      .split(/\r?\n/)
      .filter((l) => /telechargement|fichier|chargement sans fin/i.test(l))
      .slice(-8);
    console.log("journal :");
    for (const l of lignes) console.log("   " + l);
  } catch {
    console.log("(journal illisible)");
  }
  const session = JSON.parse(readFileSync(join(profil, "session.json"), "utf8").replace(/^\uFEFF/, ""));
  const fichierEnSession = session.onglets.filter((o) => (o.url || "").startsWith(FICHIER));
  console.log(`session écrite : ${session.onglets.length} onglet(s), ${fichierEnSession.length} sur le fichier`);
  ok(fichierEnSession.length === 0, "aucun onglet de téléchargement gardé en session");

  barre.fermer();
  rail.fermer();
} finally {
  tuer();
  serveur.close();
  try {
    rmSync(profil, { recursive: true, force: true });
    rmSync(dossier, { recursive: true, force: true });
  } catch {
    /* le ménage de Windows reprendra */
  }
}

console.log(echecs === 0 ? "\nTéléchargements : l'onglet ne reste pas noir, le fichier arrive." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
