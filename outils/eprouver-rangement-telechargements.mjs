#!/usr/bin/env node
/**
 * Banc d'essai du RANGEMENT DES TÉLÉCHARGEMENTS, sur le vrai binaire, fenêtre cachée.
 *
 * Profil jetable avec le rangement allumé et une règle (« nom contient facture → Papiers\Factures,
 * renommer en {date} {nom} »). Quatre fichiers sont téléchargés comme par un lien externe :
 *   photo.png        -> Images\photo.png                       (type)
 *   facture-edf.pdf  -> Papiers\Factures\AAAA-MM-JJ facture-edf.pdf (règle + renommage)
 *   plan.sh3d        -> racine                                 (aucun type)
 *   sans-nom (image/webp annoncé, sans extension)  -> Images\… (type annoncé par le site)
 * Puis « Remettre à la racine » sur la photo : le fichier sort de Images\ et le dossier vide part.
 *
 * Usage : node outils/eprouver-rangement-telechargements.mjs   (TRAME_BINAIRE / TRAME_PORT_ESSAI au besoin)
 */
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, mkdirSync, writeFileSync, existsSync, readFileSync, rmSync, readdirSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT_CDP = Number(process.env.TRAME_PORT_ESSAI || 19493);
const PORT_HTTP = PORT_CDP + 2;
const BINAIRE = process.env.TRAME_BINAIRE || join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-rangement-"));
const dossier = mkdtempSync(join(tmpdir(), "trame-rangement-fichiers-"));

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

const maintenant = new Date();
const date = `${maintenant.getFullYear()}-${String(maintenant.getMonth() + 1).padStart(2, "0")}-${String(maintenant.getDate()).padStart(2, "0")}`;

writeFileSync(
  join(profil, "reglages.json"),
  JSON.stringify({
    dossier_telechargements: dossier,
    rangement: {
      actif: true,
      regles: [{ id: 1, actif: true, sites: [], types: [], nom_contient: "facture", dossier: "Papiers/Factures", renommer: "{date} {nom}" }],
    },
  }),
  "utf8"
);

const FICHIERS = {
  "/photo.png": { type: "image/png", disposition: 'attachment; filename="photo.png"' },
  "/facture-edf.pdf": { type: "application/pdf", disposition: 'attachment; filename="facture-edf.pdf"' },
  "/plan.sh3d": { type: "application/octet-stream", disposition: 'attachment; filename="plan.sh3d"' },
  "/sans-nom": { type: "image/webp", disposition: "attachment" },
};
const serveur = createServer((req, res) => {
  const f = FICHIERS[req.url];
  if (!f) {
    res.writeHead(404).end("rien");
    return;
  }
  const corps = Buffer.alloc(4096, 0x42);
  res.writeHead(200, { "Content-Type": f.type, "Content-Disposition": f.disposition, "Content-Length": String(corps.length) });
  res.end(corps);
});
await new Promise((r) => serveur.listen(PORT_HTTP, "127.0.0.1", r));

const enfant = spawn(BINAIRE, [], {
  env: {
    ...process.env,
    TRAME_PROFIL: profil,
    TRAME_CACHEE: "1",
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP} --remote-debugging-address=127.0.0.1`,
  },
  stdio: "ignore",
});
// Un Trame caché ignore `taskkill` sans /F (piège noté) : toujours /F /T.
const tuer = () => {
  try {
    execFileSync("taskkill", ["/F", "/T", "/PID", String(enfant.pid)], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
};

const arbre = (racine, prefixe = "") => {
  const sortie = [];
  for (const n of readdirSync(racine)) {
    const p = join(racine, n);
    if (statSync(p).isDirectory()) sortie.push(...arbre(p, prefixe + n + "\\"));
    else sortie.push(prefixe + n);
  }
  return sortie;
};
const liste = () => {
  try {
    return JSON.parse(readFileSync(join(profil, "telechargements.json"), "utf8").replace(/^﻿/, ""));
  } catch {
    return [];
  }
};

try {
  const rail = await connecter((await attendreCible(PORT_CDP, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  await pause(3000);

  // Les liens arrivent comme quand Trame est le navigateur par défaut : la boîte `a-ouvrir`.
  const boite = join(profil, "a-ouvrir");
  mkdirSync(boite, { recursive: true });
  let n = 0;
  for (const chemin of Object.keys(FICHIERS)) {
    writeFileSync(join(boite, `${Date.now()}-${++n}.url`), `http://127.0.0.1:${PORT_HTTP}${chemin}`, "utf8");
    await pause(1500);
  }
  for (let i = 0; i < 40 && arbre(dossier).length < 4; i++) await pause(500);
  await pause(1500);

  const vus = arbre(dossier).sort();
  console.log("dossier des téléchargements :", JSON.stringify(vus));
  ok(vus.includes("Images\\photo.png"), "photo.png rangée dans Images\\ (type)");
  ok(vus.includes(`Papiers\\Factures\\${date} facture-edf.pdf`), `facture rangée et renommée (règle) : Papiers\\Factures\\${date} facture-edf.pdf`);
  ok(vus.includes("plan.sh3d"), "plan.sh3d laissé à la racine (aucun type)");
  ok(vus.some((v) => v.startsWith("Images\\") && v !== "Images\\photo.png"), "fichier sans extension, image/webp annoncé : rangé dans Images\\");

  const l = liste();
  const photo = l.find((t) => t.fichier === "photo.png");
  ok(photo && photo.range === "Images" && /Images/.test(photo.range_raison || ""), `la liste dit où et pourquoi : ${JSON.stringify(photo && { range: photo.range, raison: photo.range_raison })}`);

  // « Remettre à la racine », envoyé comme la page des téléchargements le fait.
  await rail.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "telechargementRacine", id: ${photo ? photo.id : 0} }))`);
  await pause(1500);
  const apres = arbre(dossier).sort();
  console.log("après « Remettre à la racine » :", JSON.stringify(apres));
  ok(apres.includes("photo.png") && !apres.includes("Images\\photo.png"), "la photo est revenue à la racine");
  ok(existsSync(join(dossier, "Images")), "Images\\ reste (il contient encore l'image sans extension)");
  // La liste est réécrite à la sortie ou à la commande : on la relit après l'arrêt.
  tuer();
  await pause(1000);
  const photo2 = liste().find((t) => t.id === photo?.id);
  ok(photo2 && !photo2.range && photo2.chemin === join(dossier, "photo.png"), `la liste suit le fichier : ${JSON.stringify(photo2 && { chemin: photo2.chemin, range: photo2.range })}`);
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

console.log(echecs === 0 ? "\nRangement : chaque fichier arrive au bon endroit." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
