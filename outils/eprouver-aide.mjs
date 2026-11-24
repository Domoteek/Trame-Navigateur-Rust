/**
 * Éprouve l'aide (0.4.62) dans le VRAI binaire, fenêtre cachée, profil jetable :
 *
 *  1. « Vidéo seule » : une vidéo sortie en incrustation survit à la mise en veille des onglets
 *     (constat de Clément, 2026-11-24 : la petite fenêtre disparaissait quand il restait sur un
 *     autre onglet) ;
 *  2. F1 depuis la page de la vidéo ouvre la Notice sur la fiche « video » ;
 *  3. la Couture répond « Côte à côte » à « split écran » ;
 *  4. Ctrl+/ fait dessiner le calque des raccourcis par la barre ;
 *  5. une page refermée puis retapée fait sortir l'astuce « rouvrir », dont « Compris » se reçoit.
 *
 *   node outils/eprouver-aide.mjs [--exe C:\…\trame.exe]
 *
 * Ne vole pas le focus (TRAME_CACHEE) : Clément peut jouer sur le PC pendant l'essai.
 */
import { spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const ICI = path.dirname(fileURLToPath(import.meta.url));
const RACINE = path.resolve(ICI, "..");
const args = process.argv.slice(2);
const option = (nom, defaut) => {
  const i = args.indexOf(`--${nom}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : defaut;
};
const EXE = option("exe", path.join(RACINE, "target", "release", "trame.exe"));
const PORT_CDP = Number(process.env.TRAME_PORT_ESSAI || 9337);
const PORT_PAGE = 8126;
const VIDEO = option("video", "https://www.w3schools.com/html/mov_bbb.mp4");
// --captures <dossier> : garde une image du calque et de l'astuce (rendu reel du moteur).
const CAPTURES = option("captures", "");
async function capturer(cdp, nom) {
  if (!CAPTURES) return;
  fs.mkdirSync(CAPTURES, { recursive: true });
  const r = await cdp.envoyer("Page.captureScreenshot", { format: "png" });
  fs.writeFileSync(path.join(CAPTURES, nom), Buffer.from(r.data, "base64"));
}

function servir() {
  const pages = {
    "/": `<!doctype html><meta charset="utf-8"><title>Essai video</title>
<style>html,body{margin:0;height:100%;background:#000}video{width:100%;height:100%}</style>
<video id="v" controls muted loop playsinline preload="auto" src="${VIDEO}"></video>`,
    "/fermee": `<!doctype html><meta charset="utf-8"><title>Page fermee</title><p>Une page qu'on referme puis qu'on retape.</p>`,
  };
  const serveur = http.createServer((req, rep) => {
    rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    rep.end(pages[req.url.split("?")[0]] || pages["/fermee"]);
  });
  return new Promise((r) => serveur.listen(PORT_PAGE, "127.0.0.1", () => r(serveur)));
}

const bilan = [];
const noter = (nom, ok, detail = "") => {
  bilan.push({ nom, ok });
  console.log(`${ok ? "  ok  " : "  ÉCHEC"} ${nom}${detail ? ` — ${detail}` : ""}`);
};
const envoyerDepuis = (cdp, message) => cdp.evaluer(`window.ipc.postMessage(${JSON.stringify(JSON.stringify(message))}), true`);

async function principale() {
  if (!fs.existsSync(EXE)) throw new Error(`binaire absent : ${EXE} (cargo build --release)`);
  const serveur = await servir();
  const profil = fs.mkdtempSync(path.join(os.tmpdir(), "trame-aide-"));
  const enfant = spawn(EXE, [], {
    cwd: RACINE,
    env: {
      ...process.env,
      TRAME_PROFIL: profil,
      TRAME_CACHEE: "1",
      TRAME_NAV: `http://127.0.0.1:${PORT_PAGE}/`,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP} --remote-debugging-address=127.0.0.1`,
    },
    stdio: "ignore",
  });
  const ouverts = [];
  const ouvrir = async (contient, delai) => {
    const c = await connecter((await attendreCible(PORT_CDP, contient, delai)).webSocketDebuggerUrl);
    await c.envoyer("Runtime.enable");
    ouverts.push(c);
    return c;
  };
  try {
    console.log(`binaire : ${EXE}\nprofil  : ${profil}\n`);
    const video = await ouvrir(`127.0.0.1:${PORT_PAGE}/`, 60000);
    await video.attendre(`(() => { const v = document.querySelector('video'); return !!v && Number.isFinite(v.duration) && v.duration > 3; })()`, { quoi: "la video" });
    const barre = await ouvrir("chrome.html?surface=barre");

    // 1. Video seule, puis un autre onglet, puis « Endormir les onglets froids ».
    await video.evaluer(`(() => {
      const v = document.querySelector('video');
      document.addEventListener('click', () => { v.play().catch(() => {}); v.requestPictureInPicture().catch((e) => { window.__pipErreur = String(e); }); }, { once: true, capture: true });
      return true;
    })()`);
    for (const type of ["mousePressed", "mouseReleased"]) {
      await video.envoyer("Input.dispatchMouseEvent", { type, x: 300, y: 200, button: "left", clickCount: 1 });
    }
    await pause(1500);
    const pip = await video.evaluer(`!!document.pictureInPictureElement`);
    // L'etat de lecture AVANT de changer d'onglet : c'est lui qui doit tenir apres.
    const jouait = await video.evaluer(`!document.querySelector('video').paused`);
    console.log(`  (après le clic : incrustation ${pip}, lecture ${jouait})`);
    if (!pip) {
      noter("vidéo seule : incrustation obtenue", false, await video.evaluer(`window.__pipErreur || "refusée"`));
    } else {
      await envoyerDepuis(video, { m: "raccourci", action: "nouvelOnglet" });
      await pause(1500);
      await envoyerDepuis(barre, { m: "ouvrir", action: { type: "commande", id: "veille" } });
      await pause(2500);
      const encore = (await cibles(PORT_CDP)).some((t) => t.url.includes(`127.0.0.1:${PORT_PAGE}/`) && !t.url.includes("fermee"));
      const etat = encore ? await video.evaluer(`({ pip: !!document.pictureInPictureElement, pause: document.querySelector('video').paused })`).catch((e) => ({ erreur: e.message })) : { page: "détruite" };
      const toujours = encore && etat.pip === true && etat.pause === !jouait;
      noter("vidéo seule : survit à la mise en veille des onglets", toujours, JSON.stringify(etat));
      if (!toujours && fs.existsSync(path.join(profil, "trame.log"))) {
        const journal = fs.readFileSync(path.join(profil, "trame.log"), "utf8").split(/\r?\n/).filter((l) => /endormi|incrust|veille|flottant|raccourci/i.test(l));
        console.log(journal.slice(-14).map((l) => "      " + l).join("\n"));
      }
      // 2. F1 depuis la page de la video : la Notice, fiche video (une video flotte).
      await envoyerDepuis(video, { m: "raccourci", action: "aide" });
      const aide = await ouvrir("aide.html", 20000);
      await aide.attendre(`!!document.querySelector('.fiche h2')`, { quoi: "la fiche" });
      const fiche = await aide.evaluer(`[location.hash, document.querySelector('.fiche h2').textContent, document.getElementById('version').textContent]`);
      noter("F1 ouvre la Notice sur la fiche de ce qu'on regarde", fiche[0] === "#video", JSON.stringify(fiche));
      noter("la Notice reçoit l'état (version)", /^Version \d/.test(fiche[2]), fiche[2]);
      await video.evaluer(`document.exitPictureInPicture().catch(() => {}), true`).catch(() => {});
    }

    // 3. La Couture : « split écran ».
    await barre.evaluer(`(() => { const r = window.trame.resultats; window.__derniers = null; window.trame.resultats = (x) => { window.__derniers = x; r(x); }; return true; })()`);
    await envoyerDepuis(barre, { m: "requete", rid: 4242, q: "split écran" });
    await barre.attendre(`window.__derniers && window.__derniers.rid === 4242`, { delai: 8000, quoi: "réponse de la Couture" });
    const premiers = await barre.evaluer(`window.__derniers.items.slice(0, 2).map((i) => i.genre + ':' + i.id)`);
    noter("la Couture répond Côte à côte à « split écran »", premiers[0] === "aide:aide:cote-a-cote", JSON.stringify(premiers));
    await envoyerDepuis(barre, { m: "requete", rid: 4243, q: "leboncoin.fr" });
    await barre.attendre(`window.__derniers && window.__derniers.rid === 4243`, { delai: 8000, quoi: "seconde réponse" });
    const adresse = await barre.evaluer(`window.__derniers.items.filter((i) => i.genre === 'aide').length`);
    noter("une adresse n'affiche pas d'aide", adresse === 0, `${adresse} ligne(s) d'aide`);

    // 4. Ctrl+/ : le calque.
    await envoyerDepuis(barre, { m: "raccourci", action: "raccourcisCalque" });
    await barre.attendre(`!document.getElementById('calque-raccourcis').hidden`, { delai: 6000, quoi: "le calque" });
    const lignes = await barre.evaluer(`document.querySelectorAll('#calque-raccourcis .calque-ligne').length`);
    noter("Ctrl+/ dessine le calque des raccourcis", lignes > 20, `${lignes} lignes`);
    await pause(400);
    await capturer(barre, "calque.png");
    await barre.evaluer(`window.__trame_echap(), true`);

    // 5. Astuce « rouvrir » : ouvrir une page, la fermer, la retaper.
    const url = `http://127.0.0.1:${PORT_PAGE}/fermee`;
    await envoyerDepuis(barre, { m: "naviguer", url, mode: "onglet" });
    const fermee = await ouvrir("/fermee", 15000);
    await fermee.attendre(`document.readyState === 'complete'`, { quoi: "la page à fermer" });
    await envoyerDepuis(fermee, { m: "raccourci", action: "fermerOnglet" });
    await pause(1200);
    await envoyerDepuis(barre, { m: "naviguer", url });
    await pause(800);
    const retapee = await ouvrir("/fermee", 15000);
    let astuce = "";
    try {
      await retapee.attendre(`!!document.getElementById('trame-astuce')`, { delai: 12000, quoi: "l'astuce" });
      astuce = await retapee.evaluer(`document.getElementById('trame-astuce').textContent`);
      await capturer(retapee, "astuce.png");
    } catch (e) {
      astuce = "";
    }
    noter("une page refermée puis retapée fait sortir l'astuce Ctrl+Maj+T", astuce.includes("Ctrl+Maj+T"), astuce.slice(0, 90));
    if (astuce) {
      await retapee.evaluer(`[...document.querySelectorAll('#trame-astuce button')].find((b) => b.textContent === 'Compris').click(), true`);
      await pause(600);
      const reglages = JSON.parse(fs.readFileSync(path.join(profil, "reglages.json"), "utf8"));
      noter("l'astuce vue est retenue (elle ne reviendra pas)", (reglages.astuces_vues || []).includes("rouvrir"), JSON.stringify(reglages.astuces_vues));
    }
  } finally {
    for (const c of ouverts) c.fermer();
    spawn("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
    serveur.close();
    await pause(1500);
    try {
      fs.rmSync(profil, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
    } catch (e) {
      console.warn(`(profil de test gardé : ${profil} — ${e.code || e.message})`);
    }
  }
  const echecs = bilan.filter((b) => !b.ok).length;
  console.log(`\n${bilan.length - echecs}/${bilan.length} vérifications passées`);
  return echecs === 0;
}

principale()
  .then((ok) => process.exit(ok ? 0 : 1))
  .catch((e) => {
    console.error(`ECHEC : ${e.message}`);
    process.exit(1);
  });
