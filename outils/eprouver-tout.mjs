/**
 * Banc d'essai « tout ensemble » : une video qui joue, trois telechargements lents, un passage
 * en grand ecran, et la navigation dans les ecrans de Trame — pendant que le reste tourne.
 *
 * Pourquoi un banc de plus : `verifier-app.mjs` eprouve un comportement a la fois sur un profil
 * jetable, `eprouver-flottant.mjs` eprouve l'incrustation. Ici on **empile** les gestes, parce
 * que c'est l'empilement qui casse : une lecture qui s'interrompt quand on ouvre les reglages,
 * un grand ecran qui ne se referme pas, un telechargement qui ne finit plus des qu'on navigue.
 *
 * Ce qui est mesure, et par quoi :
 *   - la lecture            : `currentTime` avance entre deux releves, jamais `paused` ;
 *   - le grand ecran        : `document.fullscreenElement` pose, et la video couvre la vue ;
 *   - les telechargements   : trois fichiers servis lentement, comptes dans le journal, puis
 *                             verifies sur le disque a leur taille exacte ;
 *   - la navigation         : reglages, telechargements, rail replie, retour a l'onglet de la
 *                             video — confirmes par le journal de Trame, pas par supposition ;
 *   - la tenue              : le journal ne doit porter aucune panique.
 *
 * Usage :
 *   node outils/eprouver-tout.mjs
 *   node outils/eprouver-tout.mjs --garder
 *   node outils/eprouver-tout.mjs --exe "C:\…\trame.exe"
 *
 * Aucun fichier n'est ecrit hors du profil jetable (`%TEMP%\trame-tout-*`) : le dossier des
 * telechargements est force DANS le profil, comme dans `verifier-app.mjs`.
 */
import { spawn, spawnSync } from "node:child_process";
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
const GARDER = args.includes("--garder");
const PORT_CDP = 9341;
const PORT_HTTP = 8129;
const TAILLE_DL = 8 * 1024 * 1024; // 8 Mio par fichier, 3 fichiers
const NOMS = ["gros-1.bin", "gros-2.bin", "gros-3.bin"];

const resultats = [];
const verifier = (condition, message) => {
  resultats.push({ ok: !!condition, message });
  console.log(`  ${condition ? "ok   " : "ECHEC"} ${message}`);
};

/** La page d'essai : une video qui boucle, et trois liens de telechargement. */
const PAGE = `<!doctype html><meta charset="utf-8"><title>Banc tout ensemble</title>
<style>
  html,body{margin:0;height:100%;background:#12141a;color:#e8e8ef;font:15px system-ui}
  #lecteur{height:62%;background:#000}
  video{width:100%;height:100%;display:block;background:#000}
  #liens{display:flex;gap:28px;padding:26px;justify-content:center}
  a{display:block;padding:28px 40px;background:#2f7d5a;color:#fff;border-radius:10px;text-decoration:none}
</style>
<div id="lecteur"><video id="v" muted loop playsinline preload="auto" src="/video.mp4"></video></div>
<div id="liens">
  <a id="d1" href="/dl/gros-1.bin" download>Fichier 1</a>
  <a id="d2" href="/dl/gros-2.bin" download>Fichier 2</a>
  <a id="d3" href="/dl/gros-3.bin" download>Fichier 3</a>
</div>`;

/** Sert la page, la video (avec plages) et trois fichiers lents. */
function servir(cheminVideo) {
  const video = fs.readFileSync(cheminVideo);
  const serveur = http.createServer((req, rep) => {
    const url = new URL(req.url, "http://127.0.0.1");
    if (url.pathname === "/") {
      rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      return rep.end(PAGE);
    }
    if (url.pathname === "/video.mp4") {
      const plage = req.headers.range;
      if (plage) {
        const m = /bytes=(\d+)-(\d*)/.exec(plage);
        const debut = Number(m[1]);
        const fin = m[2] ? Number(m[2]) : video.length - 1;
        rep.writeHead(206, {
          "content-type": "video/mp4",
          "accept-ranges": "bytes",
          "content-range": `bytes ${debut}-${fin}/${video.length}`,
          "content-length": fin - debut + 1,
        });
        return rep.end(video.subarray(debut, fin + 1));
      }
      rep.writeHead(200, { "content-type": "video/mp4", "accept-ranges": "bytes", "content-length": video.length });
      return rep.end(video);
    }
    const dl = /^\/dl\/(.+)$/.exec(url.pathname);
    if (dl) {
      const nom = path.basename(dl[1]);
      rep.writeHead(200, {
        "content-type": "application/octet-stream",
        "content-length": TAILLE_DL,
        "content-disposition": `attachment; filename="${nom}"`,
      });
      const tranche = Buffer.alloc(64 * 1024, 7);
      let envoye = 0;
      const pomper = () => {
        if (envoye >= TAILLE_DL) return rep.end();
        const reste = TAILLE_DL - envoye;
        const bloc = reste >= tranche.length ? tranche : tranche.subarray(0, reste);
        envoye += bloc.length;
        rep.write(bloc, () => setTimeout(pomper, 60)); // ~8 s par fichier
      };
      return pomper();
    }
    rep.writeHead(404).end();
  });
  return new Promise((r) => serveur.listen(PORT_HTTP, "127.0.0.1", () => r(serveur)));
}

