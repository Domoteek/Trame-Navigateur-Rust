#!/usr/bin/env node
/**
 * Banc d'essai du VPN INTÉGRÉ de bout en bout, sur le vrai binaire.
 *
 * Il répond à la seule question qui compte : **les pages sortent-elles par Mullvad ?** Il lance
 * Trame sur un profil d'essai qui contient une configuration WireGuard importée et le portier
 * allumé, ouvre `https://am.i.mullvad.net/json` (le service de Mullvad qui dit d'où l'on vient) et
 * lit la réponse DANS la page.
 *
 * Prérequis : les fichiers `tunnel.json` et `tunnel.cle` du profil réel (copiés par le banc), et un
 * accès réseau. Sans tunnel utilisable, le portier bloque — et le banc le dit.
 *
 * Usage : node outils/eprouver-vpn-bout-en-bout.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT = 19490;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const REEL = join(process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local"), "Trame");
const profil = mkdtempSync(join(tmpdir(), "trame-vpn-bout-"));
const JAUGE = "https://am.i.mullvad.net/json";

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

// Un profil d'essai : la configuration de Clément (chiffrée pour SON compte Windows, donc lisible
// ici), et le portier allumé.
for (const f of ["tunnel.json", "tunnel.cle"]) {
  const source = join(REEL, f);
  if (!existsSync(source)) {
    console.error(`Pas de configuration de tunnel dans ${REEL} : importe un .conf d'abord.`);
    process.exit(2);
  }
  cpSync(source, join(profil, f));
}
writeFileSync(join(profil, "reglages.json"), JSON.stringify({ tunnel_oblige: true }), "utf8");

const enfant = spawn(BINAIRE, [], {
  env: { ...process.env, TRAME_PROFIL: profil, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
  stdio: "ignore",
});
const tuer = () => {
  try {
    execFileSync("taskkill", ["/PID", String(enfant.pid)], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
};
const fini = async () => {
  for (let i = 0; i < 60; i++) {
    try {
      process.kill(enfant.pid, 0);
    } catch {
      return true;
    }
    await pause(250);
  }
  return false;
};

/** L'IP publique vue SANS tunnel, pour comparer. */
async function ipSansTunnel() {
  try {
    const r = await fetch("https://api.ipify.org?format=json");
    const j = await r.json();
    return j.ip;
  } catch {
    return "(inconnue)";
  }
}

try {
  const chezMoi = await ipSansTunnel();
  console.log(`IP sans tunnel : ${chezMoi}`);

  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  const barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(6000); // le tunnel demarre avant la premiere webview

  const journal = () => {
    try {
      return readFileSync(join(profil, "trame.log"), "utf8")
        .split(/\r?\n/)
        .filter((l) => /tunnel|mandataire/i.test(l))
        .slice(-10);
    } catch {
      return [];
    }
  };
  console.log("journal :", JSON.stringify(journal(), null, 0));

  // La page de contrôle de Mullvad, ouverte par la barre d'adresse (donc par le proxy).
  await barre.evaluer(`(() => {
    const c = document.getElementById("couture");
    c.focus();
    c.value = ${JSON.stringify(JAUGE)};
    c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
  })()`);
  await pause(12000);

  const pages = (await cibles(PORT)).filter((t) => t.type === "page" && !t.url.includes("surface="));
  console.log("pages :", JSON.stringify(pages.map((p) => p.url)));

  const cible = pages.find((p) => p.url.includes("mullvad"));
  if (!cible) {
    console.log("journal :", JSON.stringify(journal(), null, 0));
    ok(false, `la page de contrôle ne s'est pas ouverte (le portier a-t-il bloqué ?)`);
  } else {
    const page = await connecter(cible.webSocketDebuggerUrl);
    await page.envoyer("Runtime.enable");
    await pause(500);
    const texte = await page.evaluer(`document.body ? document.body.innerText : ""`);
    let ip = "(illisible)";
    let sortieMullvad = false;
    let serveur = "";
    try {
      const j = JSON.parse(texte);
      ip = j.ip || "(sans ip)";
      sortieMullvad = j.mullvad_exit_ip === true;
      serveur = j.mullvad_exit_ip_hostname || "";
    } catch {
      /* page d'erreur : on garde le texte pour le diagnostic */
    }
    console.log(`depuis Trame : ip=${ip} mullvad=${sortieMullvad} serveur=${serveur}`);
    ok(texte.trim().length > 0, "la page de contrôle a répondu à travers le tunnel");
    ok(sortieMullvad, `la sortie est bien un serveur Mullvad (${serveur || "inconnu"})`);
    ok(ip !== chezMoi, `l'IP n'est plus celle de la maison (${ip} ≠ ${chezMoi})`);
    page.fermer();
  }

  // Le vrai usage de Clément : PLUSIEURS onglets sur des hôtes différents, donc plusieurs
  // résolutions DNS en même temps. Défaut constaté le 2026-11-28 : beaucoup d'onglets ne se
  // chargeaient pas, parce qu'un seul nom pouvait être résolu à la fois.
  const sites = ["https://example.com/", "https://www.iana.org/help/example-domains"];
  const ouvrir = async (url) => {
    await rail.evaluer(`document.getElementById("btn-nouvel").click()`);
    await pause(1200);
    await barre.evaluer(`(() => {
      const c = document.getElementById("couture");
      c.focus();
      c.value = ${JSON.stringify(url)};
      c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
    })()`);
    await pause(9000);
    const cible = (await cibles(PORT)).find((t) => t.type === "page" && t.url.startsWith(url.replace(/\/$/, "")));
    if (!cible) return { url, titre: "" };
    const p = await connecter(cible.webSocketDebuggerUrl);
    await p.envoyer("Runtime.enable");
    await pause(400);
    const titre = await p.evaluer("document.title");
    p.fermer();
    return { url, titre };
  };

  for (const site of sites) {
    const resultat = await ouvrir(site);
    console.log(`onglet ${site} : titre ${JSON.stringify(resultat.titre)}`);
    ok(resultat.titre.trim().length > 0, `${site} se charge dans son propre onglet`);
  }

  barre.fermer();
  rail.fermer();
} finally {
  tuer();
  await fini();
  try {
    rmSync(profil, { recursive: true, force: true });
  } catch {
    /* le ménage de Windows reprendra */
  }
}

console.log(echecs === 0 ? "\nVPN intégré : les pages sortent par Mullvad." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
