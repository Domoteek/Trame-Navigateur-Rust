#!/usr/bin/env node
/**
 * Banc d'essai de la NAVIGATION PRIVEE sur le vrai binaire, par le chemin reel :
 * une vraie touche `Ctrl+Maj+N` envoyee dans une vraie webview (CDP), puis un cookie pose
 * dans la session privee, puis la fermeture, la purge et la relance.
 *
 * Ce qui est eprouve, et pourquoi :
 *
 *  1. **Ctrl+Maj+N** ouvre un fil prive (journal + fil dans le rail) — c'est la touche que
 *     Clement utilise, pas un appel interne ;
 *  2. le fil prive a son PROPRE profil WebView2 (`WV2Profile_prive-<id>`), donc ses cookies ne
 *     sont pas ceux du navigateur ;
 *  3. a la fermeture, la session ECRITE ne contient plus ni le fil prive ni ses onglets ;
 *  4. au demarrage suivant, le profil est purge et **le cookie n'existe plus nulle part**.
 *
 * Nuance mesuree le 2026-11-23 : Chromium recree un dossier de profil *vide* portant le meme nom
 * des son demarrage, et le recree a chaque effacement — insister ne converge pas. La promesse
 * tenue est donc « aucune donnee de la session privee ne survit », et c'est le point 4 qui le
 * prouve.
 *
 * Usage : node outils/eprouver-prive.mjs
 * Code de retour : 0 si tout tient, 1 sinon.
 */
import { spawn, execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { attendreCible, connecter, cibles, pause } from "./cdp.mjs";

const PORT_CDP = 19447;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const NOM_COOKIE = "trame_prive";
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
    // `await` : certaines conditions interrogent la base de cookies (asynchrone).
    if (await condition()) return true;
    await pause(250);
  }
  return await condition();
};

const lireLog = () => {
  try {
    return readFileSync(journal, "utf8");
  } catch {
    return "";
  }
};

const lireSession = () => {
  try {
    return JSON.parse(readFileSync(join(profil, "session.json"), "utf8").replace(/^\uFEFF/, ""));
  } catch {
    return { fils: [], onglets: [] };
  }
};

/** Les dossiers de profils prives presents sur le disque. */
const profilsPrives = () => {
  try {
    return readdirSync(dossierProfils).filter((n) => n.startsWith("WV2Profile_prive-"));
  } catch {
    return [];
  }
};

