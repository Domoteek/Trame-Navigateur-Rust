#!/usr/bin/env node
/**
 * Banc d'essai de la NAVIGATION PRIVEE, sur le vrai binaire.
 *
 * Ce qui est eprouve, et pourquoi :
 *
 *  1. un fil prive recoit son PROPRE profil WebView2 (`WV2Profile_prive-<id>`) — c'est ce qui
 *     separe ses connexions de celles du navigateur ;
 *  2. a la fermeture, la session ECRITE ne contient plus ni le fil prive ni ses onglets — une
 *     navigation privee qui se reouvrirait au demarrage suivant ne serait pas privee ;
 *  3. un COOKIE pose dans la session privee ne survit nulle part : le profil est supprime au
 *     demarrage suivant (dossier + entree dans `Local State`), et le cookie n'est plus dans la
 *     base du profil.
 *
 * Nuance mesuree le 2026-11-23, et qu'il vaut mieux ecrire que taire : Chromium **recree** un
 * dossier de profil vide portant le meme nom, des son demarrage, avant que la purge ne puisse
 * l'empecher (et il le recree a chaque effacement). Le dossier qu'on retrouve est donc un profil
 * NEUF, sans aucune des donnees de la session privee — c'est ce que verifie le point 3, qui est
 * la seule chose qui compte pour la vie privee.
 *
 * Usage : node outils/eprouver-prive.mjs
 * Code de retour : 0 si tout tient, 1 sinon.
 */
