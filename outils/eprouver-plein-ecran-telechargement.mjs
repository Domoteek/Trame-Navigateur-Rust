#!/usr/bin/env node
/**
 * Banc d'essai du PLEIN ÉCRAN face à un lien reçu de Windows, sur le vrai binaire.
 *
 * Constat de Clément (2026-11-27) : vidéo YouTube en plein écran, puis un téléchargement lancé
 * depuis Discord (Trame est le navigateur par défaut). Le navigateur ne plantait plus, mais la
 * vidéo restait « beuguée » — l'affichage plein écran du lecteur dans une fenêtre normale — et la
 * fenêtre sautait. Il fallait double-cliquer deux fois pour retrouver un plein écran correct.
 *
 * Cause : le moteur garde SON état de plein écran (`ContainsFullScreenElement`). Trame sortait du
 * plein écran pour montrer le nouvel onglet sans le lui dire ; les deux états divergeaient.
 *
 * Vérifie :
 *   1. un lien reçu pendant une page en plein écran s'ouvre EN ARRIÈRE-PLAN : la page garde son
 *      plein écran (`document.fullscreenElement` non nul), et le fichier arrive entier ;
 *   2. l'onglet de téléchargement ne reste pas (il se referme quand le fichier coule) ;
 *   3. quand on change vraiment d'onglet, la page est PRÉVENUE : `document.fullscreenElement`
 *      redevient nul (plus de fenêtre qui saute).
 *
 * Usage : node outils/eprouver-plein-ecran-telechargement.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, mkdirSync, writeFileSync, statSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT_CDP = 19482;
const PORT_HTTP = 19483;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-plein-ecran-"));
const dossier = mkdtempSync(join(tmpdir(), "trame-plein-ecran-fichiers-"));
const TAILLE = 8 * 1024 * 1024;
const FICHIER = `http://127.0.0.1:${PORT_HTTP}/gros.exe`;
const PAGE = pathToFileURL(resolve("outils/banc-onglets.html")).href + `?fichier=${encodeURIComponent(FICHIER)}`;

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

// Le réglage de Clément : plein écran « ecran » (la fenêtre passe en plein écran Windows). C'est le
// mode où la fenêtre SAUTE quand les états divergent, donc c'est celui qu'il faut éprouver.
writeFileSync(
  join(profil, "reglages.json"),
  JSON.stringify({ dossier_telechargements: dossier, plein_ecran: "ecran" }),
  "utf8"
);

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
  const morceau = Buffer.alloc(256 * 1024, 0x42);
  let envoye = 0;
  const pousse = () => {
    if (envoye >= TAILLE) {
      res.end();
      return;
    }
    res.write(morceau);
    envoye += morceau.length;
    setTimeout(pousse, 80);
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

/** Session CDP de la page dont l'adresse contient `contient`. */
async function pageDe(contient) {
  for (let i = 0; i < 90; i++) {
    const c = (await cibles(PORT_CDP)).find((t) => t.type === "page" && t.url.includes(contient));
    if (c) {
      const cdp = await connecter(c.webSocketDebuggerUrl);
      await cdp.envoyer("Page.enable");
      await cdp.envoyer("Runtime.enable");
      return cdp;
    }
    await pause(400);
  }
  throw new Error(`aucune page dont l'adresse contient ${contient}`);
}

async function cliquerLaPage(page, selecteur) {
  const point = await page.evaluer(`(() => {
    const el = document.querySelector(${JSON.stringify(selecteur)});
    if (!el) return null;
    el.scrollIntoView({ block: "center" });
    const r = el.getBoundingClientRect();
    return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
  })()`);
  if (!point) throw new Error(`élément absent : ${selecteur}`);
  await page.envoyer("Input.dispatchMouseEvent", { type: "mouseMoved", x: point.x, y: point.y });
  await pause(120);
  for (const type of ["mousePressed", "mouseReleased"]) {
    await page.envoyer("Input.dispatchMouseEvent", { type, x: point.x, y: point.y, button: "left", clickCount: 1 });
    await pause(120);
  }
}

