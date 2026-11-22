#!/usr/bin/env node
// Ajoute un fil d'onglets a une session Trame existante.
//
// Usage : node outils/ajouter-fil.mjs <dossier-profil> <fichier.json>
//
// Le fichier JSON decrit le fil :
//   { "nom": "Home lab", "onglets": [ { "url": "...", "titre": "..." }, ... ] }
//
// PRECAUTION : l'application ecrit session.json a l'arret et toutes les 3 s. Il faut donc la
// fermer avant de lancer cet outil, sinon la session sera ecrasee au redemarrage.
//
// Les onglets ajoutes sont crees EN VEILLE (aucune webview) : un fil de 13 services coute
// zero memoire tant qu'on n'en ouvre pas un.

import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const [profil, fichier] = process.argv.slice(2);
if (!profil || !fichier) {
  console.error("usage : node outils/ajouter-fil.mjs <dossier-profil> <fichier.json>");
  process.exit(1);
}

const cheminSession = join(profil, "session.json");
const session = existsSync(cheminSession)
  ? JSON.parse(readFileSync(cheminSession, "utf8"))
  : { onglets: [], fils: [], actif: null, rail_replie: false, prochain_onglet: 1, prochain_fil: 1 };

const description = JSON.parse(readFileSync(fichier, "utf8"));
const onglets = Array.isArray(description) ? description : description.onglets || [];
const nom = (Array.isArray(description) ? "Fil" : description.nom) || "Fil";

if (!onglets.length) {
  console.error("aucun onglet dans le fichier");
  process.exit(1);
}

const maintenant = Date.now();
let idOnglet = Math.max(session.prochain_onglet || 0, ...session.onglets.map((o) => o.id)) + 1;
const idFil = Math.max(session.prochain_fil || 0, ...session.fils.map((f) => f.id)) + 1;

// Un fil deja present portant le meme nom est remplace : deux « Home lab » seraient un piege.
const existant = session.fils.find((f) => f.nom === nom);
const idFilUtilise = existant ? existant.id : idFil;
if (existant) {
  session.onglets = session.onglets.filter((o) => o.fil !== existant.id);
  session.fils = session.fils.filter((f) => f.id !== existant.id);
  console.log(`  fil existant « ${nom} » retire (${idFilUtilise}), il sera recree`);
}

const nouveaux = onglets.map((o) => {
  const onglet = {
    id: idOnglet,
    url: o.url,
    titre: o.titre || "",
    fil: idFilUtilise,
    parent: null,
    declencheur: "saisie",
    cree_le: maintenant,
    dernier_focus: maintenant,
    nb_focus: 0,
    temps_actif_ms: 0,
    defilement: 0,
    ancre: false,
    a_lire: false,
    // 0,5 : au-dessus du seuil de veille, mais froid — ils seront endormis des le demarrage.
    vitalite: 0.5,
  };
  idOnglet += 1;
  return onglet;
});

session.fils.push({
  id: idFilUtilise,
  nom,
  auto_nom: false,
  cree_le: maintenant,
  actif_le: maintenant,
  replie: false,
});
session.onglets.push(...nouveaux);
session.prochain_onglet = idOnglet;
session.prochain_fil = Math.max(idFil, idFilUtilise) + 1;

writeFileSync(cheminSession, JSON.stringify(session, null, 2), "utf8");

console.log(`fil « ${nom} » ecrit : ${nouveaux.length} onglet(s), id ${idFilUtilise}`);
for (const o of nouveaux) console.log(`  ${o.id}  ${o.url}`);
console.log(`session : ${session.onglets.length} onglet(s), ${session.fils.length} fil(s) -> ${cheminSession}`);
