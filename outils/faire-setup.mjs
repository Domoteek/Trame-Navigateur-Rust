#!/usr/bin/env node
/**
 * Fabrique l'installation de Trame : signature Azure Trusted Signing, setup Inno Setup, archive
 * portable, empreintes SHA-256.
 *
 * Sortie dans `dist/` :
 *   Trame-<version>-setup.exe     installation par utilisateur (aucun droit administrateur)
 *   Trame-<version>-portable.zip  la meme chose sans installation (ui/ + extensions/ a cote de l'exe)
 *   trame.exe                     l'executable signe, tel qu'il est embarque
 *   SHA256SUMS.txt                empreintes des deux fichiers a distribuer
 *
 * Lancement :
 *   node outils/faire-setup.mjs                   Azure Trusted Signing (comme les autres projets)
 *   node outils/faire-setup.mjs --sans-build      reutilise target\release\trame.exe
 *   node outils/faire-setup.mjs --sans-signature  ne signe rien (essai hors ligne)
 *   node outils/faire-setup.mjs --auto-signe      certificat auto-signe local, si Azure est injoignable
 *
 * SUR LA SIGNATURE. Par defaut, l'outil partage de l'ecosysteme est utilise :
 * `D:\LIFE\_tools\signature\signer.ps1` (compte Azure « domoteek », profil « MesApps »,
 * endpoint `https://weu.codesigning.azure.net/`), avec les identifiants de
 * `50-secrets\azure-code-signing.env` — jamais lus ni affiches par ce script. C'est le meme
 * mecanisme que HydraNext, et la meme regle : **l'executable est signe avant de fabriquer
 * l'installation**, sinon le client installe un exe non signe sous un setup signe.
 */

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { faireNotice } from "./faire-notice.mjs";

const ICI = path.dirname(fileURLToPath(import.meta.url));
const RACINE = path.resolve(ICI, "..");
const DIST = path.join(RACINE, "dist");
const EXE = path.join(RACINE, "target", "release", "trame.exe");

// Outil partage de signature Azure (hors projet : il sert a tous les projets LIFE).
const SIGNER_AZURE = "D:/LIFE/_tools/signature/signer.ps1";

const args = process.argv.slice(2);
const SANS_BUILD = args.includes("--sans-build");
const SANS_SIGNATURE = args.includes("--sans-signature");
const AUTO_SIGNE = args.includes("--auto-signe");

const ISCC = [
  "C:/Program Files (x86)/Inno Setup 6/ISCC.exe",
  "C:/Program Files/Inno Setup 6/ISCC.exe",
].find((p) => fs.existsSync(p));

// ------------------------------------------------------------------ outils

function dire(texte) {
  console.log(texte);
}

function echouer(texte) {
  console.error(`\nECHEC : ${texte}`);
  process.exit(1);
}

function lancer(commande, liste, options = {}) {
  const resultat = spawnSync(commande, liste, {
    cwd: RACINE,
    encoding: "utf8",
    windowsHide: true,
    maxBuffer: 32 * 1024 * 1024,
    ...options,
  });
  return {
    code: resultat.status,
    sortie: `${resultat.stdout || ""}${resultat.stderr || ""}`.trim(),
    erreur: resultat.error,
  };
}

function powershell(script, timeoutMs = 120000) {
  const r = lancer(
    "powershell",
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script],
    { timeout: timeoutMs }
  );
  return r;
}

function version() {
  const texte = fs.readFileSync(path.join(RACINE, "Cargo.toml"), "utf8");
  return (texte.match(/^version = "([^"]+)"/m) || [])[1] || "0.0.0";
}

function empreinte(chemin) {
  const h = createHash("sha256");
  h.update(fs.readFileSync(chemin));
  return h.digest("hex");
}

function tailleLisible(octets) {
  if (octets > 1048576) return `${(octets / 1048576).toFixed(1)} Mo`;
  if (octets > 1024) return `${(octets / 1024).toFixed(0)} Ko`;
  return `${octets} o`;
}

// ------------------------------------------------------------------ signature

/** Signe une liste de fichiers avec Azure Trusted Signing (outil partage). */
function signerAzure(fichiers, description = "Trame") {
  if (!fs.existsSync(SIGNER_AZURE)) {
    echouer(`outil de signature Azure introuvable : ${SIGNER_AZURE}`);
  }
  const liste = fichiers.map((f) => `'${f.replace(/'/g, "''")}'`).join(",");
  dire(`   Azure Trusted Signing : ${fichiers.map((f) => path.basename(f)).join(", ")}`);
  const r = powershell(
    `& '${SIGNER_AZURE}' -Fichiers ${liste} -Description '${description}'; exit $LASTEXITCODE`
  );
  // Le script partage ecrit son deroulement ; on ne garde que ce qui compte en cas d'echec.
  if (r.code !== 0) {
    echouer(`signature Azure impossible :\n${r.sortie.split(/\r?\n/).slice(-25).join("\n")}`);
  }
  return r.sortie;
}

