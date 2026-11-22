/**
 * Eprouve les deux boutons de dossier des telechargements, dans la vraie application.
 *
 * Constat de Clement, 2026-11-22, sur le PC de son fils : « quand il telecharge des documents,
 * ca les met bien dans Telechargements, mais quand je fais ouvrir le dossier, ca m'ouvre le
 * dossier Documents. » Cause mesuree : `explorer` etait appele avec un seul argument
 * (`/select,C:\…\mon document.pdf`). Des que le chemin contient une espace, Windows cite
 * l'argument entier, `explorer` ne reconnait plus le prefixe, et n'echouant pas il ouvre son
 * dossier par defaut. En deux arguments (`/select,` puis le chemin), il ouvre le bon dossier et
 * selectionne le fichier.
 *
 * Ce banc mesure les deux choses que Clement demande, et rien d'autre :
 *   1. une destination **memorisee** ailleurs que le dossier de l'utilisateur recoit bien les
 *      nouveaux fichiers — et l'ancien fichier reste ou il etait ;
 *   2. « Ouvrir le dossier » ouvre la destination memorisee, et « Afficher dans le dossier »
 *      d'une ancienne entree ouvre **son** dossier d'origine — jamais Documents.
 *
 * Le controle ne se fait pas sur le journal seul : on enumere les fenetres de l'Explorateur par
 * le modele objet Shell et on regarde leur `LocationURL`. C'est la seule preuve que le dossier
 * demande est celui qui s'ouvre.
 *
 * Les chemins d'essai contiennent volontairement des espaces : c'est ce qui cassait.
 *
 * Usage :
 *   node outils/eprouver-dossiers.mjs
 *   node outils/eprouver-dossiers.mjs --garder        # garde le profil pour examen
 *   node outils/eprouver-dossiers.mjs --exe "C:\…\trame.exe"
 *
 * Aucune ecriture hors de `%TEMP%\trame-dossiers-*`, et seules les fenetres ouvertes par le banc
 * sont refermees.
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
const PORT_CDP = 9347;
const PORT_HTTP = 8137;
const MARQUE = "trame-dossiers-";

const resultats = [];
const verifier = (ok, message) => {
  resultats.push({ ok: !!ok, message });
  console.log(`  ${ok ? "ok   " : "ECHEC"} ${message}`);
};

const PAGE = `<!doctype html><meta charset="utf-8"><title>essai dossiers</title>
<style>body{font:16px system-ui;padding:40px}a{display:block;padding:30px;background:#2f7d5a;color:#fff;border-radius:8px;text-decoration:none;width:320px}</style>
<a id="dl" href="/fichier%20avec%20espaces.txt" download>Telecharger un fichier</a>`;

/** Les fenetres de l'Explorateur, par leur adresse (une seule preuve qui vaille). */
function fenetresExplorer() {
  const script = `$s = New-Object -ComObject Shell.Application
$s.Windows() | ForEach-Object { $_.LocationURL }`;
  const r = spawnSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8" });
  return (r.stdout || "").split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
}

/** Referme les fenetres ouvertes par le banc, et elles seules. */
function fermerFenetres() {
  spawnSync(
    "powershell",
    ["-NoProfile", "-Command", `$s = New-Object -ComObject Shell.Application
$s.Windows() | Where-Object { $_.LocationURL -match '${MARQUE}' } | ForEach-Object { $_.Quit() }`],
    { stdio: "ignore" }
  );
}

