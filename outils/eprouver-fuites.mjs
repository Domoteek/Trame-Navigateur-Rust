#!/usr/bin/env node
/**
 * Banc anti-fuite du VPN intégré, sur le vrai binaire et le vrai tunnel.
 *
 * Deux questions, à poser avant toute publication qui touche au tunnel :
 *   1. Avec le tunnel monté, une page peut-elle apprendre la VRAIE adresse du poste ?
 *      - par HTTP (l'adresse vue par un site) ;
 *      - par WebRTC (STUN) — fuite réelle constatée le 2026-11-26 en 0.4.93, corrigée en 0.4.94 par
 *        `--webrtc-ip-handling-policy=disable_non_proxied_udp` (WebView2 ignore le nom de Chrome,
 *        `--force-webrtc-ip-handling-policy`).
 *   2. Si le tunnel tombe : rien ne sort pendant la coupure, puis il revient seul, sur le même
 *      port, sans redémarrer Trame (coupure provoquée par `TRAME_SONDE_COUPURE_S`).
 *
 * Il copie la configuration de tunnel indiquée (par défaut l'active) du profil réel dans un profil
 * jetable et lance Trame caché. Attention : ne pas l'employer sur une configuration WireGuard que
 * Trame utilise au même moment (une même clé sur deux pairs se dispute la session) ; une
 * configuration OpenVPN ne pose pas ce souci.
 *
 * Usage : node outils/eprouver-fuites.mjs [--tunnel <id du dossier dans tunnels\>] [--sans-coupure]
 */
