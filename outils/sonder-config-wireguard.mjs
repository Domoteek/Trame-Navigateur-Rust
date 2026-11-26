#!/usr/bin/env node
/**
 * Sonde d'une configuration WireGuard : « est-ce que ce fichier perce ? »
 *
 * Prend un `.conf` de fournisseur (NordVPN, Mullvad, Proton…) et répond en une commande à la seule
 * question utile avant de toucher au profil de tous les jours : **le pair répond-il à la poignée de
 * main ?** Le fichier est importé dans un **profil jetable** (la clé y est chiffrée par DPAPI comme
 * dans l'usage normal), puis la sonde du tunnel (`TRAME_TUNNEL_ESSAI=1`) est lancée sur ce profil.
 * Le vrai profil n'est jamais ouvert.
 *
 * Un serveur WireGuard **ne répond jamais** à une clé qu'il ne connaît pas : une poignée de main
 * silencieuse veut dire que la configuration n'est pas provisionnée côté fournisseur (téléchargement
 * interrompu, clé révoquée, mauvais serveur), pas que le moteur de Trame est en cause. Le contrôle
 * qui le prouve : passer une configuration qui marche (Mullvad) donne « poignée de main en 20 ms ».
 *
 * Usage : node outils/sonder-config-wireguard.mjs [chemin.conf]
 *         (sans argument : la dernière configuration importée dans le profil réel)
 */
import { spawn, spawnSync, execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, statSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT = 19493;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const REEL = join(process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local"), "Trame");

/** Le dernier `.conf` arrivé dans Téléchargements, à défaut celui du profil réel. */
function trouverConf() {
  if (process.argv[2]) return process.argv[2];
  const telechargements = join(process.env.USERPROFILE || homedir(), "Downloads");
  if (!existsSync(telechargements)) return null;
  const recents = readdirSync(telechargements)
    .filter((f) => f.toLowerCase().endsWith(".conf"))
    .map((f) => join(telechargements, f))
    .map((f) => ({ f, m: statSync(f).mtimeMs }))
    .sort((a, b) => b.m - a.m);
  return recents[0] ? recents[0].f : null;
}

const conf = trouverConf();
if (!conf || !existsSync(conf)) {
  console.error("Aucun fichier .conf : indiquer son chemin en argument.");
  process.exit(2);
}

const profil = mkdtempSync(join(tmpdir(), "trame-sonde-conf-"));
console.log(`configuration : ${conf}`);
console.log(`profil jetable : ${profil}\n`);

const tuer = (pid) => {
  if (!pid) return;
  try {
    execFileSync("taskkill", ["/PID", String(pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
};

// 1. Import dans le profil jetable, par le même chemin que l'interface (IPC `tunnelImporter`).
const enfant = spawn(BINAIRE, [], {
  env: {
    ...process.env,
    TRAME_PROFIL: profil,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
  },
  stdio: "ignore",
});

/** Le dossier de la configuration importée dans le profil jetable (`tunnels/<id>/`). */
function dossierImporte() {
  const d = join(profil, "tunnels");
  if (!existsSync(d)) return null;
  for (const e of readdirSync(d, { withFileTypes: true })) {
    if (!e.isDirectory()) continue;
    const p = join(d, e.name);
    if (existsSync(join(p, "tunnel.json")) && existsSync(join(p, "tunnel.cle"))) return p;
  }
  return null;
}

let ecrit = false;
let dossier = null;
try {
  const cible = await attendreCible(PORT, "surface=rail");
  const rail = await connecter(cible.webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  await pause(1500);
  const message = JSON.stringify({ m: "tunnelImporter", chemin: conf });
  await rail.evaluer(`window.ipc.postMessage(${JSON.stringify(message)}); true`);
  for (let i = 0; i < 50; i++) {
    dossier = dossierImporte();
    if (dossier) {
      ecrit = true;
      break;
    }
    await pause(200);
  }
} catch (e) {
  console.error(`import impossible : ${e.message}`);
} finally {
  tuer(enfant.pid);
  await pause(800);
}

if (!ecrit) {
  console.error("\nECHEC : la configuration n'a pas ete acceptee par Trame (fichier refuse ou non ecrit).");
  console.error(`Voir le journal du profil jetable : ${join(profil, "trame.log")}`);
  process.exit(1);
}

// 2. Ce que Trame a retenu du fichier (partie publique seulement : jamais la clé).
const publique = JSON.parse(readFileSync(join(dossier, "tunnel.json"), "utf8"));
console.log("  adresses   :", (publique.adresses || []).join(", ") || "(aucune)");
console.log("  endpoint   :", publique.endpoint);
console.log("  dns        :", (publique.dns || []).join(", ") || "(aucun)");
console.log("  allowedIps :", (publique.allowedIps || []).join(", "));
console.log("  mtu        :", publique.mtu ?? "(defaut 1420)");
console.log("  keepalive  :", publique.keepalive ?? "(aucun)");
console.log(`\ncled privee : presente, chiffree (tunnel.cle ${readFileSync(join(dossier, "tunnel.cle")).length} octets)\n`);

// 3. La poignée de main, sur le vrai moteur.
console.log("poignee de main…");
const sonde = spawnSync(BINAIRE, [], {
  env: { ...process.env, TRAME_PROFIL: profil, TRAME_TUNNEL_ESSAI: "1" },
  encoding: "utf8",
  timeout: 60_000,
});
const sortie = `${sonde.stdout || ""}\n${sonde.stderr || ""}`;
const ligne = sortie
  .split(/\r?\n/)
  .filter((l) => l.includes("tunnel :"))
  .pop();
console.log(`\n${ligne || "(aucune ligne « tunnel : » – voir le journal du profil jetable)"}`);

const perce = Boolean(ligne && /poignée de main|poignee de main/.test(ligne) && !/ECHEC/.test(ligne));
console.log(
  perce
    ? "\nVERDICT : la configuration perce — le pair repond, elle peut etre importee dans le vrai profil."
    : "\nVERDICT : aucune reponse du pair. La configuration n'est pas provisionnee cote fournisseur" +
        " (telechargement interrompu, cle revoquee, ou mauvais serveur) — le moteur de Trame n'est pas" +
        " en cause si une configuration connue pour marcher repond, elle, en 20 ms."
);
console.log(`journal : ${join(profil, "trame.log")}`);
process.exit(perce ? 0 : 1);
