#!/usr/bin/env node
// Fabrique ressources/trame.ico a partir des PNG de ressources/icones/.
//
// Le format ICO est simple : un en-tete, une entree de repertoire par image, puis les donnees.
// Windows (Vista et suivants) accepte des images PNG directement dans le .ico — inutile de
// fabriquer des DIB avec masque AND, ce qui evite une bonne centaine de lignes de bits.
//
// Usage : node outils/faire-ico.mjs

import { readFileSync, writeFileSync, readdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, "..");
const dossier = join(racine, "ressources", "icones");
const sortie = join(racine, "ressources", "trame.ico");

/** Tailles retenues : celles que Windows utilise reellement (barre des taches, alt-tab,
 *  explorateur, apercu HD). 20, 40 et 96 couvrent les mises a l'echelle a 125 % et 150 %. */
const TAILLES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256];

const images = [];
for (const taille of TAILLES) {
  const chemin = join(dossier, `trame-${taille}.png`);
  let donnees;
  try {
    donnees = readFileSync(chemin);
  } catch {
    console.warn(`  absent : ${chemin}`);
    continue;
  }
  images.push({ taille, donnees });
}

if (!images.length) {
  console.error("aucune image : rien a faire");
  process.exit(1);
}

const entete = Buffer.alloc(6);
entete.writeUInt16LE(0, 0); // reserve
entete.writeUInt16LE(1, 2); // type 1 = icone
entete.writeUInt16LE(images.length, 4);

const repertoire = Buffer.alloc(16 * images.length);
let decalage = entete.length + repertoire.length;

images.forEach((image, index) => {
  const base = index * 16;
  // La largeur/hauteur 0 signifie 256 dans le format ICO.
  repertoire.writeUInt8(image.taille >= 256 ? 0 : image.taille, base);
  repertoire.writeUInt8(image.taille >= 256 ? 0 : image.taille, base + 1);
  repertoire.writeUInt8(0, base + 2); // nombre de couleurs (0 = vraies couleurs)
  repertoire.writeUInt8(0, base + 3); // reserve
  repertoire.writeUInt16LE(1, base + 4); // plans
  repertoire.writeUInt16LE(32, base + 6); // bits par pixel
  repertoire.writeUInt32LE(image.donnees.length, base + 8);
  repertoire.writeUInt32LE(decalage, base + 12);
  decalage += image.donnees.length;
});

writeFileSync(sortie, Buffer.concat([entete, repertoire, ...images.map((i) => i.donnees)]));

// Relecture de controle : on ne declare pas un fichier valide sans le relire.
const relu = readFileSync(sortie);
const nombre = relu.readUInt16LE(4);
const detail = [];
for (let i = 0; i < nombre; i += 1) {
  const base = 6 + i * 16;
  const largeur = relu.readUInt8(base) || 256;
  const octets = relu.readUInt32LE(base + 8);
  const position = relu.readUInt32LE(base + 12);
  const signature = relu.subarray(position, position + 4).toString("hex");
  if (signature !== "89504e47") throw new Error(`image ${largeur} n'est pas un PNG`);
  detail.push(`${largeur}px/${octets}o`);
}
console.log(`ecrit : ${sortie} (${relu.length} octets, ${nombre} images)`);
console.log("  " + detail.join("  "));
