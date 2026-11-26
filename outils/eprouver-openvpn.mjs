#!/usr/bin/env node
/**
 * Banc d'essai d'OpenVPN dans Trame, sur le vrai binaire.
 *
 * Il répond à la seule question qui compte pour un profil OpenVPN : **les pages sortent-elles par ce
 * tunnel ?** Il lance Trame sur un profil jetable, importe le `.ovpn` (avec les identifiants de
 * service si le profil les demande), arme le portier, redémarre, attend le montage, puis traverse le
 * mandataire SOCKS5 pour lire l'adresse de sortie.
 *
 * Ce qu'il sait dire :
 *   - « importé » : le moteur a lu le profil et Trame l'a rangé (chiffré) ;
 *   - « monté » : le serveur a poussé adresse, DNS et MTU, et le mandataire est en place ;
 *   - « la sortie est X » : l'adresse vue depuis le tunnel, à comparer à celle du poste ;
 *   - « refusé / non monté » : la raison exacte, telle que l'interface l'affiche.
 *
 * Usage :
 *   node outils/eprouver-openvpn.mjs --profil <fichier.ovpn> [--utilisateur U --motdepasse P]
 *   (sans --profil : la dernière configuration OpenVPN active du profil réel)
 *
 * Aucun identifiant n'est affiché : le banc dit seulement s'ils ont été fournis.
 */
