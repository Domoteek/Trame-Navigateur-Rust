#!/usr/bin/env node
/**
 * Verifie l'installation produite par `outils/faire-setup.mjs`, de bout en bout :
 * installation silencieuse dans un dossier jetable, controle des fichiers, du raccourci et de
 * l'entree « Applications », lancement du programme installe, desinstallation silencieuse, et
 * nettoyage complet.
 *
 * Ce que ce script refuse de faire : toucher au vrai profil (`%LOCALAPPDATA%\Trame`). Le
 * programme installe est lance avec `TRAME_PROFIL` sur un dossier temporaire, et le controle
 * final verifie que le profil reel n'a pas ete cree/modifie par l'installation.
 *
 * Lancement :
 *   node outils/verifier-setup.mjs
 *   node outils/verifier-setup.mjs --garder   (garde le dossier installe pour inspection)
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ICI = path.dirname(fileURLToPath(import.meta.url));
const RACINE = path.resolve(ICI, "..");
const DIST = path.join(RACINE, "dist");
const GARDER = process.argv.includes("--garder");

const SYSTEME = process.env.SystemDrive || "C:";
const PROFIL_REEL = path.join(process.env.LOCALAPPDATA, "Trame");
const MENU_DEMARRER = path.join(
  process.env.APPDATA,
  "Microsoft",
  "Windows",
  "Start Menu",
  "Programs"
);

let echecs = 0;
function ok(message) {
  console.log(`  ok    ${message}`);
}
function ko(message) {
  echecs += 1;
  console.log(`  ECHEC ${message}`);
}
function verifier(condition, message) {
  if (condition) ok(message);
  else ko(message);
}

function version() {
  const texte = fs.readFileSync(path.join(RACINE, "Cargo.toml"), "utf8");
  return (texte.match(/^version = "([^"]+)"/m) || [])[1] || "0.0.0";
}

function attendreEnfant(enfant, limiteMs) {
  return new Promise((resoudre) => {
    const minuterie = setTimeout(() => {
      try {
        spawnSync("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
      } catch {
        enfant.kill("SIGKILL");
      }
    }, limiteMs);
    enfant.on("exit", (code) => {
      clearTimeout(minuterie);
      resoudre(code);
    });
  });
}

function registre() {
  const r = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `$k = Get-ChildItem 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall' -ErrorAction SilentlyContinue |
  Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -eq 'Trame' } |
  Select-Object -First 1
if (-not $k) { Write-Output 'absent'; exit 0 }
$p = Get-ItemProperty $k.PSPath
Write-Output ("nom=" + $p.DisplayName)
Write-Output ("version=" + $p.DisplayVersion)
Write-Output ("dossier=" + $p.InstallLocation)
Write-Output ("desinstalleur=" + $p.UninstallString)`,
    ],
    { encoding: "utf8", windowsHide: true }
  );
  const valeurs = {};
  for (const ligne of (r.stdout || "").split(/\r?\n/)) {
    const [cle, ...reste] = ligne.split("=");
    if (cle && reste.length) valeurs[cle.trim()] = reste.join("=").trim();
  }
  return valeurs;
}

/** Valeur d'une cle HKCU (ou `undefined`), par `reg query` : sans PowerShell, sans droits. */
function valeurRegistre(cle, nom) {
  const args = ["query", `HKCU\\${cle}`, ...(nom ? ["/v", nom] : ["/ve"])];
  const r = spawnSync("reg", args, { encoding: "utf8", windowsHide: true });
  if (r.status !== 0) return undefined;
  const ligne = (r.stdout || "").split(/\r?\n/).find((l) => /REG_\w+/.test(l));
  return ligne ? ligne.replace(/^.*?REG_\w+\s*/, "").trim() : "";
}

/** Ce qui fait de Trame un navigateur aux yeux de Windows (Applications par defaut). */
function inscriptionNavigateur() {
  return {
    enregistre: valeurRegistre("Software\\RegisteredApplications", "Trame"),
    commande: valeurRegistre("Software\\Classes\\TrameURL\\shell\\open\\command"),
    https: valeurRegistre(
      "Software\\Clients\\StartMenuInternet\\Trame\\Capabilities\\URLAssociations",
      "https"
    ),
  };
}

/**
 * Une VRAIE installation de Trame existe-t-elle sur ce poste ?
 *
 * Pourquoi cette garde. Ce contrôle installe puis **désinstalle** une copie avec le même
 * identifiant d'application que la vraie (le script Inno n'en a qu'un). Son désinstalleur
 * supprime donc les clés partagées : raccourci du menu Démarrer, entrée « Applications »,
 * inscription navigateur, dossier d'installation mémorisé. Mesure du 2026-11-22 : rejoué alors
 * qu'une 0.4.10 était installée, il a laissé le poste avec un exe orphelin, plus de raccourci,
 * plus d'entrée « Applications » et plus d'inscription navigateur — et le dossier d'installation
 * mémorisé a continué de pointer vers le dossier jetable, si bien que les installations suivantes
 * n'atteignaient plus le vrai.
 */
