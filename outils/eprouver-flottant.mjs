/**
 * Éprouve le **retour de la fenêtre d'incrustation** (Chromium) dans l'onglet de Trame.
 *
 * Constat de Clément, 2026-11-22 : « quand je clique sur sortir la vidéo, je sors bien la vidéo du
 * navigateur, je peux changer d'onglet et naviguer ; mais si je clique sur retour à l'onglet, il
 * renvoie bien la vidéo en cours de lecture, mais il ne m'affiche pas l'onglet dans lequel il y a
 * la vidéo ».
 *
 * C'est exact : dans Chrome, c'est le navigateur qui montre l'onglet ; ici personne ne le faisait
 * — `ui/incrustation.js` écoutait déjà `leavepictureinpicture` (pour relancer une vidéo que la
 * croix met en pause) sans jamais demander à Trame d'afficher l'onglet.
 *
 * Ce banc d'essai le mesure de bout en bout : page locale avec une vidéo, incrustation demandée
 * par un **vrai clic** (CDP), puis un second onglet activé (l'onglet de la vidéo devient alors
 * invisible), puis sortie de l'incrustation — et on regarde si l'onglet de la vidéo est revenu
 * au premier plan, c'est-à-dire si sa page se déclare de nouveau `visible`.
 *
 * Usage :
 *   node outils/eprouver-flottant.mjs                     # target/release/trame.exe
 *   node outils/eprouver-flottant.mjs --exe "C:\…\trame.exe"
 */
import { spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { attendreCible, connecter, pause } from "./cdp.mjs";

const ICI = path.dirname(fileURLToPath(import.meta.url));
const RACINE = path.resolve(ICI, "..");
const args = process.argv.slice(2);
const option = (nom, defaut) => {
  const i = args.indexOf(`--${nom}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : defaut;
};
const EXE = option("exe", path.join(RACINE, "target", "release", "trame.exe"));
const PORT_CDP = 9335;
const PORT_PAGE = 8124;
const VIDEO = option("video", "https://www.w3schools.com/html/mov_bbb.mp4");

/** Une page locale minimale : une video qui boucle, et rien d'autre a charger. */
function servirPage() {
  const html = `<!doctype html><meta charset="utf-8"><title>Essai flottant</title>
<style>html,body{margin:0;height:100%;background:#000}video{width:100%;height:100%}</style>
<video id="v" controls muted loop playsinline preload="auto" src="${VIDEO}"></video>`;
  const serveur = http.createServer((_, rep) => {
    rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    rep.end(html);
  });
  return new Promise((r) => serveur.listen(PORT_PAGE, "127.0.0.1", () => r(serveur)));
}

const ETAT = `(() => {
  const v = document.querySelector('video');
  return {
    pret: !!v && Number.isFinite(v.duration) && v.duration > 3,
    lecture: !!v && !v.paused,
    pip: !!document.pictureInPictureElement,
    visible: document.visibilityState,
    titre: document.title,
  };
})()`;

async function principale() {
  if (!fs.existsSync(EXE)) {
    console.error(`Binaire absent : ${EXE}\nConstruire d'abord : cargo build --release`);
    process.exit(2);
  }
  const serveur = await servirPage();
  const profil = fs.mkdtempSync(path.join(os.tmpdir(), "trame-flottant-"));
  const enfant = spawn(EXE, [], {
    cwd: RACINE,
    env: {
      ...process.env,
      TRAME_PROFIL: profil,
      TRAME_NAV: `http://127.0.0.1:${PORT_PAGE}/`,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP}`,
    },
    stdio: "ignore",
  });
  let cdp = null;
  let ok = true;
  const dire = (t) => console.log(t);
  try {
    dire(`binaire : ${EXE}`);
    dire(`page    : http://127.0.0.1:${PORT_PAGE}/`);
    cdp = await connecter((await attendreCible(PORT_CDP, `127.0.0.1:${PORT_PAGE}`)).webSocketDebuggerUrl);
    await cdp.envoyer("Runtime.enable");
    await cdp.attendre(`(() => { const v = document.querySelector('video'); return !!v && Number.isFinite(v.duration) && v.duration > 3; })()`, {
      delai: 60000,
      quoi: "la video de la page locale",
    });
    dire(`avant   : ${JSON.stringify(await cdp.evaluer(ETAT))}`);

    // Un vrai clic : `requestPictureInPicture` exige un geste utilisateur fiable.
    await cdp.evaluer(`(() => {
      const v = document.querySelector('video');
      v.removeAttribute('disablepictureinpicture');
      v.muted = true;
      const demarrer = () => v.requestPictureInPicture();
      document.addEventListener('click', () => {
        v.play().catch(() => {});
        demarrer().catch((e) => { window.__pipErreur = String(e); });
      }, { once: true, capture: true });
      return true;
    })()`);
    for (const type of ["mousePressed", "mouseReleased"]) {
      await cdp.envoyer("Input.dispatchMouseEvent", { type, x: 400, y: 200, button: "left", clickCount: 1 });
    }
    await pause(1500);
    const apresClic = await cdp.evaluer(ETAT);
    dire(`clic    : ${JSON.stringify(apresClic)}`);
    if (apresClic.pip) {
      // Deuxieme onglet : l'onglet de la video passe au second plan. C'est la situation de
      // Clement quand il navigue pendant que la video est sortie.
      await cdp.evaluer(`window.ipc.postMessage(JSON.stringify({ m: 'raccourci', action: 'nouvelOnglet' }))`);
      await pause(2500);
      const enArrierePlan = await cdp.evaluer(ETAT);
      dire(`autre onglet actif : ${JSON.stringify(enArrierePlan)}`);
      if (!enArrierePlan.pip) {
        dire("\n(la fenetre d'incrustation n'a pas survecu au changement d'onglet : essai interrompu)");
        ok = false;
      } else {
        // Le geste de Clement : « retour a l'onglet » dans la fenetre d'incrustation. Chromium
        // termine l'incrustation ; a Trame de montrer l'onglet.
        await cdp.evaluer(`document.exitPictureInPicture()`);
        await pause(2500);
        const apresRetour = await cdp.evaluer(ETAT);
        dire(`apres retour       : ${JSON.stringify(apresRetour)}`);
        const revenu = apresRetour.visible === "visible";
        dire(`onglet de la video affiche : ${revenu}`);
        ok = ok && revenu && !apresRetour.pip;
      }
    } else {
      const erreur = await cdp.evaluer(`window.__pipErreur || ""`);
      dire(`\n(incrustation impossible : ${erreur || "sans raison donnee"})`);
      ok = false;
    }
    dire(ok ? "\nRECAP : le retour de l'incrustation ramene bien l'onglet" : "\nRECAP : le retour de l'incrustation NE ramene PAS l'onglet");
    return ok;
  } finally {
    cdp?.fermer();
    spawn("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
    serveur.close();
    await pause(1200);
    try {
      fs.rmSync(profil, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
    } catch (e) {
      console.warn(`(profil de test garde : ${profil} — ${e.code || e.message})`);
    }
  }
}

principale()
  .then((ok) => process.exit(ok ? 0 : 1))
  .catch((e) => {
    console.error(`ECHEC : ${e.message}`);
    process.exit(1);
  });
