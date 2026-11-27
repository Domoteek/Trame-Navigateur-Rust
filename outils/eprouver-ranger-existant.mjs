#!/usr/bin/env node
/**
 * Banc d'essai de « RANGER L'EXISTANT » (Réglages → Téléchargements), sur le vrai binaire, caché.
 *
 * Dossier de téléchargements jetable, déjà rempli :
 *   - à la racine, à ranger : photo.jpg, facture-edf.pdf (règle « facture »), conflit.png (un
 *     Images\conflit.png existe déjà : les deux doivent rester) ;
 *   - à la racine, à laisser : plan.sh3d (aucun type), .cache-perso, en-cours.crdownload ;
 *   - dans un sous-dossier : Perso\a.png (jamais touché).
 * Puis aperçu → ranger → défaire, par les messages de la rubrique. Défaire doit rendre
 * EXACTEMENT l'arbre de départ.
 *
 * Usage : node outils/eprouver-ranger-existant.mjs   (TRAME_BINAIRE / TRAME_PORT_ESSAI au besoin)
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT_CDP = Number(process.env.TRAME_PORT_ESSAI || 19497);
const BINAIRE = process.env.TRAME_BINAIRE || join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-existant-"));
const dossier = mkdtempSync(join(tmpdir(), "trame-existant-fichiers-"));

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

// Le rangement est ÉTEINT : on regarde justement avant de l'allumer.
writeFileSync(
  join(profil, "reglages.json"),
  JSON.stringify({
    dossier_telechargements: dossier,
    rangement: {
      actif: false,
      regles: [{ id: 1, actif: true, sites: [], types: [], nom_contient: "facture", dossier: "Papiers/Factures", renommer: "" }],
    },
  }),
  "utf8"
);
const ecrire = (chemin, contenu) => {
  mkdirSync(join(dossier, chemin, ".."), { recursive: true });
  writeFileSync(join(dossier, chemin), contenu);
};
ecrire("photo.jpg", "jpg");
ecrire("facture-edf.pdf", "pdf");
ecrire("conflit.png", "nouvelle");
ecrire("Images/conflit.png", "ancienne");
ecrire("plan.sh3d", "sh3d");
ecrire(".cache-perso", "cache");
ecrire("en-cours.crdownload", "partiel");
ecrire("Perso/a.png", "perso");

const arbre = (racine, prefixe = "") => {
  const sortie = [];
  for (const n of readdirSync(racine)) {
    const p = join(racine, n);
    if (statSync(p).isDirectory()) sortie.push(...arbre(p, prefixe + n + "\\"));
    else sortie.push(prefixe + n + "=" + readFileSync(p, "utf8"));
  }
  return sortie.sort();
};
const depart = arbre(dossier);

const enfant = spawn(BINAIRE, [], {
  env: {
    ...process.env,
    TRAME_PROFIL: profil,
    TRAME_CACHEE: "1",
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP} --remote-debugging-address=127.0.0.1`,
  },
  stdio: "ignore",
});
const tuer = () => {
  try {
    execFileSync("taskkill", ["/F", "/T", "/PID", String(enfant.pid)], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
};

try {
  // Une surface d'interface a le droit d'envoyer les messages de la rubrique.
  const rail = await connecter((await attendreCible(PORT_CDP, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  await pause(3000);
  const envoyer = (o) => rail.evaluer(`window.ipc.postMessage(${JSON.stringify(JSON.stringify(o))})`);

  console.log("départ :", JSON.stringify(depart));
  await envoyer({ m: "rangementApercu" });
  await pause(1500);
  ok(JSON.stringify(arbre(dossier)) === JSON.stringify(depart), "l'aperçu ne déplace rien");

  await envoyer({ m: "rangementValider" });
  await pause(2000);
  const range = arbre(dossier);
  console.log("rangé  :", JSON.stringify(range));
  const racine = range.filter((f) => !f.includes("\\")).map((f) => f.split("=")[0]);
  ok(!racine.includes("photo.jpg"), "photo.jpg a quitté la racine");
  ok(range.includes("Papiers\\Factures\\facture-edf.pdf=pdf"), "la facture suit la règle (Papiers\\Factures)");
  ok(range.includes("Images\\conflit.png=ancienne") && range.some((f) => /^Images\\conflit.*\.png=nouvelle$/.test(f) && f !== "Images\\conflit.png=nouvelle"), "conflit de nom : les deux exemplaires restent");
  ok(["plan.sh3d", ".cache-perso", "en-cours.crdownload"].every((n) => racine.includes(n)), "sans type, caché et en cours : laissés à la racine");
  ok(range.includes("Perso\\a.png=perso"), "un sous-dossier existant n'est pas touché");
  let journal = [];
  try {
    journal = readdirSync(profil).filter((n) => /rangement/i.test(n));
  } catch {
    /* rien */
  }
  ok(journal.length > 0, `journal de rangement écrit dans le profil (${JSON.stringify(journal)})`);

  await envoyer({ m: "rangementDefaire" });
  await pause(2000);
  const remis = arbre(dossier);
  console.log("défait :", JSON.stringify(remis));
  ok(JSON.stringify(remis) === JSON.stringify(depart), "défaire rend exactement l'arbre de départ");
  rail.fermer();
} finally {
  tuer();
  await pause(800);
  for (const d of [profil, dossier]) {
    try {
      rmSync(d, { recursive: true, force: true });
    } catch {
      /* le ménage de Windows reprendra le dossier */
    }
  }
}

console.log(echecs === 0 ? "\nRanger l'existant : rangé, rien de touché à tort, et défait à l'identique." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