try {
  const rail = await connecter((await attendreCible(PORT_CDP, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  const barre = await connecter((await attendreCible(PORT_CDP, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000);

  const onglets = () =>
    rail.evaluer(`Array.from(document.querySelectorAll("#fils .fil .onglet")).map((el) => ({
      id: Number(el.dataset.onglet), actif: el.classList.contains("actif"),
      titre: (el.querySelector(".titre") || {}).textContent || "",
    }))`);
  const saisir = async (valeur) => {
    await barre.evaluer(`(() => {
      const c = document.getElementById("couture");
      c.focus();
      c.value = ${JSON.stringify(valeur)};
      c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
    })()`);
    await pause(1500);
  };

  await saisir(PAGE);
  const page = await pageDe("banc-onglets.html");
  await pause(2000);

  // Un deuxième onglet, pour l'étape 3 (changer vraiment d'onglet).
  await rail.evaluer(`document.getElementById("btn-nouvel").click()`);
  await pause(1800);
  let liste = await onglets();
  const idBanc = liste.find((o) => o.titre.includes("Banc"))?.id;
  if (idBanc === undefined) throw new Error(`onglet de mesure introuvable : ${JSON.stringify(liste)}`);
  await rail.evaluer(`document.querySelector('.onglet[data-onglet="${idBanc}"]').click()`);
  await pause(1500);

  // 1. La page passe en plein écran (vrai clic : la permission l'exige).
  await cliquerLaPage(page, "#plein-ecran");
  await pause(2500);
  const plein = await page.evaluer(`!!document.fullscreenElement`);
  console.log("page en plein écran :", plein);
  ok(plein, "la page de mesure est passée en plein écran");

  // 2. Un lien reçu de Windows pendant ce plein écran.
  const avant = (await onglets()).length;
  mkdirSync(join(profil, "a-ouvrir"), { recursive: true });
  writeFileSync(join(profil, "a-ouvrir", `${Date.now()}-1.url`), FICHIER, "utf8");
  console.log("lien externe déposé pendant le plein écran :", FICHIER);

  let taille = 0;
  for (let i = 0; i < 60; i++) {
    await pause(500);
    const f = fichiers()[0];
    taille = f ? f.taille : 0;
    if (taille >= TAILLE) break;
  }
  const apres = await onglets();
  const encorePlein = await page.evaluer(`!!document.fullscreenElement`);
  console.log(`après le lien : ${apres.length} onglet(s) (avant ${avant}), fichier ${taille} octets, plein écran ${encorePlein}`);
  ok(taille >= TAILLE, "le fichier est arrivé entier");
  ok(encorePlein, "la page a GARDÉ son plein écran (le lien n'a pas pris l'écran)");
  ok(apres.length === avant, `aucun onglet de trop (${avant} -> ${apres.length})`);
  ok(apres.some((o) => o.actif && o.titre.includes("Banc")), "l'onglet affiché est resté celui de la page");

  // 3. Changer vraiment d'onglet : la page doit être PRÉVENUE (sinon la fenêtre saute).
  const autre = apres.find((o) => !o.titre.includes("Banc"));
  await rail.evaluer(`document.querySelector('.onglet[data-onglet="${autre.id}"]').click()`);
  await pause(2500);
  const toujoursPlein = await page.evaluer(`!!document.fullscreenElement`);
  console.log("plein écran de la page après changement d'onglet :", toujoursPlein);
  ok(!toujoursPlein, "la page est sortie du plein écran quand Trame en est sorti (états égaux)");

  // Journal, pour la trace.
  tuer();
  ok(await fini(), "le processus a quitté");
  try {
    const lignes = readFileSync(join(profil, "trame.log"), "utf8")
      .split(/\r?\n/)
      .filter((l) => /plein ecran|arri.re-plan|telechargement|referme/i.test(l))
      .slice(-10);
    console.log("journal :");
    for (const l of lignes) console.log("   " + l);
  } catch {
    console.log("(journal illisible)");
  }

  page.fermer();
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

console.log(echecs === 0 ? "\nPlein écran : le lien reçu ne le casse plus." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
