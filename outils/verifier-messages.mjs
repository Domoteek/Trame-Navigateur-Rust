// Audit : chaque message qu'une page d'interface envoie est-il ROUTE par l'hote ?
//
// La lecon est venue trois fois (prive, sauvegardeAuto*, trameReparer) : un message ecrit dans
// une fonction que le repartiteur n'atteint jamais — l'interface envoie, l'hote ignore en
// silence. Ce script compare la liste des messages des pages a leur presence dans `traiter_ipc`
// (le seul endroit qui recoit les messages des surfaces).
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const racine = process.cwd();
const app = readFileSync(join(racine, "src", "app.rs"), "utf8");
const outils = readFileSync(join(racine, "src", "app", "outils.rs"), "utf8");
const depannage = readFileSync(join(racine, "src", "app", "depannage.rs"), "utf8");
const sourceHote = app + "\n" + outils + "\n" + depannage;

// 1. Les messages envoyes par les pages.
const messages = new Map();
for (const fichier of readdirSync(join(racine, "ui"))) {
  if (!/\.(js|html)$/.test(fichier)) continue;
  const texte = readFileSync(join(racine, "ui", fichier), "utf8");
  for (const m of texte.matchAll(/\bm:\s*"([a-zA-Z0-9_]+)"/g)) {
    if (!messages.has(m[1])) messages.set(m[1], new Set());
    messages.get(m[1]).add(fichier);
  }
}

// 2. Ce que l'hote accepte. Tous les fichiers de `src/` comptent : le repartiteur appelle des
//    sous-fonctions (`traiter_ipc_outils`, `traiter_ipc_fenetre`, `traiter_ipc_flottant`,
//    `aide.rs`, `depannage.rs`...), et un bras peut vivre dans n'importe lequel. Ne regarder que
//    `app.rs` produisait des FAUX POSITIFS — `trameReparer`, `trameRapport`, `aideOuvrir`,
//    `fenetre` et `barreFenetre` etaient tous bien traites (constate le 2026-11-23).
const fs = await import("node:fs");
const fiche = (dossier) =>
  fs.readdirSync(dossier, { withFileTypes: true }).flatMap((e) => {
    const chemin = join(dossier, e.name);
    if (e.isDirectory()) return fiche(chemin);
    return e.name.endsWith(".rs") ? [chemin] : [];
  });

const hote = fiche(join(racine, "src"))
  .map((f) => readFileSync(f, "utf8"))
  .join("\n");
const routes = new Set();
for (const m of hote.matchAll(/"([a-zA-Z0-9_]+)"\s*(?:\||=>)/g)) routes.add(m[1]);

// 3. Le verdict.
// `ui/index.html` est la page du SPIKE d'origine : rien ne la sert (aucune route interne ne la
// nomme), donc son `uiReady` ne part jamais. On l'ignore plutot que de supprimer un vestige.
const IGNORES = new Set(["uiReady"]);

const manquants = [...messages.entries()]
  .filter(([m]) => !routes.has(m) && !IGNORES.has(m))
  .map(([m, fichiers]) => `${m} (${[...fichiers].join(", ")})`)
  .sort();

const traites = messages.size - manquants.length - IGNORES.size;

console.log(`messages envoyes par l'interface : ${messages.size}`);
console.log(`reconnus par le repartiteur      : ${traites}`);
if (manquants.length) {
  console.log("\nMESSAGES IGNORES PAR L'HOTE :");
  for (const m of manquants) console.log("  - " + m);
} else {
  console.log("\nAucun message envoye dans le vide.");
}
process.exitCode = manquants.length ? 1 : 0;
