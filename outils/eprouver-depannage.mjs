#!/usr/bin/env node
/**
 * Éprouve le dépannage (0.4.64) dans le VRAI binaire, caché, sur un profil jetable :
 *
 *  1. vérification d'après mise à jour : un profil « venu d'une vieille version » (version_vue
 *     ancienne, saute-pub retouché sans changer de numéro) est remis à neuf au démarrage ;
 *  2. « Rapport pour Claude » : le fichier est écrit, sans adresse complète ;
 *  3. « Réparer Trame » : caches vidés, extensions livrées remises à neuf, témoins des extensions
 *     Chrome oubliés, réglages techniques par défaut — fils et favoris intacts.
 *
 *   node outils/eprouver-depannage.mjs     (TRAME_BINAIRE / TRAME_PORT_ESSAI au besoin)
 */
import { spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const RACINE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const BINAIRE = process.env.TRAME_BINAIRE || path.join(RACINE, "target", "release", "trame.exe");
const PORT = Number(process.env.TRAME_PORT_ESSAI || 19465);
const PORT_PAGE = 8129;

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

const serveur = http.createServer((_, rep) => {
  rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
  rep.end(`<!doctype html><meta charset="utf-8"><title>Page secrète</title><p>essai</p>`);
});
await new Promise((r) => serveur.listen(PORT_PAGE, "127.0.0.1", r));

const profil = fs.mkdtempSync(path.join(os.tmpdir(), "trame-depannage-"));
const telechargements = path.join(profil, "dl-essai");
const lancer = () =>
  spawn(BINAIRE, [], {
    env: {
      ...process.env,
      TRAME_PROFIL: profil,
      TRAME_CACHEE: "1",
      TRAME_NAV: `http://127.0.0.1:${PORT_PAGE}/une-page-tres-privee?q=secret`,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT} --remote-debugging-address=127.0.0.1`,
    },
    stdio: "ignore",
  });
const arreter = async (enfant) => {
  spawn("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
  await pause(2000);
};
const journal = () => fs.readFileSync(path.join(profil, "trame.log"), "utf8");
const lireReglages = () => JSON.parse(fs.readFileSync(path.join(profil, "reglages.json"), "utf8"));
const ecrireReglages = (r) => fs.writeFileSync(path.join(profil, "reglages.json"), JSON.stringify(r, null, 2));
const anciennes = () => (fs.existsSync(path.join(profil, "extensions-anciennes")) ? fs.readdirSync(path.join(profil, "extensions-anciennes")) : []);
const fichierSaute = path.join(profil, "extensions", "saute-pub", "manifest.json");

try {
  // Premier démarrage : profil neuf, extensions livrées copiées.
  console.log("premier démarrage (profil neuf)");
  let enfant = lancer();
  await attendreCible(PORT, `127.0.0.1:${PORT_PAGE}`, 45000);
  await pause(2500);
  await arreter(enfant);
  ok(fs.existsSync(fichierSaute), "saute-pub copié dans le profil");

  // Le profil « d'une vieille version » : saute-pub retouché, même numéro ; réglages techniques
  // déréglés ; dossier de téléchargements jetable.
  const livre = fs.readFileSync(fichierSaute, "utf8");
  fs.writeFileSync(fichierSaute, livre.replace(/\}\s*$/, ',"_vieux": true}'));
  fs.writeFileSync(path.join(profil, "extensions", "saute-pub", "vieux-fichier.js"), "// laissé par une vieille version");
  const r = lireReglages();
  Object.assign(r, { version_vue: "0.4.1", veille_auto: false, memoire_basse: false, pause_onglets: false, dossier_telechargements: telechargements });
  ecrireReglages(r);

  console.log("\n1. démarrage après « mise à jour »");
  enfant = lancer();
  await attendreCible(PORT, `127.0.0.1:${PORT_PAGE}`, 45000);
  const barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(2000);
  ok(/verification d'apres mise a jour \(0\.4\.1 -> [\d.]+\) : remis a neuf : saute-pub/.test(journal()), "la vérification a remis saute-pub à neuf");
  ok(fs.readFileSync(fichierSaute, "utf8") === livre, "saute-pub identique à la livrée");
  ok(!fs.existsSync(path.join(profil, "extensions", "saute-pub", "vieux-fichier.js")), "le vieux fichier est parti du profil");
  ok(anciennes().some((n) => n.startsWith("saute-pub")), "l'ancienne copie est gardée dans extensions-anciennes");
  ok(lireReglages().version_vue !== "0.4.1", `version vue retenue (${lireReglages().version_vue})`);

  console.log("\n2. rapport pour Claude");
  await barre.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "ouvrir", action: { type: "commande", id: "rapport" } })), true`);
  await pause(2500);
  const rapports = fs.existsSync(telechargements) ? fs.readdirSync(telechargements).filter((f) => f.startsWith("Rapport Trame")) : [];
  ok(rapports.length === 1, `rapport écrit (${rapports.join(", ") || "aucun"})`);
  if (rapports.length) {
    const texte = fs.readFileSync(path.join(telechargements, rapports[0]), "utf8");
    for (const attendu of ["Trame            :", "Windows          :", "Moteur WebView2  :", "Extensions de Trame", "saute-pub", "Réglages :", "Journal"]) {
      ok(texte.includes(attendu), `le rapport contient « ${attendu.trim()} »`);
    }
    ok(!texte.includes("une-page-tres-privee") && !texte.includes("secret"), "aucune adresse complète ni recherche dans le rapport");
    ok(!/"lecture_exclus"|"dossier_telechargements"/.test(texte), "pas de dossiers ni de sites exclus dans le rapport");
  }

  console.log("\n3. réparer Trame");
  const avant = anciennes().length;
  fs.mkdirSync(path.join(profil, "extensions-chrome-adaptees"), { recursive: true });
  fs.writeFileSync(path.join(profil, "extensions-chrome-adaptees", "temoin"), "");
  const reg = lireReglages();
  ecrireReglages({ ...reg });
  await barre.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "ouvrir", action: { type: "commande", id: "reparer" } })), true`);
  await pause(3500);
  const j = journal();
  ok(/reparation : caches de profil principal vides/.test(j), "caches du profil principal vidés");
  ok(/reparation : caches de l'onglet \d+ vides/.test(j), "caches vidés pour l'onglet ouvert");
  ok(anciennes().length > avant, `extensions livrées remises à neuf (${anciennes().length - avant} copie(s) gardée(s))`);
  ok(!fs.existsSync(path.join(profil, "extensions-chrome-adaptees")), "témoins des extensions Chrome oubliés");
  const apres = lireReglages();
  ok(apres.veille_auto && apres.memoire_basse && apres.pause_onglets, "réglages techniques remis par défaut");
  ok(apres.dossier_telechargements === telechargements, "les autres réglages sont intacts");
  barre.fermer();
  await arreter(enfant);
} catch (e) {
  console.error(`ECHEC : ${e.message}`);
  echecs += 1;
} finally {
  serveur.close();
  try {
    fs.rmSync(profil, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
  } catch {
    console.warn(`(profil gardé : ${profil})`);
  }
}
console.log(echecs ? `\n${echecs} échec(s)` : "\ntout tient");
process.exit(echecs ? 1 : 0);
