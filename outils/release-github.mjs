#!/usr/bin/env node
/**
 * Met une version publiée de Trame sur GitHub : `main` et les étiquettes poussés, puis une
 * release avec le setup signé, l'archive portable et les empreintes de `dist/`.
 *
 * Règle de Clément (2026-11-24) : « à chaque fois que tu publies l'application, la nouvelle
 * release doit aussi être sur GitHub ». À lancer juste après `upload_trame.js`, avec le même
 * `dist/` (faire-setup.mjs le vide à chaque fabrication).
 *
 *   node outils/release-github.mjs 0.4.63 --titre "des menus qui tiennent" --notes notes.md
 *
 * Vérifie avant : l'étiquette vX.Y.Z existe et le `dist/` est celui de cette version.
 * Vérifie après : la release est la plus récente et son setup a l'empreinte du fichier local.
 */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const RACINE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const DEPOT = "Domoteek/Trame-Navigateur-Rust";
const args = process.argv.slice(2);
const option = (nom) => {
  const i = args.indexOf(`--${nom}`);
  return i >= 0 ? args[i + 1] : undefined;
};
const version = args.find((a) => /^\d+\.\d+\.\d+$/.test(a));
const echouer = (texte) => {
  console.error(`release : ${texte}`);
  process.exit(1);
};
const lancer = (cmd, liste, options = {}) => execFileSync(cmd, liste, { cwd: RACINE, encoding: "utf8", ...options });
const empreinte = (octets) => createHash("sha256").update(octets).digest("hex");

if (!version) echouer("usage : node outils/release-github.mjs X.Y.Z --titre \"…\" --notes fichier.md");
const etiquette = `v${version}`;
const setup = path.join(RACINE, "dist", `Trame-${version}-setup.exe`);
const portable = path.join(RACINE, "dist", `Trame-${version}-portable.zip`);
const sommes = path.join(RACINE, "dist", "SHA256SUMS.txt");
for (const f of [setup, portable, sommes]) if (!fs.existsSync(f)) echouer(`absent : ${f} (dist/ d'une autre version ?)`);
if (!lancer("git", ["tag", "--list", etiquette]).trim()) echouer(`étiquette ${etiquette} absente`);

console.log("1. main et étiquettes poussés");
lancer("git", ["push", "origin", "main"], { stdio: "inherit" });
lancer("git", ["push", "origin", "--tags"], { stdio: "inherit" });

console.log(`2. release ${etiquette}`);
const titre = `Trame ${version}${option("titre") ? ` — ${option("titre")}` : ""}`;
const notes = option("notes");
lancer("gh", [
  "release", "create", etiquette, "--repo", DEPOT, "--title", titre, "--latest",
  ...(notes ? ["--notes-file", notes] : ["--generate-notes"]),
  setup, portable, sommes,
], { stdio: "inherit" });

console.log("3. vérification");
const distante = lancer("gh", ["release", "download", etiquette, "--repo", DEPOT, "-p", path.basename(setup), "-O", "-"], { encoding: "buffer", maxBuffer: 64 * 1024 * 1024 });
const locale = fs.readFileSync(setup);
if (empreinte(distante) !== empreinte(locale)) echouer("le setup de la release n'a pas l'empreinte du fichier local");
const derniere = lancer("gh", ["release", "list", "--repo", DEPOT, "--limit", "1"]).split("\t")[2];
if (derniere !== etiquette) echouer(`la plus récente est ${derniere}, pas ${etiquette}`);
console.log(`   setup ${empreinte(locale)} identique, ${etiquette} est la plus récente`);
console.log(`   https://github.com/${DEPOT}/releases/tag/${etiquette}`);
