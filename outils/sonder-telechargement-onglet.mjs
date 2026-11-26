#!/usr/bin/env node
/**
 * SONDE — que devient un téléchargement quand on referme l'onglet qui l'a lancé ?
 *
 * Constat de Clément (2026-11-26) : un lien de téléchargement (fichier .exe sur Discord) ouvre un
 * onglet qui reste NOIR — la navigation est consommée par le téléchargement, donc aucune page
 * n'arrive jamais — et qui relance le téléchargement à chaque redémarrage.
 *
 * Avant de choisir le remède, il faut savoir si le fichier survit à la fermeture de l'onglet :
 * la sonde sert un gros fichier lentement, referme l'onglet en plein téléchargement, et regarde la
 * taille finale.
 *
 * Usage : node outils/sonder-telechargement-onglet.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, writeFileSync, statSync, existsSync, rmSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT_CDP = 19478;
const PORT_HTTP = 19480;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-sonde-telechargement-"));
const dossier = mkdtempSync(join(tmpdir(), "trame-sonde-fichiers-"));
const TAILLE = 24 * 1024 * 1024; // 24 Mo, servi lentement : le temps de refermer l'onglet

// Le dossier de téléchargement du profil d'essai (jamais celui de Clément).
writeFileSync(
  join(profil, "reglages.json"),
  JSON.stringify({ dossier_telechargements: dossier }),
  "utf8"
);

// Un fichier servi en pièces détachées, avec une pause : ~12 s pour 24 Mo.
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
    setTimeout(pousse, 120);
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

const fichiers = () => {
  try {
    return readdirSync(dossier).map((n) => ({ nom: n, taille: statSync(join(dossier, n)).size }));
  } catch {
    return [];
  }
};

try {
  const rail = await connecter((await attendreCible(PORT_CDP, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  const barre = await connecter((await attendreCible(PORT_CDP, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000);

  await barre.evaluer(`(() => {
    const c = document.getElementById("couture");
    c.focus();
    c.value = "http://127.0.0.1:${PORT_HTTP}/gros.exe";
    c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
  })()`);

  // On attend que le fichier grossisse (téléchargement commencé).
  let vu = null;
  for (let i = 0; i < 60; i++) {
    await pause(500);
    const f = fichiers().find((x) => x.taille > 512 * 1024);
    if (f) {
      vu = f;
      break;
    }
  }
  console.log("téléchargement commencé :", JSON.stringify(vu));
  if (!vu) throw new Error("aucun téléchargement n'a démarré");

  // L'onglet qui a lancé le téléchargement : on le referme comme un utilisateur (sa croix).
  const onglets = await rail.evaluer(`Array.from(document.querySelectorAll("#fils .fil .onglet")).map((el) => ({
    id: Number(el.dataset.onglet), titre: (el.querySelector(".titre") || {}).textContent || "",
    domaine: (el.querySelector(".domaine") || {}).textContent || "",
  }))`);
  console.log("onglets :", JSON.stringify(onglets));
  const cible = onglets[onglets.length - 1];
  await rail.evaluer(
    `document.querySelector('.onglet[data-onglet="${cible.id}"] .fermer').click()`
  );
  console.log(`onglet ${cible.id} refermé.`);

  // Le fichier doit continuer à grossir, puis se terminer.
  let dernier = 0;
  let stable = 0;
  for (let i = 0; i < 80; i++) {
    await pause(500);
    const f = fichiers()[0];
    const taille = f ? f.taille : 0;
    if (taille === dernier && taille > 0) stable += 1;
    else stable = 0;
    dernier = taille;
    if (taille >= TAILLE) break;
    if (stable >= 8) break; // 4 s sans bouger : arrêté
  }
  const fin = fichiers()[0];
  console.log(`\ntaille finale : ${fin ? fin.taille : 0} / ${TAILLE} octets`);
  if (fin && fin.taille >= TAILLE) {
    console.log("VERDICT : le fichier est complet malgré la fermeture de l'onglet.");
  } else {
    console.log("VERDICT : le téléchargement est PERDU quand l'onglet se referme.");
  }

  barre.fermer();
  rail.fermer();
} finally {
  tuer();
  serveur.close();
  await pause(1000);
  try {
    rmSync(profil, { recursive: true, force: true });
    rmSync(dossier, { recursive: true, force: true });
  } catch {
    /* le ménage de Windows reprendra */
  }
}
