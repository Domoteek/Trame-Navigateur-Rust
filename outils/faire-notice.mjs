#!/usr/bin/env node
/**
 * Fabrique la notice PDF de Trame : `dist/notice.pdf`.
 *
 * Source : `ui/aide.html?impression` (le même texte que la page Notice, tiré de `ui/notice.js`),
 * imprimée par Microsoft Edge sans fenêtre (`--headless --print-to-pdf`). Edge est présent sur
 * tout Windows 11, et la page s'ouvre en `file://` : notice.js est un script, pas un fetch.
 *
 * Appelée par `faire-setup.mjs` avant Inno Setup : le PDF part dans `{app}\ui\notice.pdf`, que
 * le bouton « Télécharger la notice (PDF) » copie dans les téléchargements. Seule aussi :
 *
 *   node outils/faire-notice.mjs            → dist/notice.pdf
 *   node outils/faire-notice.mjs <sortie>   → ailleurs
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

const ICI = path.dirname(fileURLToPath(import.meta.url));
const RACINE = path.resolve(ICI, "..");

export function versionCargo() {
  const cargo = fs.readFileSync(path.join(RACINE, "Cargo.toml"), "utf8");
  return (cargo.match(/^version\s*=\s*"([^"]+)"/m) || [])[1] || "";
}

function trouverEdge() {
  const candidats = [
    process.env.EDGE,
    path.join(process.env["ProgramFiles(x86)"] || "C:\\Program Files (x86)", "Microsoft", "Edge", "Application", "msedge.exe"),
    path.join(process.env.ProgramFiles || "C:\\Program Files", "Microsoft", "Edge", "Application", "msedge.exe"),
  ].filter(Boolean);
  return candidats.find((c) => fs.existsSync(c)) || null;
}

/** Imprime la notice en PDF ; rend le chemin écrit, ou lève une erreur parlante. */
export function faireNotice(sortie = path.join(RACINE, "dist", "notice.pdf")) {
  const edge = trouverEdge();
  if (!edge) throw new Error("Microsoft Edge introuvable (variable EDGE pour en donner le chemin)");
  const page = pathToFileURL(path.join(RACINE, "ui", "aide.html")).href + `?impression&pdf&version=${encodeURIComponent(versionCargo())}`;
  fs.mkdirSync(path.dirname(sortie), { recursive: true });
  if (fs.existsSync(sortie)) fs.rmSync(sortie);
  // Profil jetable : ne jamais toucher au profil Edge de Clément, ni attendre une instance ouverte.
  const profil = fs.mkdtempSync(path.join(os.tmpdir(), "trame-notice-"));
  try {
    const r = spawnSync(
      edge,
      [
        "--headless=new",
        "--disable-gpu",
        "--no-first-run",
        "--no-pdf-header-footer",
        "--run-all-compositor-stages-before-draw",
        "--virtual-time-budget=4000",
        `--user-data-dir=${profil}`,
        `--print-to-pdf=${sortie}`,
        page,
      ],
      { encoding: "utf8", timeout: 90_000, windowsHide: true }
    );
    if (!fs.existsSync(sortie) || fs.statSync(sortie).size < 20_000) {
      throw new Error(`notice PDF non écrite (code ${r.status}) : ${(r.stderr || "").slice(-600)}`);
    }
  } finally {
    fs.rmSync(profil, { recursive: true, force: true });
  }
  return sortie;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const f = faireNotice(process.argv[2] ? path.resolve(process.argv[2]) : undefined);
    console.log(`notice : ${f} (${Math.round(fs.statSync(f).size / 1024)} Ko)`);
  } catch (e) {
    console.error(`notice : ${e.message}`);
    process.exit(1);
  }
}