import { spawn, spawnSync, execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

import { cibles, connecter, pause } from "./cdp.mjs";

const args = process.argv.slice(2);
const valeur = (nom) => {
  const i = args.indexOf(`--${nom}`);
  return i >= 0 ? args[i + 1] : undefined;
};
const PORT = 19498;
const COUPURE_S = 35;
const BINAIRE = process.env.TRAME_BINAIRE || join(process.cwd(), "target", "release", "trame.exe");
const REEL = join(process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local"), "Trame");
const avecCoupure = !args.includes("--sans-coupure");

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

const actif = existsSync(join(REEL, "tunnels", "actif.txt")) ? readFileSync(join(REEL, "tunnels", "actif.txt"), "utf8").trim() : "";
const id = valeur("tunnel") || actif;
if (!id || !existsSync(join(REEL, "tunnels", id))) {
  console.error("Aucune configuration de tunnel : passer --tunnel <dossier>.");
  process.exit(2);
}

const profil = mkdtempSync(join(tmpdir(), "trame-fuites-"));
cpSync(join(REEL, "tunnels", id), join(profil, "tunnels", id), { recursive: true });
writeFileSync(join(profil, "tunnels", "actif.txt"), id);
writeFileSync(join(profil, "reglages.json"), JSON.stringify({ tunnel_oblige: true, astuces: false }, null, 2));
const JOURNAL = join(profil, "trame.log");
const journal = () => (existsSync(JOURNAL) ? readFileSync(JOURNAL, "utf8") : "");
const attendre = async (motif, ms) => {
  for (let i = 0; i < ms / 250; i++) {
    if (motif.test(journal())) return true;
    await pause(250);
  }
  return false;
};

const ipDirecte = spawnSync("curl.exe", ["-s", "--max-time", "15", "https://api.ipify.org"], { encoding: "utf8" }).stdout.trim();
console.log(`binaire : ${BINAIRE}\ntunnel  : ${id}\nadresse réelle du poste : ${ipDirecte || "(inconnue)"}\n`);
if (!ipDirecte) {
  console.error("Impossible de lire l'adresse réelle : le banc ne pourrait rien comparer.");
  process.exit(2);
}

const env = {
  ...process.env,
  TRAME_PROFIL: profil,
  TRAME_CACHEE: "1",
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
};
if (avecCoupure) env.TRAME_SONDE_COUPURE_S = String(COUPURE_S);
const enfant = spawn(BINAIRE, [], { env, stdio: "ignore" });

const parProxy = (proxy, url = "https://api.ipify.org") => {
  const debut = Date.now();
  const r = spawnSync("curl.exe", ["-s", "--max-time", "20", "--socks5-hostname", proxy, url], { encoding: "utf8" });
  return { sortie: (r.stdout || "").trim(), ms: Date.now() - debut };
};

try {
  ok(await attendre(/écoute SOCKS5/, 60_000), "le tunnel monte");
  const proxy = (journal().match(/écoute SOCKS5 sur (127\.0\.0\.1:\d+)/) || [])[1];
  await pause(3000);

  // Une page ordinaire (le Nouvel onglet est interne : on la fait naviguer dehors).
  let cible = null;
  for (let i = 0; i < 40 && !cible; i++) {
    cible = (await cibles(PORT)).find((c) => c.type === "page" && !/surface=/.test(c.url));
    if (!cible) await pause(500);
  }
  if (!cible) throw new Error("aucun onglet ouvert");
  const page = await connecter(cible.webSocketDebuggerUrl);
  await page.envoyer("Runtime.enable");
  await page.envoyer("Page.navigate", { url: "https://api.ipify.org/?format=json" });
  await pause(6000);
  let vu = "";
  try {
    vu = JSON.parse(await page.evaluer(`document.body ? document.body.innerText : ""`)).ip || "";
  } catch {
    vu = "";
  }
  console.log("\n1. Fuites, tunnel monté");
  ok(Boolean(vu), `une page joint Internet (${vu || "rien"})`);
  ok(vu && vu !== ipDirecte, "par HTTP, la page voit l'adresse du VPN, pas la vraie");

  const candidats = await page.evaluer(`new Promise((fin) => {
    const vus = [];
    let pc;
    try { pc = new RTCPeerConnection({ iceServers: [{ urls: "stun:stun.l.google.com:19302" }, { urls: "stun:stun.cloudflare.com:3478" }] }); }
    catch (e) { fin(["(RTCPeerConnection indisponible : " + e.message + ")"]); return; }
    pc.createDataChannel("sonde");
    pc.onicecandidate = (e) => { if (e.candidate) vus.push(e.candidate.candidate); else fin(vus); };
    pc.createOffer().then((o) => pc.setLocalDescription(o));
    setTimeout(() => fin(vus), 8000);
  })`);
  const reflexifs = (candidats || []).filter((c) => / typ srflx /.test(c));
  console.log(`  ${candidats.length} candidat(s) WebRTC, dont ${reflexifs.length} réflexif(s)`);
  ok(!(candidats || []).some((c) => c.includes(ipDirecte)), "par WebRTC, la vraie adresse n'apparaît pas");
  ok(reflexifs.length === 0, "WebRTC ne sort pas en UDP à côté du tunnel (aucun candidat STUN)");

  if (avecCoupure) {
    console.log(`\n2. Coupure du tunnel (provoquée ${COUPURE_S} s après le montage)`);
    ok(await attendre(/tunnel coupé/, (COUPURE_S + 20) * 1000), "le mandataire voit la coupure");
    const pendant = parProxy(proxy);
    ok(pendant.sortie === "" && pendant.ms < 5000, `pendant la coupure rien ne sort, et vite (${pendant.ms} ms, « ${pendant.sortie} »)`);
    ok(await attendre(/toast : Tunnel coupé/, 15_000), "Trame le dit (message « Tunnel coupé »)");
    ok(await attendre(/tunnel rétabli/, 60_000), "le tunnel revient seul");
    await pause(1500);
    const apres = parProxy(proxy);
    ok(apres.sortie && apres.sortie !== ipDirecte, `après la reprise, même port, les pages repassent (${apres.sortie || "rien"})`);
    ok(await attendre(/toast : Tunnel rétabli/, 15_000), "Trame le dit (message « Tunnel rétabli »)");
    await page.envoyer("Page.reload", {});
    await pause(5000);
    const reVu = await page.evaluer(`document.body ? document.body.innerText : ""`);
    ok(reVu.includes(".") && !reVu.includes(ipDirecte), `la page se recharge par le tunnel (${reVu.slice(0, 40)})`);
  }
} catch (e) {
  console.error(`\nECHEC du banc : ${e.message}`);
  echecs += 1;
} finally {
  try {
    execFileSync("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    /* déjà parti */
  }
  await pause(1000);
  if (process.env.TRAME_BANC_GARDE) console.log("profil gardé : " + profil);
  else {
    try {
      rmSync(profil, { recursive: true, force: true });
    } catch {
      /* Windows fera le ménage */
    }
  }
}
console.log(echecs === 0 ? "\nAucune fuite, et le tunnel se remonte seul." : `\n${echecs} contrôle(s) en échec.`);
process.exit(echecs === 0 ? 0 : 1);
