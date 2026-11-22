#!/usr/bin/env node
// Fabrique toutes les images de l'icone a partir de ressources/trame.svg (la seule source) :
//   ressources/icones/trame-<taille>.png   une image par taille, fond transparent
//   ressources/trame-64.rgba               pixels bruts 64x64 pour l'icone de fenetre (tao)
//   ressources/icones/apercu.png           planche de controle (fond sombre et fond clair)
// puis lance outils/faire-ico.mjs pour assembler ressources/trame.ico.
//
// Le dessin passe par Chromium (Playwright), le meme moteur que l'interface : un SVG rendu
// a 16 px y est lisse correctement, sans dependance de traitement d'image dans le projet.
//
// Usage : node outils/faire-icones.mjs

import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, "..");
const svg = readFileSync(join(racine, "ressources", "trame.svg"), "utf8");
const dossier = join(racine, "ressources", "icones");
mkdirSync(dossier, { recursive: true });

const TAILLES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256];

// Playwright : celui du projet s'il existe, sinon l'installation partagee de LIFE.
function chargerPlaywright() {
  const candidats = [
    process.env.PLAYWRIGHT_CORE,
    join(racine, "node_modules", "playwright-core"),
    "D:/LIFE/_tools/playwright/node_modules/playwright-core",
    "D:/LIFE/_tools/playwright/node_modules/playwright",
  ].filter(Boolean);
  const exiger = createRequire(import.meta.url);
  for (const c of candidats) {
    if (existsSync(c)) return exiger(c);
  }
  throw new Error("Playwright introuvable (PLAYWRIGHT_CORE, node_modules, D:/LIFE/_tools/playwright)");
}

const { chromium } = chargerPlaywright();
const navigateur = await chromium.launch();
const page = await navigateur.newPage();
await page.setContent("<html><body></body></html>");

const sortie = await page.evaluate(
  async ({ svg, tailles }) => {
    const img = new Image();
    img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
    await img.decode();
    const resultat = {};
    for (const t of tailles) {
      const c = document.createElement("canvas");
      c.width = t;
      c.height = t;
      const g = c.getContext("2d");
      g.imageSmoothingQuality = "high";
      g.drawImage(img, 0, 0, t, t);
      resultat[t] = {
        png: c.toDataURL("image/png").split(",")[1],
        rgba: t === 64 ? Array.from(g.getImageData(0, 0, t, t).data) : null,
      };
    }
    // Planche de controle : chaque taille, a l'echelle 1, sur fond sombre puis clair.
    const planche = document.createElement("canvas");
    planche.width = 760;
    planche.height = 300;
    const p = planche.getContext("2d");
    p.fillStyle = "#808080";
    p.fillRect(0, 0, 760, 300);
    [["#1e1e1e", 10], ["#f3f3f3", 155]].forEach(([fond, y]) => {
      p.fillStyle = fond;
      p.fillRect(10, y, 740, 135);
      let x = 20;
      for (const t of [128, 64, 48, 32, 24, 20, 16]) {
        p.drawImage(img, x, y + (135 - t) / 2, t, t);
        x += t + 18;
      }
    });
    resultat.planche = planche.toDataURL("image/png").split(",")[1];
    return resultat;
  },
  { svg, tailles: TAILLES },
);
await navigateur.close();

for (const t of TAILLES) {
  writeFileSync(join(dossier, `trame-${t}.png`), Buffer.from(sortie[t].png, "base64"));
}
writeFileSync(join(racine, "ressources", "trame-64.rgba"), Buffer.from(sortie[64].rgba));
writeFileSync(join(dossier, "apercu.png"), Buffer.from(sortie.planche, "base64"));
console.log(`${TAILLES.length} PNG, trame-64.rgba (${sortie[64].rgba.length} octets), apercu.png`);

const ico = spawnSync(process.execPath, [join(ici, "faire-ico.mjs")], { stdio: "inherit" });
process.exit(ico.status ?? 1);