function lancer() {
  mkdirSync(profil, { recursive: true });
  return spawn(BINAIRE, [], {
    env: {
      ...process.env,
      TRAME_PROFIL: profil,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP}`,
    },
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

async function attendreFin(enfant, ms = 30_000) {
  const fin = Date.now() + ms;
  while (Date.now() < fin) {
    try {
      process.kill(enfant.pid, 0);
    } catch {
      return true;
    }
    await pause(250);
  }
  return false;
}

/** La cible CDP d'un onglet (pas une surface d'interface, pas la coquille `about:blank`). */
async function cibleOnglet() {
  const fin = Date.now() + 45_000;
  while (Date.now() < fin) {
    const c = (await cibles(PORT_CDP)).find(
      (t) =>
        t.type === "page" &&
        !t.url.includes("surface=") &&
        // La webview nait sur `about:blank` : cette cible-la n'a ni `window.ipc` ni les scripts
        // de Trame, s'y attacher fait croire que les touches ne marchent pas (mesure du
        // 2026-11-23, premiere version de ce banc).
        t.url !== "about:blank" &&
        !t.url.startsWith("devtools")
    );
    if (c) return c;
    await pause(400);
  }
  throw new Error("aucune webview d'onglet n'est apparue");
}

/** Envoie une vraie combinaison de touches a une webview (le chemin de l'utilisateur). */
async function touche(cdp, { key, code, vk, ctrl = false, chiffre = false }) {
  const modifiers = (ctrl ? 2 : 0) | (chiffre ? 8 : 0);
  const commun = { key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
  await cdp.envoyer("Input.dispatchKeyEvent", { type: "keyDown", ...commun, text: "" });
  await cdp.envoyer("Input.dispatchKeyEvent", { type: "keyUp", ...commun });
}

/** Les cookies d'un dossier de profil Chromium precis (vide si la base est absente). */
async function cookiesDans(dossier) {
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
  } catch {
    // Chromium tient la base ouverte pendant la session : cette lecture ne vaut qu'apres la
    // fermeture (mesure du 2026-11-23). On ne crie pas, on rend vide.
    return [];
  }
}

/** Les cookies de TOUS les profils prives presents (vide s'il n'y en a aucun). */
async function cookiesDesPrives() {
  const tous = [];
  for (const dossier of profilsPrives()) {
    tous.push(...(await cookiesDans(join(dossierProfils, dossier))));
  }
  return tous;
}

if (!existsSync(BINAIRE)) {
  console.error(`binaire introuvable : ${BINAIRE} (lancer : cargo build --release)`);
  process.exit(1);
}

// Un site local qui pose un cookie : c'est la trace a faire disparaitre.
const serveur = createServer((_, reponse) => {
  reponse.writeHead(200, {
    "Content-Type": "text/html; charset=utf-8",
    "Set-Cookie": `${NOM_COOKIE}=secret; Path=/; Max-Age=86400`,
  });
  reponse.end("<html><body><h1>page privee</h1></body></html>");
});
await new Promise((r) => serveur.listen(0, "127.0.0.1", r));
const site = `http://127.0.0.1:${serveur.address().port}/`;
console.log(`profil d'essai : ${profil}`);
console.log(`site d'essai  : ${site}`);

// ---- 1. la touche, dans la vraie fenetre
let enfant = lancer();
const onglet = await cibleOnglet();
const page = await connecter(onglet.webSocketDebuggerUrl);
await page.envoyer("Runtime.enable");
await touche(page, { key: "N", code: "KeyN", vk: 78, ctrl: true, chiffre: true });
ok(
  await attendre(() => lireLog().includes("navigation privee : fil"), 20_000),
  "Ctrl+Maj+N ouvre un fil prive (le journal le dit)"
);
page.fermer();

// Le fil prive doit se voir dans le rail, et avoir son propre profil WebView2.
const rail = await connecter((await attendreCible(PORT_CDP, "surface=rail")).webSocketDebuggerUrl);
await rail.envoyer("Runtime.enable");
ok(
  await rail.attendre(
    `Array.from(document.querySelectorAll(".fil")).some((f) => (f.textContent || "").includes("Navigation privée"))`,
    { delai: 20_000, quoi: "un fil « Navigation privée » dans le rail" }
  ),
  "le fil privé apparaît dans le rail"
);
ok(
  await attendre(() => profilsPrives().length > 0, 30_000),
  `le fil privé a son propre profil WebView2 (${profilsPrives().join(", ")})`
);

// ---- 2. un cookie pose dans la session privee, par la barre d'adresse (IPC interne)
// Au passage, la MEME touche depuis le RAIL : les surfaces ne recevaient pas `raccourcis.js`,
// donc Ctrl+Maj+N ne faisait rien des que le clavier etait dans le rail (constate le 2026-11-23).
const commun = { key: "N", code: "KeyN", windowsVirtualKeyCode: 78, nativeVirtualKeyCode: 78, modifiers: 10 };
await rail.envoyer("Input.dispatchKeyEvent", { type: "keyDown", ...commun, text: "" });
await rail.envoyer("Input.dispatchKeyEvent", { type: "keyUp", ...commun });
ok(
  await attendre(() => lireLog().includes("raccourci recu : prive (depuis Rail)"), 15_000),
  "Ctrl+Maj+N fonctionne aussi quand le clavier est dans le rail"
);

await rail.evaluer(
  `window.ipc.postMessage(JSON.stringify({ m: "naviguer", url: ${JSON.stringify(site)} }))`
);
// Laisse le temps a la page de se charger et au cookie d'arriver sur le disque. La base n'est
// PAS lisible pendant la session (Chromium la tient) : on verifie apres la fermeture.
await pause(3_000);
rail.fermer();

// ---- 3. fermeture : la session ecrite ne contient plus rien de prive
fermer(enfant);
ok(await attendreFin(enfant), "le processus a bien quitté après la fermeture");
ok(
  await attendre(() => !lireSession().fils.some((f) => f.prive), 30_000),
  "après fermeture, la session écrite ne contient plus de fil privé"
);
const session = lireSession();
ok(!session.onglets.some((o) => o.url === site), "l'onglet privé n'est pas écrit dans la session");

// ---- 3 bis. la preuve d'isolation : le cookie est dans le profil prive, PAS dans le principal
const dansPrives = await cookiesDesPrives();
ok(
  dansPrives.some((c) => c.name === NOM_COOKIE),
  `le cookie a bien été écrit dans le profil privé (${JSON.stringify(dansPrives)})`
);
const dansPrincipal = await cookiesDans(join(dossierProfils, "Default"));
ok(
  !dansPrincipal.some((c) => c.name === NOM_COOKIE),
  "il n'est PAS dans le profil principal : les deux sessions sont séparées"
);

// ---- 4. relance : purge, et le cookie ne doit plus exister
enfant = lancer();
ok(
  await attendre(() => lireLog().includes("profil prive efface"), 30_000),
  "au démarrage suivant, la purge annonce l'effacement du profil privé"
);
await pause(4_000);
const restants = await cookiesDesPrives();
ok(
  !restants.some((c) => c.name === NOM_COOKIE),
  `le cookie de la session privée n'existe plus (${restants.length} cookie(s) : ${JSON.stringify(restants)})`
);
if (profilsPrives().length > 0) {
  console.log(
    `  note  Chromium a recrée ${profilsPrives().join(", ")} (vide, sans le cookie) : insister ne\n` +
      "        converge pas, il le recrée à chaque effacement ; la purge reprend à chaque démarrage."
  );
}

fermer(enfant);
await attendreFin(enfant);
serveur.close();
try {
  rmSync(profil, { recursive: true, force: true });
} catch {
  /* le ménage de Windows reprendra le dossier d'essai */
}

console.log(echecs === 0 ? "\nNavigation privee : tout tient." : `\n${echecs} verification(s) en echec.`);
process.exitCode = echecs === 0 ? 0 : 1;