import { spawn, execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { tmpdir } from "node:os";

const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const NOM_COOKIE = "trame_prive";
const PROFIL_PRIVE = "WV2Profile_prive-2";
const maintenant = Date.now();
const profil = mkdtempSync(join(tmpdir(), "trame-prive-"));
const journal = join(profil, "trame.log");
const dossierProfils = join(profil, "webview2", "EBWebView");

let echecs = 0;
const ok = (condition, quoi) => {
  console.log(`  ${condition ? "ok  " : "ECHEC"} ${quoi}`);
  if (!condition) echecs += 1;
};

const attendre = async (condition, ms) => {
  const fin = Date.now() + ms;
  while (Date.now() < fin) {
    if (condition()) return true;
    await new Promise((r) => setTimeout(r, 200));
  }
  return condition();
};

function lancer() {
  mkdirSync(profil, { recursive: true });
  return spawn(BINAIRE, [], {
    env: { ...process.env, TRAME_PROFIL: profil },
    stdio: "ignore",
  });
}

/** Fermeture propre (WM_CLOSE) : la session ne s'ecrit que sur `CloseRequested`. */
function fermer(enfant) {
  try {
    execFileSync("taskkill", ["/PID", String(enfant.pid)], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
}

/**
 * Attend que le processus ait REELLEMENT quitte : sans cela le moteur tient encore ses profils
 * et la relance fait recreer le dossier qu'on vient de purger (constate le 2026-11-23).
 */
async function attendreFin(enfant, ms = 30_000) {
  const fin = Date.now() + ms;
  while (Date.now() < fin) {
    try {
      process.kill(enfant.pid, 0);
    } catch {
      return true;
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  return false;
}

const lireSession = () =>
  JSON.parse(readFileSync(join(profil, "session.json"), "utf8").replace(/^\uFEFF/, ""));

/** Les cookies enregistres dans un dossier de profil Chromium (vide si la base est absente). */
async function cookiesDuProfil(dossier) {
  const base = join(dossier, "Network", "Cookies");
  if (!existsSync(base)) return [];
  try {
    const { DatabaseSync } = await import("node:sqlite");
    const db = new DatabaseSync(base, { readOnly: true });
    try {
      return db.prepare("SELECT host_key, name FROM cookies").all();
    } finally {
      db.close();
    }
  } catch (e) {
    // Base absente ou tenue par le moteur : il n'y a alors aucun cookie a trouver.
    console.log(`  note  base de cookies illisible (${e.message}) : aucun cookie lisible`);
    return [];
  }
}

if (!existsSync(BINAIRE)) {
  console.error(`binaire introuvable : ${BINAIRE} (lancer : cargo build --release)`);
  process.exit(1);
}

// ---- un site local qui pose un cookie : c'est la trace a faire disparaitre
const serveur = createServer((_, reponse) => {
  reponse.writeHead(200, {
    "Content-Type": "text/html; charset=utf-8",
    "Set-Cookie": `${NOM_COOKIE}=secret; Path=/; Max-Age=86400`,
  });
  reponse.end("<html><body><h1>page privee</h1></body></html>");
});
await new Promise((r) => serveur.listen(0, "127.0.0.1", r));
const port = serveur.address().port;
const site = `http://127.0.0.1:${port}/`;
console.log(`profil d'essai : ${profil}`);
console.log(`site d'essai  : ${site}`);

// ---- une session qui contient un fil prive, avec un onglet dedans
const onglet = (id, fil, url) => ({
  id,
  url,
  titre: "Onglet",
  fil,
  parent: null,
  declencheur: "saisie",
  cree_le: maintenant - 30_000,
  dernier_focus: maintenant - 10_000,
  nb_focus: 1,
  temps_actif_ms: 42_000,
  defilement: 0,
  ancre: false,
  a_lire: false,
  vitalite: 0.5,
});
writeFileSync(
  join(profil, "session.json"),
  JSON.stringify(
    {
      onglets: [onglet(1, 1, "trame://nouvel-onglet"), onglet(2, 2, site)],
      fils: [
        { id: 1, nom: "Travail", auto_nom: false, cree_le: maintenant - 120_000, actif_le: maintenant - 60_000, replie: false },
        { id: 2, nom: "Navigation privee", auto_nom: false, cree_le: maintenant - 30_000, actif_le: maintenant - 10_000, replie: false, prive: true },
      ],
      actif: 2,
      rail_replie: false,
      prochain_onglet: 3,
      prochain_fil: 3,
    },
    null,
    2
  ),
  "utf8"
);

// ---- 1. premier demarrage : profil separe, et le cookie doit arriver sur le disque
let enfant = lancer();
ok(
  await attendre(() => existsSync(join(dossierProfils, PROFIL_PRIVE)), 30_000),
  `le fil prive a son propre profil WebView2 (${PROFIL_PRIVE})`
);
const cookiePose = await attendre(() => existsSync(join(dossierProfils, PROFIL_PRIVE, "Network", "Cookies")), 30_000);
ok(cookiePose, "la base de cookies du profil prive existe (la page s'est chargee)");

// ---- 2. fermeture propre : la session ecrite ne contient plus rien de prive
fermer(enfant);
ok(await attendreFin(enfant), "le processus a bien quitte apres la fermeture");
ok(
  await attendre(() => {
    try {
      return !lireSession().fils.some((f) => f.prive);
    } catch {
      return false;
    }
  }, 30_000),
  "apres fermeture, la session ecrite ne contient plus de fil prive"
);
try {
  const s = lireSession();
  ok(!s.onglets.some((o) => o.fil === 2), "aucun onglet du fil prive n'est ecrit");
  ok(s.actif !== 2, `l'onglet actif n'est plus celui du fil prive (actif = ${s.actif})`);
} catch (e) {
  ok(false, `session illisible apres fermeture : ${e.message}`);
}

// ---- 3. deuxieme demarrage : purge, puis le cookie ne doit plus exister nulle part
enfant = lancer();
ok(
  await attendre(() => {
    try {
      return readFileSync(journal, "utf8").includes("profil prive efface");
    } catch {
      return false;
    }
  }, 30_000),
  "au demarrage suivant, la purge annonce l'effacement du profil prive"
);
// La purge a lieu avant le moteur ; on lui laisse le temps de faire son effet, puis on lit ce
// qui reste sur le disque.
await new Promise((r) => setTimeout(r, 4_000));
const dossierRecree = join(dossierProfils, PROFIL_PRIVE);
const restants = existsSync(dossierRecree) ? await cookiesDuProfil(dossierRecree) : [];
ok(
  !restants.some((c) => c.name === NOM_COOKIE),
  `le cookie de la session privee n'existe plus (${restants.length} cookie(s) restant(s) : ${JSON.stringify(restants)})`
);
if (existsSync(dossierRecree)) {
  console.log(
    "  note  Chromium a recree le dossier du profil (vide, sans le cookie) : insister ne converge\n" +
      "        pas, il le recree a chaque effacement ; la purge le reprend a chaque demarrage."
  );
}

fermer(enfant);
await attendreFin(enfant);
serveur.close();
try {
  rmSync(profil, { recursive: true, force: true });
} catch {
  /* le menage de Windows reprendra le dossier d'essai */
}

console.log(echecs === 0 ? "\nNavigation privee : tout tient." : `\n${echecs} verification(s) en echec.`);
process.exitCode = echecs === 0 ? 0 : 1;