/** Fixe le dossier des telechargements DANS le profil jetable (jamais celui de Clement). */
function telechargementsDansLeProfil(profil) {
  const chemin = path.join(profil, "reglages.json");
  let reglages = {};
  if (fs.existsSync(chemin)) {
    try {
      reglages = JSON.parse(fs.readFileSync(chemin, "utf8"));
    } catch {
      reglages = {};
    }
  }
  const dossier = path.join(profil, "telechargements");
  reglages.dossier_telechargements = dossier;
  fs.writeFileSync(chemin, JSON.stringify(reglages, null, 2));
  return dossier;
}

const ETAT = `(() => {
  const v = document.querySelector('#v');
  const r = v ? v.getBoundingClientRect() : null;
  return {
    pret: !!v && Number.isFinite(v.duration) && v.duration > 3,
    temps: v ? v.currentTime : null,
    pause: v ? v.paused : null,
    plein: !!document.fullscreenElement,
    vue: [innerWidth, innerHeight],
    video: r ? [Math.round(r.width), Math.round(r.height)] : null,
    visible: document.visibilityState,
    url: location.pathname,
  };
})()`;

/** Clic vrai a la souris, au centre de l'element. */
async function cliquer(cdp, selecteur) {
  const boite = await cdp.evaluer(`(() => {
    const e = document.querySelector('${selecteur}');
    if (!e) return null;
    const r = e.getBoundingClientRect();
    return [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)];
  })()`);
  if (!boite) throw new Error(`element introuvable a l'ecran : ${selecteur}`);
  for (const type of ["mousePressed", "mouseReleased"]) {
    await cdp.envoyer("Input.dispatchMouseEvent", { type, x: boite[0], y: boite[1], button: "left", clickCount: 1 });
  }
}

/** Envoie un raccourci a Trame par le pont de la page. */
async function raccourci(cdp, action) {
  await cdp.evaluer(`window.ipc.postMessage(JSON.stringify({ m: 'raccourci', action: '${action}' }))`);
}

