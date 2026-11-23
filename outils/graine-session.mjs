#!/usr/bin/env node
// Genere une session de demonstration pour verifier le rail, la trame et la veille.
// Usage : node outils/graine-session.mjs <dossier-profil>
//
// Les horodatages sont calcules par rapport a maintenant : les ages affiches et l'axe
// de temps de la vue Trame doivent rester credibles quelle que soit la date d'execution.

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const profil = process.argv[2];
if (!profil) {
  console.error("usage : node outils/graine-session.mjs <dossier-profil>");
  process.exit(1);
}

const maintenant = Date.now();
const minute = 60_000;

/** @type {{id:number,nom:string,auto:boolean,cree:number}[]} */
const fils = [
  { id: 1, nom: "Batteries lithium", auto: false, cree: maintenant - 130 * minute },
  { id: 2, nom: "Rust async", auto: false, cree: maintenant - 95 * minute },
  { id: 3, nom: "Home lab", auto: false, cree: maintenant - 60 * minute },
  { id: 4, nom: "Divers", auto: true, cree: maintenant - 20 * minute },
];

const brut = [
  // fil 1 — une exploration technique, avec filiation
  ["https://fr.wikipedia.org/wiki/Lithium", "Lithium — Wikipédia", 1, null, 124, 0.86, 0.96, 4],
  ["https://batteryuniversity.com/article/bu-204-how-do-lithium-ion-batteries-work", "BU-204 : How do lithium-ion batteries work?", 1, 1, 111, 0.62, 0.78, 2],
  ["https://fr.wikipedia.org/wiki/Accumulateur_lithium-ion", "Accumulateur lithium-ion — Wikipédia", 1, 2, 98, 0.0, 0.44, 1],
  ["https://www.cea.fr/comprendre/Pages/energies/nucleaire/essentiel-sur-le-lithium.aspx", "Le lithium, un métal stratégique", 1, 3, 84, 0.0, 0.21, 0],

  // fil 2 — la programmation
  ["https://doc.rust-lang.org/book/ch17-00-async-await.html", "Async et await — Le langage Rust", 2, null, 88, 1.0, 0.72, 3],
  ["https://tokio.rs/tokio/tutorial", "Tutoriel Tokio", 2, 5, 74, 0.0, 0.30, 1],
  ["https://blog.rust-lang.org/2026/01/12/Rust-1.93.0.html", "Annonce de Rust 1.93", 2, null, 61, 0.0, 0.18, 0],

  // fil 3 — la maison
  ["https://192.0.2.5:8006/#v1:0:=node/pve", "Proxmox — nœud pve", 3, null, 44, 0.0, 0.55, 2],
  ["https://192.0.2.1/", "OPNsense", 3, null, 33, 0.0, 0.12, 1],
  ["https://ha.domo-srv.ovh/lovelace/0", "Home Assistant — Vue d'ensemble", 3, null, 21, 0.0, 0.09, 0],

  // fil 4
  ["https://news.ycombinator.com/", "Hacker News", 4, null, 9, 0.0, 0.41, 1],
];

const onglets = brut.map((ligne, index) => {
  const [url, titre, fil, parent, ageMin, vitalite, defilement, nbFocus] = ligne;
  return {
    id: index + 1,
    url,
    titre,
    fil,
    parent: parent ?? null,
    declencheur: parent ? "lien" : index === 0 ? "nouvelonglet" : "saisie",
    cree_le: maintenant - ageMin * minute,
    dernier_focus: maintenant - Math.max(1, ageMin - 5) * minute,
    nb_focus: nbFocus,
    temps_actif_ms: nbFocus * 42_000,
    defilement,
    ancre: index === 4,
    a_lire: index === 2,
    vitalite,
  };
});

const session = {
  onglets,
  fils: fils.map((f) => ({
    id: f.id,
    nom: f.nom,
    auto_nom: f.auto,
    cree_le: f.cree,
    actif_le: maintenant - 5 * minute,
    replie: false,
  })),
  actif: 1,
  rail_replie: false,
  prochain_onglet: onglets.length + 1,
  prochain_fil: fils.length + 1,
};

mkdirSync(profil, { recursive: true });
const chemin = join(profil, "session.json");
writeFileSync(chemin, JSON.stringify(session, null, 2), "utf8");
console.log(`session de demonstration ecrite : ${chemin} (${onglets.length} onglets, ${fils.length} fils)`);