/** Certificat auto-signe local : repli quand Azure est injoignable. */
function signerAutoSigne(fichiers) {
  const signtool = [
    "C:/Program Files (x86)/Windows Kits/10/bin/10.0.22621.0/x64/signtool.exe",
    "C:/Program Files (x86)/Windows Kits/10/bin/x64/signtool.exe",
  ].find((p) => fs.existsSync(p));
  if (!signtool) echouer("signtool.exe introuvable (Windows SDK)");

  const existant = powershell(
    `$c = Get-ChildItem Cert:\\CurrentUser\\My -CodeSigningCert | Where-Object { $_.Subject -like '*Trame*' } | Select-Object -First 1
if ($c) { Write-Output $c.Thumbprint }`
  ).sortie;
  let pouce = (existant || "").split(/\r?\n/).map((l) => l.trim()).find((l) => /^[0-9A-F]{40}$/i.test(l));
  if (!pouce) {
    dire("   certificat auto-signe « Trame » : creation…");
    const sortie = powershell(
      `$c = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=Trame" ` +
        `-KeyUsage DigitalSignature -KeyExportPolicy Exportable ` +
        `-CertStoreLocation Cert:\\CurrentUser\\My -NotAfter (Get-Date).AddYears(5)
Write-Output $c.Thumbprint`
    ).sortie;
    pouce = (sortie || "").split(/\r?\n/).map((l) => l.trim()).find((l) => /^[0-9A-F]{40}$/i.test(l));
    if (!pouce) echouer("creation du certificat auto-signe impossible");
  }
  for (const fichier of fichiers) {
    const r = lancer(signtool, [
      "sign", "/fd", "SHA256", "/sha1", pouce, "/d", "Trame",
      "/tr", "http://timestamp.digicert.com", "/td", "SHA256", fichier,
    ]);
    if (r.code !== 0) {
      const sans = lancer(signtool, ["sign", "/fd", "SHA256", "/sha1", pouce, "/d", "Trame", fichier]);
      if (sans.code !== 0) echouer(`signature auto-signee impossible : ${sans.sortie}`);
      dire(`   signe (auto-signe, sans horodatage) : ${path.basename(fichier)}`);
    } else {
      dire(`   signe (auto-signe) : ${path.basename(fichier)}`);
    }
  }
}

function signer(fichiers) {
  if (SANS_SIGNATURE) {
    dire("   signature ignoree (--sans-signature)");
    return;
  }
  if (AUTO_SIGNE) {
    signerAutoSigne(fichiers);
    return;
  }
  signerAzure(fichiers);
}

function verifierSignature(fichier) {
  const r = powershell(
    `$s = Get-AuthenticodeSignature -FilePath '${fichier.replace(/'/g, "''")}'
Write-Output ("statut=" + $s.Status)
Write-Output ("editeur=" + $s.SignerCertificate.Subject)
if ($s.TimeStamperCertificate) { Write-Output ("horodatage=oui") }
Write-Output ("detail=" + $s.StatusMessage)`
  );
  const valeurs = {};
  for (const ligne of (r.sortie || "").split(/\r?\n/)) {
    const [cle, ...reste] = ligne.split("=");
    if (cle && reste.length) valeurs[cle.trim()] = reste.join("=").trim();
  }
  return valeurs;
}

// ------------------------------------------------------------------ taches

function construire() {
  dire("1. compilation (cargo build --release)");
  if (SANS_BUILD && fs.existsSync(EXE)) {
    dire("   ignoree (--sans-build)");
    return;
  }
  const r = lancer("cargo", ["build", "--release"]);
  if (r.code !== 0) echouer(`compilation impossible :\n${r.sortie.slice(-2000)}`);
  dire("   compilee");
}