async function principale() {
  if (!fs.existsSync(EXE)) {
    console.error(`Binaire absent : ${EXE}\nConstruire d'abord : cargo build --release`);
    process.exit(2);
  }
  const profil = fs.mkdtempSync(path.join(os.tmpdir(), "trame-tout-"));
  const dossierDl = telechargementsDansLeProfil(profil);

  // Video locale : le banc ne depend d'aucun site externe pour la lecture.
  // H.264 dans du MP4 : c'est ce que le moteur de Chromium decode. Un MPEG-4 « part 2 »
  // (l'encodeur `mpeg4`) ne se lit pas du tout — constate en ecrivant ce banc.
  const cheminVideo = path.join(profil, "video.mp4");
  const ffmpeg = spawnSync(
    "ffmpeg",
    ["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=duration=20:size=640x360:rate=20",
     "-c:v", "libx264", "-preset", "ultrafast", "-crf", "30", "-movflags", "+faststart",
     "-pix_fmt", "yuv420p", cheminVideo],
    { stdio: "ignore" }
  );
  if (ffmpeg.status !== 0 || !fs.existsSync(cheminVideo)) {
    console.error("ffmpeg n'a pas su fabriquer la video d'essai");
    process.exit(2);
  }

  const serveur = await servir(cheminVideo);
  const enfant = spawn(EXE, [], {
    cwd: RACINE,
    env: {
      ...process.env,
      TRAME_PROFIL: profil,
      TRAME_NAV: `http://127.0.0.1:${PORT_HTTP}/`,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
        `--remote-debugging-port=${PORT_CDP} --autoplay-policy=no-user-gesture-required`,
    },
    stdio: "ignore",
  });
  const journal = () => {
    const f = path.join(profil, "trame.log");
    return fs.existsSync(f) ? fs.readFileSync(f, "utf8") : "";
  };
  let cdp = null;
  let sain = true;

  try {
    console.log(`binaire : ${EXE}`);
    console.log(`page    : http://127.0.0.1:${PORT_HTTP}/`);
    console.log(`profil  : ${profil}\n`);

    cdp = await connecter((await attendreCible(PORT_CDP, `127.0.0.1:${PORT_HTTP}`)).webSocketDebuggerUrl);
    await cdp.envoyer("Runtime.enable");
    await cdp.evaluer(`(() => {
      const v = document.querySelector('#v');
      if (v) v.addEventListener('error', () => { window.__erreurVideo = v.error ? v.error.code + '/' + v.error.message : 'inconnue'; });
      return true;
    })()`);
    try {
      await cdp.attendre(`(() => { const v = document.querySelector('#v'); return !!v && Number.isFinite(v.duration) && v.duration > 3; })()`, {
        delai: 60000,
        quoi: "la video de la page locale",
      });
    } catch (e) {
      // Diagnostic : ce que la page sait de sa video, plutot qu'une attente muette.
      const diag = await cdp.evaluer(`(() => {
        const v = document.querySelector('#v');
        return { presente: !!v, pret: v ? v.readyState : null, reseau: v ? v.networkState : null,
                 duree: v ? v.duration : null, erreur: window.__erreurVideo || '',
                 code: v && v.error ? v.error.code : null, texte: v && v.error ? v.error.message : '' };
      })()`);
      throw new Error(`${e.message} — etat de la video : ${JSON.stringify(diag)}`);
    }

    // ---- 1. la lecture
    await cdp.evaluer(`document.querySelector('#v').play().catch(() => {})`);
    await pause(600);
    const t0 = await cdp.evaluer(ETAT);
    await pause(1500);
    const t1 = await cdp.evaluer(ETAT);
    verifier(t1.temps > t0.temps + 0.5, `la video avance (${t0.temps.toFixed(2)} s -> ${t1.temps.toFixed(2)} s)`);
    verifier(!t1.pause, "la video n'est pas en pause apres le demarrage");

    // ---- 2. trois telechargements lances ensemble
    for (const n of [1, 2, 3]) {
      await cliquer(cdp, `#d${n}`);
      await pause(250);
    }
    await pause(2000);
    const lances = (journal().match(/telechargement : /g) || []).length;
    verifier(lances >= 3, `trois telechargements lances (journal : ${lances})`);
    const resteeSurLaPage = await cdp.evaluer(`location.pathname === '/'`);
    verifier(resteeSurLaPage, "la page de lecture n'a pas ete quittee par les telechargements");

    // ---- 3. le grand ecran, video toujours en lecture
    await cdp.evaluer(`(() => {
      const zone = document.querySelector('#lecteur');
      const v = document.querySelector('#v');
      zone.addEventListener('click', () => {
        v.play().catch(() => {});
        v.requestFullscreen().catch((e) => { window.__pleinErreur = String(e); });
      }, { once: true, capture: true });
      return true;
    })()`);
    await cliquer(cdp, "#lecteur");
    await pause(1800);
    const plein = await cdp.evaluer(ETAT);
    const erreurPlein = await cdp.evaluer(`window.__pleinErreur || ""`);
    verifier(plein.plein, `la video passe en grand ecran${erreurPlein ? ` (erreur : ${erreurPlein})` : ""}`);
    if (plein.plein) {
      verifier(plein.video[0] >= plein.vue[0] * 0.9, `la video couvre la vue (${plein.video[0]} px sur ${plein.vue[0]})`);
    }
    verifier(!plein.pause, "la lecture continue pendant le grand ecran");

    // ---- 4. sortie du grand ecran
    await cdp.evaluer(`document.exitFullscreen().catch(() => {})`);
    await pause(1500);
    const sorti = await cdp.evaluer(ETAT);
    verifier(!sorti.plein, "le grand ecran se referme");
    verifier(!sorti.pause && sorti.temps > t1.temps, `la lecture n'a pas ete coupee (${sorti.temps.toFixed(2)} s)`);

    // ---- 5. navigation dans les ecrans, pendant que tout tourne
    await raccourci(cdp, "reglages");
    await pause(2200);
    verifier(journal().includes("reglages.html"), "l'ecran des reglages s'ouvre");

    await raccourci(cdp, "telechargements");
    await pause(2200);
    verifier(journal().includes("telechargements.html"), "l'ecran des telechargements s'ouvre");

    await raccourci(cdp, "rail");
    await pause(1200);
    verifier(/rail/.test(journal()), "le rail se replie (commande recue)");

    // Retour a l'onglet de la video : on tourne jusqu'a ce qu'il soit visible.
    let revenu = false;
    for (let i = 0; i < 5 && !revenu; i += 1) {
      await raccourci(cdp, "ongletSuivant");
      await pause(1200);
      revenu = (await cdp.evaluer(ETAT)).visible === "visible";
    }
    verifier(revenu, "le retour a l'onglet de la video aboutit");
    const apresNav = await cdp.evaluer(ETAT);
    verifier(!apresNav.pause, "la video joue toujours apres la navigation");
    verifier(apresNav.temps > sorti.temps + 0.5, `elle a continue d'avancer en arriere-plan (${sorti.temps.toFixed(1)} s -> ${apresNav.temps.toFixed(1)} s)`);

    // ---- 6. les telechargements ont abouti, malgre tout cela
    const echeance = Date.now() + 45000;
    let fichiers = [];
    while (Date.now() < echeance) {
      fichiers = fs.existsSync(dossierDl) ? fs.readdirSync(dossierDl) : [];
      if (fichiers.length >= 3 && fichiers.every((f) => fs.statSync(path.join(dossierDl, f)).size === TAILLE_DL)) break;
      await pause(1000);
    }
    const complets = fichiers.filter((f) => fs.statSync(path.join(dossierDl, f)).size === TAILLE_DL);
    verifier(complets.length === 3, `les trois fichiers sont complets sur le disque (${complets.length}/3)`);
    const termines = (journal().match(/telechargement termine/g) || []).length;
    verifier(termines >= 3, `le journal compte trois fins (${termines})`);
    // Le moteur de WebView2 laisse parfois tomber l'evenement de fin quand plusieurs
    // telechargements se chevauchent (defaut #5638) : on ne juge pas cela comme une faute,
    // on le compte — c'est justement ce que le filet est la pour rattraper.
    const deduites = (journal().match(/fin deduite du fichier/g) || []).length;
    console.log(`  info  ${deduites} fin(s) sur 3 n'ont ete signalees par le moteur qu'a travers le filet`);
    verifier(deduites <= 3, `les fins deduites ne depassent pas les trois telechargements (${deduites})`);

    // ---- 7. la tenue : aucune panique, aucune erreur inattendue
    const lignes = journal().split(/\r?\n/);
    const paniques = lignes.filter((l) => /panique|panicked|RUST_BACKTRACE|thread '.*' panicked/i.test(l));
    verifier(paniques.length === 0, `aucune panique dans le journal${paniques.length ? ` : ${paniques[0]}` : ""}`);
    const erreurs = lignes.filter((l) => l.startsWith("!!") && !/protocole : 404 pour trame:\/\/localhost\/favicon\.ico/.test(l));
    verifier(erreurs.length === 0, `aucune erreur inattendue (${erreurs.length})${erreurs.length ? ` : ${erreurs.slice(0, 3).join(" | ")}` : ""}`);
  } catch (e) {
    sain = false;
    console.log(`  ECHEC le banc s'est interrompu : ${e.message}`);
  } finally {
    cdp?.fermer();
    spawn("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
    serveur.close();
    await pause(1500);
    if (!GARDER) {
      try {
        fs.rmSync(profil, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
      } catch (e) {
        console.warn(`(profil de test garde : ${profil} — ${e.code || e.message})`);
      }
    } else {
      console.log(`\n(profil garde : ${profil})`);
    }
  }

  const echecs = resultats.filter((r) => !r.ok).length;
  console.log(`\n${resultats.length - echecs}/${resultats.length} verification(s) passee(s)`);
  return sain && echecs === 0;
}

principale()
  .then((ok) => process.exit(ok ? 0 : 1))
  .catch((e) => {
    console.error(`ECHEC : ${e.message}`);
    process.exit(1);
  });