function installationReelle() {
  const raccourci = path.join(MENU_DEMARRER, "Trame", "Trame.lnk");
  const dossierDefaut = path.join(process.env.LOCALAPPDATA, "Programs", "Trame", "trame.exe");
  const entrees = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `Get-ChildItem 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall' |
         ForEach-Object { $p = Get-ItemProperty $_.PSPath; if ($p.DisplayName -eq 'Trame') { Write-Output $p.InstallLocation } }`,
    ],
    { encoding: "utf8", windowsHide: true }
  );
  const emplacements = `${entrees.stdout || ""}`
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter(Boolean);
  const horsAterlier = emplacements.filter((e) => !e.toLowerCase().includes("\\temp\\"));
  return {
    raccourci: fs.existsSync(raccourci),
    dossier: fs.existsSync(dossierDefaut),
    emplacement: horsAterlier[0] || "",
  };
}

async function main() {
  const v = version();
  const setup = path.join(DIST, `Trame-${v}-setup.exe`);
  console.log(`Verification de l'installation Trame ${v}\n`);
  if (!fs.existsSync(setup)) {
    console.error(`Installation absente : ${setup}\nLancer d'abord : node outils/faire-setup.mjs`);
    process.exit(2);
  }

  // Jamais pendant qu'une vraie installation existe : le désinstalleur d'essai emporterait son
  // raccourci, son entrée « Applications » et son inscription navigateur (voir plus haut).
  const reelle = installationReelle();
  if ((reelle.raccourci || reelle.dossier || reelle.emplacement) && !process.argv.includes("--force")) {
    console.error(
      "Une installation de Trame existe sur ce poste :\n" +
        (reelle.emplacement ? `  dossier enregistre : ${reelle.emplacement}\n` : "") +
        (reelle.dossier
          ? `  executable installe : ${path.join(process.env.LOCALAPPDATA, "Programs", "Trame", "trame.exe")}\n`
          : "") +
        (reelle.raccourci ? "  raccourci du menu Demarrer present\n" : "") +
        "\nCe controle installe puis desinstalle une copie sous le MEME identifiant d'application :\n" +
        "sa desinstallation retirerait le raccourci, l'entree « Applications » et l'inscription\n" +
        "navigateur de la vraie installation (constate le 2026-11-22).\n\n" +
        "Deux issues : desinstaller Trame d'abord (Parametres > Applications), ou lancer avec --force\n" +
        "en sachant que l'installation reelle devra etre reinstallee juste apres."
    );
    process.exit(3);
  }

  const atelier = fs.mkdtempSync(path.join(os.tmpdir(), "trame-setup-"));
  const cible = path.join(atelier, "Programs", "Trame");
  const profilEssai = path.join(atelier, "profil");
  const profilAvant = fs.existsSync(PROFIL_REEL)
    ? fs.statSync(PROFIL_REEL).mtimeMs
    : null;

  // 1. installation silencieuse, par utilisateur, dans un dossier jetable
  console.log("1. installation silencieuse");
  const installation = spawn(
    setup,
    ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-", "/CURRENTUSER", `/DIR=${cible}`, "/TASKS="],
    { stdio: "ignore" }
  );
  const codeInstallation = await attendreEnfant(installation, 120000);
  verifier(codeInstallation === 0, `l'installation se termine sans erreur (code ${codeInstallation})`);

  // 2. fichiers en place
  console.log("\n2. fichiers installes");
  for (const relatif of [
    "trame.exe",
    "ui/chrome.html",
    "ui/chrome.js",
    "ui/reglages.html",
    "extensions/compteur-visites/manifest.json",
    "LISEZMOI.txt",
    "README.md",
    "unins000.exe",
  ]) {
    verifier(fs.existsSync(path.join(cible, relatif)), `présent : ${relatif}`);
  }
  const exe = path.join(cible, "trame.exe");
  const tailleExe = fs.existsSync(exe) ? fs.statSync(exe).size : 0;
  verifier(tailleExe > 1_000_000, `trame.exe pèse ${(tailleExe / 1048576).toFixed(1)} Mo`);

  // 3. signature de l'executable installe (le setup doit avoir embarque l'exe signe)
  console.log("\n3. signature");
  const signature = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `$s = Get-AuthenticodeSignature -FilePath '${exe}'
Write-Output $s.Status
Write-Output $s.SignerCertificate.Subject`,
    ],
    { encoding: "utf8", windowsHide: true }
  );
  const lignesSignature = (signature.stdout || "").split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
  verifier(lignesSignature[0] === "Valid", `signature de l'exe installe : ${lignesSignature[0] || "aucune"}`);
  verifier(
    (lignesSignature[1] || "").includes("CAHAGNE CLEMENT"),
    `signataire : ${lignesSignature[1] || "aucun"}`
  );

  // 4. raccourci et entree « Applications »
  console.log("\n4. integration Windows");
  const raccourci = path.join(MENU_DEMARRER, "Trame", "Trame.lnk");
  verifier(fs.existsSync(raccourci), `raccourci du menu Démarrer : ${raccourci}`);
  const cle = registre();
  verifier(cle.nom === "Trame", `entrée « Applications » : ${cle.nom || "absente"}`);
  verifier(cle.version === v, `version affichée : ${cle.version || "?"}`);
  verifier(
    (cle.dossier || "").toLowerCase().startsWith(cible.toLowerCase()),
    `dossier d'installation enregistré : ${cle.dossier || "?"}`
  );
  const nav = inscriptionNavigateur();
  verifier(
    nav.enregistre === "Software\\Clients\\StartMenuInternet\\Trame\\Capabilities",
    `Trame inscrit comme navigateur : ${nav.enregistre ?? "absent"}`
  );
  verifier(
    (nav.commande || "").toLowerCase() === `"${exe}" "%1"`.toLowerCase(),
    `les liens ouvrent l'exe installé : ${nav.commande ?? "absent"}`
  );
  verifier(nav.https === "TrameURL", `https proposé à Trame : ${nav.https ?? "absent"}`);

  // 5. le programme installe demarre (sur un profil jetable, jamais le vrai)
  console.log("\n5. lancement du programme installe");
  const lancement = spawn(exe, [], {
    env: { ...process.env, TRAME_PROFIL: profilEssai },
    stdio: "ignore",
  });
  await new Promise((r) => setTimeout(r, 9000));
  const journal = fs.existsSync(path.join(profilEssai, "trame.log"))
    ? fs.readFileSync(path.join(profilEssai, "trame.log"), "utf8")
    : "";
  try {
    spawnSync("taskkill", ["/PID", String(lancement.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    lancement.kill();
  }
  verifier(journal.includes("Trame demarre"), "le programme installe écrit son journal de démarrage");
  verifier(
    journal.includes("interface installee dans le profil"),
    "l'interface s'installe dans le profil jetable"
  );
  verifier(
    fs.existsSync(path.join(profilEssai, "webview2", "EBWebView")),
    "le profil WebView2 est bien dans le profil"
  );

  // 6. desinstallation silencieuse
  console.log("\n6. desinstallation");
  const desinstalleur = path.join(cible, "unins000.exe");
  if (fs.existsSync(desinstalleur)) {
    const desinstallation = spawn(
      desinstalleur,
      ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"],
      { stdio: "ignore" }
    );
    await attendreEnfant(desinstallation, 120000);
    // Inno supprime son dossier apres coup : on lui laisse un instant.
    await new Promise((r) => setTimeout(r, 2500));
  }
  verifier(!fs.existsSync(path.join(cible, "trame.exe")), "l'executable est retire");
  verifier(!fs.existsSync(path.join(cible, "ui")), "l'interface est retiree");
  const apres = registre();
  verifier(apres.nom === undefined, `entrée « Applications » retirée (${apres.nom || "absente"})`);
  verifier(!fs.existsSync(raccourci), "raccourci du menu Démarrer retiré");
  const navApres = inscriptionNavigateur();
  verifier(
    navApres.enregistre === undefined && navApres.commande === undefined && navApres.https === undefined,
    "inscription comme navigateur retirée"
  );
  // Les cles sont communes a tout le compte : cet essai a aussi retire celles d'une vraie
  // installation. Il faut donc reinstaller Trame pour de bon apres ce controle.
  console.log("  --   rappel : réinstallez Trame après ce contrôle (l'essai a retiré l'inscription)");

  // 7. le profil reel n'a pas ete touche
  console.log("\n7. profil reel intact");
  const profilApres = fs.existsSync(PROFIL_REEL) ? fs.statSync(PROFIL_REEL).mtimeMs : null;
  // Un Trame ouvert sur le profil reel y ecrit sa session toutes les 8 s : la date du dossier
  // change alors sans que l'installation y soit pour rien (constate le 2026-11-21, un echec sur
  // deux passes). Ce controle n'a de sens que Trame ferme : sinon on le dit, sans conclure.
  const verrou = path.join(PROFIL_REEL, "trame.lock");
  if (fs.existsSync(verrou) && profilAvant !== profilApres) {
    console.log("  --   non vérifiable : Trame est ouvert sur le profil réel et y écrit (fermez-le pour ce contrôle)");
  } else {
    verifier(
      profilAvant === profilApres,
      "le profil réel n'a pas été créé ni modifié par l'installation"
    );
  }

  if (!GARDER) {
    fs.rmSync(atelier, { recursive: true, force: true });
  } else {
    console.log(`\n(dossier conservé : ${atelier})`);
  }

  console.log(
    `\n${echecs === 0 ? "Tout est vérifié" : `${echecs} échec(s)`} — installation ${path.basename(setup)}`
  );
  process.exit(echecs === 0 ? 0 : 1);
}

main().catch((e) => {
  console.error("vérification interrompue :", e);
  process.exit(2);
});