async function principale() {
  if (!fs.existsSync(EXE)) {
    console.error(`Binaire absent : ${EXE}\nConstruire d'abord : cargo build --release`);
    process.exit(2);
  }

  const racine = fs.mkdtempSync(path.join(os.tmpdir(), MARQUE));
  const ancien = path.join(racine, "ancien dossier");
  const nouveau = path.join(racine, "nouveau dossier et espace");
  const profil = path.join(racine, "profil");
  for (const d of [ancien, nouveau, profil]) fs.mkdirSync(d, { recursive: true });
  const vieuxFichier = path.join(ancien, "vieux document.pdf");
  fs.writeFileSync(vieuxFichier, "vieux");

  // Destination memorisee, ailleurs que le dossier de l'utilisateur, avec des espaces.
  fs.writeFileSync(
    path.join(profil, "reglages.json"),
    JSON.stringify({ dossier_telechargements: nouveau }, null, 2)
  );
  // Une entree ancienne : son fichier vit dans l'ANCIEN dossier.
  fs.writeFileSync(
    path.join(profil, "telechargements.json"),
    JSON.stringify(
      [
        {
          id: 111,
          url: "https://exemple.fr/vieux%20document.pdf",
          fichier: "vieux document.pdf",
          chemin: vieuxFichier,
          site: "exemple.fr",
          total: 5,
          recu: 5,
          etat: "termine",
          raison: "",
          debut: Date.now() - 90000,
          fin: Date.now() - 89000,
        },
      ],
      null,
      2
    )
  );

  const serveur = http.createServer((req, rep) => {
    const chemin = decodeURIComponent(new URL(req.url, "http://x").pathname);
    if (chemin === "/") {
      rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      return rep.end(PAGE);
    }
    rep.writeHead(200, {
      "content-type": "application/octet-stream",
      "content-length": 2048,
      "content-disposition": 'attachment; filename="fichier avec espaces.txt"',
    });
    return rep.end(Buffer.alloc(2048, 65));
  });
  await new Promise((r) => serveur.listen(PORT_HTTP, "127.0.0.1", r));

  const enfant = spawn(EXE, [], {
    cwd: RACINE,
    env: {
      ...process.env,
      TRAME_PROFIL: profil,
      TRAME_NAV: `http://127.0.0.1:${PORT_HTTP}/`,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP}`,
    },
    stdio: "ignore",
  });

  const journal = () => {
    const f = path.join(profil, "trame.log");
    return fs.existsSync(f) ? fs.readFileSync(f, "utf8") : "";
  };
  const cliquerTexte = (cdp, texte) =>
    cdp.evaluer(`(() => {
      const b = [...document.querySelectorAll('button')].find((x) => x.textContent.trim() === ${JSON.stringify(texte)});
      if (!b) return false;
      b.click();
      return true;
    })()`);

  let cdp = null;
  let sain = true;
  try {
    console.log(`binaire : ${EXE}`);
    console.log(`ancien  : ${ancien}`);
    console.log(`nouveau : ${nouveau}\n`);

    cdp = await connecter((await attendreCible(PORT_CDP, `127.0.0.1:${PORT_HTTP}`)).webSocketDebuggerUrl);
    await cdp.envoyer("Runtime.enable");
    await cdp.attendre(`!!document.querySelector('#dl')`, { delai: 60000, quoi: "le lien de telechargement" });
    await cdp.evaluer(`document.querySelector('#dl').click()`);

    // 1. La destination memorisee recoit le nouveau fichier ; l'ancien ne bouge pas.
    const cible = path.join(nouveau, "fichier avec espaces.txt");
    for (let i = 0; i < 40 && !fs.existsSync(cible); i += 1) await pause(500);
    verifier(fs.existsSync(cible), "le nouveau fichier arrive dans la destination memorisee");
    verifier(!fs.existsSync(path.join(ancien, "fichier avec espaces.txt")), "il n'arrive pas dans l'ancien dossier");
    verifier(fs.existsSync(vieuxFichier), "l'ancien fichier est reste ou il etait");

    // 2. Les deux boutons de la page des telechargements.
    await cdp.evaluer(`window.ipc.postMessage(JSON.stringify({ m: 'raccourci', action: 'telechargements' }))`);
    await pause(2500);
    cdp.fermer();
    cdp = await connecter((await attendreCible(PORT_CDP, "telechargements.html")).webSocketDebuggerUrl);
    await cdp.envoyer("Runtime.enable");
    await cdp.attendre(`!!document.getElementById('ouvrir-dossier')`, { delai: 20000, quoi: "la page des telechargements" });

    const avantOuvrir = fenetresExplorer();
    await cliquerTexte(cdp, "Ouvrir le dossier");
    await pause(3000);
    const apresOuvrir = fenetresExplorer().filter((u) => !avantOuvrir.includes(u));
    verifier(/dossier ouvert/.test(journal()), "le journal note l'ouverture du dossier");
    verifier(
      apresOuvrir.some((u) => decodeURIComponent(u).includes("nouveau dossier et espace")),
      `« Ouvrir le dossier » ouvre la destination memorisee (${apresOuvrir.map((u) => decodeURIComponent(u)).join(" | ") || "aucune fenetre"})`
    );
    verifier(!apresOuvrir.some((u) => /Documents$/i.test(decodeURIComponent(u))), "il n'ouvre pas Documents");

    const avantAncien = fenetresExplorer();
    const clique = await cdp.evaluer(`(() => {
      const bloc = [...document.querySelectorAll('.dl')].find((d) => (d.querySelector('.nom') || {}).textContent === 'vieux document.pdf');
      if (!bloc) return false;
      const b = [...bloc.querySelectorAll('button')].find((x) => x.textContent.trim() === 'Afficher dans le dossier');
      if (!b) return false;
      b.click();
      return true;
    })()`);
    verifier(clique, "le bouton de l'ancien fichier est present");
    await pause(3000);
    const apresAncien = fenetresExplorer().filter((u) => !avantAncien.includes(u));
    verifier(/montre dans son dossier/.test(journal()), "le journal note l'affichage de l'ancien fichier");
    verifier(
      apresAncien.some((u) => decodeURIComponent(u).includes("ancien dossier")),
      `l'ancien fichier ouvre son propre dossier (${apresAncien.map((u) => decodeURIComponent(u)).join(" | ") || "aucune fenetre"})`
    );
    verifier(!apresAncien.some((u) => /Documents$/i.test(decodeURIComponent(u))), "il n'ouvre pas Documents");
  } catch (e) {
    sain = false;
    verifier(false, `le banc s'est interrompu : ${e.message}`);
  } finally {
    cdp?.fermer();
    spawnSync("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
    serveur.close();
    fermerFenetres();
    await pause(1500);
    const echecs = resultats.filter((r) => !r.ok).length;
    if (!GARDER && echecs === 0) {
      try {
        fs.rmSync(racine, { recursive: true, force: true, maxRetries: 10, retryDelay: 250 });
      } catch (e) {
        console.warn(`(dossier de test garde : ${racine} — ${e.code || e.message})`);
      }
    } else {
      console.log(`\n(dossier de test garde : ${racine})`);
    }
    console.log(`\n${resultats.length - echecs}/${resultats.length} verification(s) passee(s)`);
    process.exit(sain && echecs === 0 ? 0 : 1);
  }
}

principale().catch((e) => {
  console.error(`ECHEC : ${e.message}`);
  process.exit(1);
});