import { spawn, spawnSync, execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT = 19494;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const REEL = join(process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local"), "Trame");
const JAUGE = "https://ipinfo.io/json";

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};
const note = (quoi) => console.log(`  note  ${quoi}`);

// --- arguments (jamais affichés : seul le nom du fichier l'est)
const args = process.argv.slice(2);
const valeur = (nom) => {
  const i = args.indexOf(`--${nom}`);
  return i >= 0 ? args[i + 1] : undefined;
};
const profilFourni = valeur("profil");
const utilisateur = valeur("utilisateur");
const motdepasse = valeur("motdepasse");

const profil = mkdtempSync(join(tmpdir(), "trame-openvpn-"));
const JOURNAL = join(profil, "trame.log");
const journal = () => (existsSync(JOURNAL) ? readFileSync(JOURNAL, "utf8") : "");
const vivant = (pid) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};
const tuerLancement = (enfant) => {
  if (!enfant) return;
  try {
    execFileSync("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
};

/** Le dossier de la configuration importée dans le profil jetable. */
function dossierImporte() {
  const d = join(profil, "tunnels");
  if (!existsSync(d)) return null;
  for (const e of readdirSync(d, { withFileTypes: true })) {
    if (e.isDirectory() && existsSync(join(d, e.name, "openvpn.json"))) return join(d, e.name);
  }
  return null;
}

let enfant = null;
const lancer = async () => {
  enfant = spawn(BINAIRE, [], {
    env: { ...process.env, TRAME_PROFIL: profil, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
    stdio: "ignore",
  });
  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  await pause(2500);
  return rail;
};

/** Ce que la page des réglages montre : une ligne par configuration, avec son résumé. */
const rangeesReglages = async (rail) => {
  await rail.evaluer(`(() => {
    const c = document.getElementById("couture");
    if (!c) return false;
    c.focus();
    c.value = "trame://localhost/reglages.html#confidentialite";
    c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    return true;
  })()`);
  await pause(2500);
  const cibles = await (await import("./cdp.mjs")).cibles(PORT);
  const cible = cibles.find((t) => t.type === "page" && t.url.includes("reglages.html"));
  if (!cible) return [];
  return cible;
};

try {
  // 1. Un profil : celui qu'on nous donne, ou celui du profil réel s'il y en a un d'actif.
  let texte = null;
  let chemin = null;
  if (profilFourni) {
    if (!existsSync(profilFourni)) throw new Error(`fichier introuvable : ${profilFourni}`);
    chemin = profilFourni;
    texte = readFileSync(chemin, "utf8");
  } else {
    const id = existsSync(join(REEL, "tunnels", "actif.txt"))
      ? readFileSync(join(REEL, "tunnels", "actif.txt"), "utf8").trim()
      : "";
    const f = join(REEL, "tunnels", id, "profil.cle");
    if (!id || !existsSync(f)) {
      console.error("Aucun profil OpenVPN : passer --profil <fichier.ovpn> (et les identifiants).");
      process.exit(2);
    }
    note(`profil réel détecté : ${id}`);
  }
  if (texte !== null) {
    const lignes = texte.split(/\r?\n/).length;
    console.log(`\nprofil : ${chemin} (${lignes} lignes)`);
    console.log(`identifiants fournis : ${utilisateur ? "oui" : "non"}`);
  }

  const rail = await lancer();

  // 2. Import par le message de l'interface (le même que le bouton « Importer… »).
  const message = { m: "tunnelImporter" };
  if (chemin) message.chemin = chemin;
  if (utilisateur) {
    message.utilisateur = utilisateur;
    message.motdepasse = motdepasse || "";
  }
  await rail.evaluer(`window.ipc.postMessage(${JSON.stringify(JSON.stringify(message))}); true`);
  let dossier = null;
  for (let i = 0; i < 100; i++) {
    dossier = dossierImporte();
    if (dossier) break;
    await pause(200);
  }
  ok(Boolean(dossier), "le profil est accepté et rangé (chiffré)");
  if (dossier) {
    const publics = JSON.parse(readFileSync(join(dossier, "openvpn.json"), "utf8"));
    console.log(
      `  rangé : ${publics.nom} — ${publics.protocole} ${publics.serveur}:${publics.port}` +
        ` · identifiants ${publics.autologin ? "non requis" : "requis"}`
    );
    const chiffre = readFileSync(join(dossier, "profil.cle"));
    ok(!chiffre.includes(Buffer.from("remote ")), "le profil n'est pas en clair sur le disque");
    ok(
      !existsSync(join(dossier, "identifiants.cle")) || !readFileSync(join(dossier, "identifiants.cle")).includes(Buffer.from(motdepasse || "\u0000")),
      "les identifiants ne sont pas en clair"
    );
  }
  if (!dossier) {
    const refus = journal().split(/\r?\n/).filter((l) => l.includes("OpenVPN refus")).pop();
    console.log(`\n  raison : ${refus || "(voir le journal ci-dessous)"}`);
    console.log(`\n${echecs} contrôle(s) en échec — le profil n'a pas été accepté.`);
    process.exit(1);
  }

  // 3. Portier armé, puis redémarrage : le tunnel OpenVPN doit monter.
  await rail.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "tunnelOblige", actif: true })); true`);
  await pause(1500);
  tuerLancement(enfant);
  await pause(1500);
  const rail2 = await lancer();

  // Attention : `journal()` est synchrone. Une version `async` de ce prédicat rendait une promesse,
  // donc toujours vraie — le banc annonçait « monté » sur un profil qui échouait (constaté le
  // 2026-11-28, corrigé).
  const monte = () => journal().split(/\r?\n/).some((l) => l.includes("openvpn : tunnel monté"));
  const echoue = () => journal().includes("démarrage impossible");
  let monteVrai = false;
  for (let i = 0; i < 200; i++) {
    if (monte()) {
      monteVrai = true;
      break;
    }
    if (echoue()) break;
    await pause(250);
  }
  ok(monteVrai, "le tunnel OpenVPN monte");
  if (!monteVrai) {
    const raison = journal()
      .split(/\r?\n/)
      .filter((l) => l.includes("démarrage impossible") || l.includes("tunnel : ECHEC"))
      .pop();
    console.log(`\n  raison : ${raison || "(le moteur n'a rien dit — voir le journal)"}`);
    console.log(`  journal : ${JOURNAL}`);
    console.log(`\n${echecs} contrôle(s) en échec — le tunnel n'a pas monté.`);
    process.exit(1);
  }

  // 4. La sortie réelle, à travers le mandataire SOCKS5.
  await pause(2000);
  const proxy = (journal().match(/proxy socks5:\/\/([0-9.]+:\d+)/g) || []).pop()?.replace("proxy socks5://", "");
  ok(Boolean(proxy), `le mandataire est posé (${proxy || "aucun"})`);
  if (proxy) {
    const resultat = spawnSync("curl.exe", ["-s", "--max-time", "30", "--socks5-hostname", proxy, JAUGE], {
      encoding: "utf8",
    });
    let vu = null;
    try {
      vu = JSON.parse(resultat.stdout || "{}");
    } catch {
      vu = null;
    }
    ok(Boolean(vu && vu.ip), `la page de contrôle répond (${(resultat.stdout || "").slice(0, 80)}…)`);
    if (vu && vu.ip) {
      console.log(`\n  adresse de sortie : ${vu.ip} · ${vu.country || "?"} · ${vu.org || vu.city || ""}`);
      console.log("  (à comparer à l'adresse du poste : elle doit être différente)");
    }
  }

  rail2.fermer?.();
} catch (e) {
  console.error(`\nECHEC du banc : ${e.message}`);
  echecs += 1;
} finally {
  tuerLancement(enfant);
  await pause(800);
  if (process.env.TRAME_BANC_GARDE) {
    console.log("  journal gardé : " + JOURNAL);
  } else {
    try {
      rmSync(profil, { recursive: true, force: true });
    } catch {
      /* le ménage de Windows reprendra */
    }
  }
}

console.log(echecs === 0 ? "\nOpenVPN : le profil perce." : `\n${echecs} contrôle(s) en échec.`);
process.exit(echecs === 0 ? 0 : 1);
