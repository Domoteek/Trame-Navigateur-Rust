#!/usr/bin/env node
/**
 * Banc du mandataire SOCKS5 sous charge, sur le vrai binaire et le vrai tunnel.
 *
 * Question : quand le VPN est monté, les sites s'ouvrent-ils TOUS, même demandés en même temps ?
 * (Défaut du 2026-11-26 : Google, Amazon, GitHub injoignables — même port local pour toutes les
 * connexions, sockets DNS qui se volaient les réponses.)
 *
 * Il copie la configuration de tunnel ACTIVE du profil réel (chiffrée pour ce compte Windows) dans
 * un profil jetable, lance Trame caché, attend le montage, puis envoie des requêtes HTTPS en
 * parallèle (plusieurs par site) à travers le mandataire, et compte celles qui répondent.
 *
 * Usage : node outils/eprouver-mandataire-sites.mjs [--binaire chemin\trame.exe] [--tours 2]
 */
import { spawn, execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

const args = process.argv.slice(2);
const valeur = (nom, defaut) => {
  const i = args.indexOf(`--${nom}`);
  return i >= 0 ? args[i + 1] : defaut;
};
const BINAIRE = valeur("binaire", process.env.TRAME_BINAIRE || join(process.cwd(), "target", "release", "trame.exe"));
const TOURS = Number(valeur("tours", "2"));
const REEL = join(process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local"), "Trame");
const pause = (ms) => new Promise((r) => setTimeout(r, ms));

const SITES = [
  "https://www.google.com/",
  "https://www.google.fr/search?q=meteo",
  "https://www.gstatic.com/generate_204",
  "https://www.youtube.com/",
  "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg",
  "https://www.amazon.fr/",
  "https://m.media-amazon.com/images/G/08/gno/sprites/nav-sprite-global-1x-reorg-privacy._CB546805360_.png",
  "https://github.com/",
  "https://api.github.com/",
  "https://www.wikipedia.org/",
  "https://www.lemonde.fr/",
  "https://www.leboncoin.fr/",
  "https://www.cloudflare.com/",
  "https://www.microsoft.com/",
  "https://ipinfo.io/json",
];
const PAR_SITE = 3;

const id = existsSync(join(REEL, "tunnels", "actif.txt")) ? readFileSync(join(REEL, "tunnels", "actif.txt"), "utf8").trim() : "";
if (!id || !existsSync(join(REEL, "tunnels", id))) {
  console.error("Aucune configuration de tunnel active dans le profil réel.");
  process.exit(2);
}

const profil = mkdtempSync(join(tmpdir(), "trame-mandataire-"));
cpSync(join(REEL, "tunnels", id), join(profil, "tunnels", id), { recursive: true });
writeFileSync(join(profil, "tunnels", "actif.txt"), id);
writeFileSync(join(profil, "reglages.json"), JSON.stringify({ tunnel_oblige: true, astuces: false }, null, 2));
const JOURNAL = join(profil, "trame.log");
const journal = () => (existsSync(JOURNAL) ? readFileSync(JOURNAL, "utf8") : "");

console.log(`binaire : ${BINAIRE}`);
console.log(`tunnel  : ${id}`);
const enfant = spawn(BINAIRE, [], { env: { ...process.env, TRAME_PROFIL: profil, TRAME_CACHEE: "1" }, stdio: "ignore" });

let echecs = 0;
try {
  let proxy = null;
  for (let i = 0; i < 240 && !proxy; i++) {
    const m = journal().match(/écoute SOCKS5 sur (127\.0\.0\.1:\d+)/);
    if (m) proxy = m[1];
    if (/démarrage impossible|tunnel : ECHEC/.test(journal())) break;
    await pause(250);
  }
  if (!proxy) throw new Error("le tunnel n'a pas monté (voir " + JOURNAL + ")");
  console.log(`mandataire : ${proxy}\n`);
  await pause(1500);

  const unAppel = (url) =>
    new Promise((resolve) => {
      const debut = Date.now();
      const c = spawn("curl.exe", ["-s", "-o", "NUL", "-w", "%{http_code}", "--max-time", "25", "--socks5-hostname", proxy, url]);
      let sortie = "";
      c.stdout.on("data", (d) => (sortie += d));
      c.on("close", () => resolve({ url, code: sortie.trim() || "000", ms: Date.now() - debut }));
    });

  let total = 0;
  let bons = 0;
  for (let tour = 1; tour <= TOURS; tour++) {
    const demandes = SITES.flatMap((u) => Array.from({ length: PAR_SITE }, () => u));
    const resultats = await Promise.all(demandes.map(unAppel));
    const parSite = new Map();
    for (const r of resultats) {
      const s = parSite.get(r.url) || { ok: 0, codes: [], ms: 0 };
      if (r.code !== "000") s.ok += 1;
      s.codes.push(r.code);
      s.ms = Math.max(s.ms, r.ms);
      parSite.set(r.url, s);
    }
    console.log(`tour ${tour} (${demandes.length} requêtes en même temps)`);
    for (const [url, s] of parSite) {
      const hote = new URL(url).host;
      console.log(`  ${s.ok === PAR_SITE ? "ok  " : "ECHEC"} ${hote.padEnd(24)} ${s.ok}/${PAR_SITE}  [${s.codes.join(" ")}]  ${s.ms} ms`);
    }
    const t = resultats.filter((r) => r.code !== "000").length;
    total += resultats.length;
    bons += t;
    console.log("");
  }
  echecs = total - bons;
  console.log(`${bons}/${total} requêtes ont abouti.`);

  // Un gros fichier : aucun octet ne doit se perdre en route (une vidéo, c'est ça).
  const OCTETS = 30_000_000;
  const gros = await new Promise((resolve) => {
    const debut = Date.now();
    const c = spawn("curl.exe", ["-s", "-o", "NUL", "-w", "%{size_download}", "--max-time", "90", "--socks5-hostname", proxy, `https://speed.cloudflare.com/__down?bytes=${OCTETS}`]);
    let sortie = "";
    c.stdout.on("data", (d) => (sortie += d));
    c.on("close", () => resolve({ taille: Number(sortie.trim() || 0), ms: Date.now() - debut }));
  });
  const debit = ((gros.taille * 8) / 1e6 / (gros.ms / 1000)).toFixed(1);
  const entier = gros.taille === OCTETS;
  if (!entier) echecs += 1;
  console.log(`${entier ? "ok  " : "ECHEC"} gros fichier : ${gros.taille}/${OCTETS} octets en ${gros.ms} ms (${debit} Mbit/s)`);
  const lignesDns = journal().split(/\r?\n/).filter((l) => /DNS du tunnel|ne répond pas|refusé/.test(l));
  if (lignesDns.length) console.log(`\njournal (${lignesDns.length} ligne(s) d'échec), les dernières :\n  ` + lignesDns.slice(-8).join("\n  "));
} catch (e) {
  console.error(`ECHEC du banc : ${e.message}`);
  echecs += 1;
} finally {
  try {
    execFileSync("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    /* déjà parti */
  }
  await pause(1000);
  if (!process.env.TRAME_BANC_GARDE) {
    try {
      rmSync(profil, { recursive: true, force: true });
    } catch {
      /* Windows fera le ménage */
    }
  } else console.log("profil gardé : " + profil);
}
process.exit(echecs === 0 ? 0 : 1);