function copierRessources(destination) {
  const copier = (source, cible) => {
    if (!fs.existsSync(source)) return;
    if (fs.statSync(source).isDirectory()) {
      fs.mkdirSync(cible, { recursive: true });
      for (const nom of fs.readdirSync(source)) copier(path.join(source, nom), path.join(cible, nom));
    } else {
      fs.copyFileSync(source, cible);
    }
  };
  copier(EXE, path.join(destination, "trame.exe"));
  copier(path.join(RACINE, "ui"), path.join(destination, "ui"));
  // La notice PDF fabriquee a l'etape 3 (`faire-notice.mjs`) : lue par « Télécharger la notice ».
  copier(path.join(DIST, "notice.pdf"), path.join(destination, "ui", "notice.pdf"));
  copier(path.join(RACINE, "extensions"), path.join(destination, "extensions"));
  copier(path.join(RACINE, "README.md"), path.join(destination, "README.md"));
  copier(path.join(ICI, "LISEZMOI-install.txt"), path.join(destination, "LISEZMOI.txt"));
}

function portePortable(v) {
  dire("4. archive portable");
  const atelier = path.join(DIST, "portable");
  fs.mkdirSync(atelier, { recursive: true });
  copierRessources(atelier);
  const zip = path.join(DIST, `Trame-${v}-portable.zip`);
  const r = powershell(
    `Compress-Archive -Path '${atelier}\\*' -DestinationPath '${zip}' -Force`
  );
  if (r.code !== 0) echouer(`archive portable impossible : ${r.sortie}`);
  fs.rmSync(atelier, { recursive: true, force: true });
  dire(`   ${path.basename(zip)} (${tailleLisible(fs.statSync(zip).size)})`);
  return zip;
}

function compilerSetup(v) {
  dire("5. compilation de l'installation (Inno Setup)");
  if (!ISCC) echouer("ISCC.exe introuvable (Inno Setup 6)");
  // La version vient de Cargo.toml (seule source) : /D remplace celle ecrite dans trame.iss.
  const r = lancer(ISCC, [`/DAppVersion=${v}`, path.join(ICI, "trame.iss")], {
    env: { ...process.env, TRAME_VERSION: v },
  });
  const setup = path.join(DIST, `Trame-${v}-setup.exe`);
  if (r.code !== 0 || !fs.existsSync(setup)) {
    echouer(`Inno Setup a echoue :\n${r.sortie.slice(-3000)}`);
  }
  dire(`   ${path.basename(setup)} (${tailleLisible(fs.statSync(setup).size)})`);
  return setup;
}

// ------------------------------------------------------------------ principal

function main() {
  const v = version();
  dire(`Trame ${v} — fabrication de l'installation\n`);

  construire();
  if (!fs.existsSync(EXE)) echouer("target/release/trame.exe absent");

  // L'executable est signe AVANT la compilation du setup : c'est celui-la qui sera installe.
  dire("2. signature de l'executable");
  signer([EXE]);

  dire("3. preparation de dist/");
  fs.rmSync(DIST, { recursive: true, force: true });
  fs.mkdirSync(DIST, { recursive: true });
  fs.copyFileSync(EXE, path.join(DIST, "trame.exe"));
  // La notice PDF (ui/aide.html?impression, imprimee par Edge) : embarquee dans {app}\ui.
  try {
    const notice = faireNotice(path.join(DIST, "notice.pdf"));
    dire(`   notice : ${path.basename(notice)} (${tailleLisible(fs.statSync(notice).size)})`);
  } catch (e) {
    echouer(`notice PDF impossible : ${e.message}`);
  }

  const zip = portePortable(v);
  const setup = compilerSetup(v);

  if (!SANS_SIGNATURE) {
    dire("6. signature de l'installation");
    signer([setup]);
  }

  dire("7. empreintes");
  const lignes = [setup, zip].map((f) => `${empreinte(f)}  ${path.basename(f)}`);
  fs.writeFileSync(path.join(DIST, "SHA256SUMS.txt"), `${lignes.join("\n")}\n`, "utf8");
  for (const ligne of lignes) dire(`   ${ligne}`);

  dire("\n8. verification des signatures");
  let toutValide = true;
  for (const fichier of [path.join(DIST, "trame.exe"), setup]) {
    const s = verifierSignature(fichier);
    if (s.statut !== "Valid") toutValide = false;
    dire(
      `   ${path.basename(fichier).padEnd(28)} ${(s.statut || "?").padEnd(12)} ` +
        `${s.editeur || "aucun"}` +
        (s.horodatage ? "  (horodate)" : "")
    );
    if (s.statut !== "Valid" && s.detail) dire(`      detail : ${s.detail}`);
  }

  dire(`\nTermine. A distribuer :`);
  for (const f of [setup, zip]) {
    dire(`  dist\\${path.basename(f)}  (${tailleLisible(fs.statSync(f).size)})`);
  }
  if (!toutValide && !SANS_SIGNATURE) {
    dire("\nATTENTION : une signature n'est pas « Valid ». Voir le detail ci-dessus.");
    process.exit(1);
  }
}

main();
