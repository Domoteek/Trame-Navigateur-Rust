#!/usr/bin/env node
/**
 * Banc d'essai du PORTIER « TUNNEL OBLIGATOIRE », sur le vrai binaire.
 *
 * Choix de Clément (2026-11-27) : quand ce réglage est allumé et qu'aucun tunnel (VPN) n'est monté,
 * Trame **refuse** d'ouvrir un site et affiche une page qui explique — plutôt que de laisser passer
 * le trafic en clair à son insu.
 *
 * Vérifie, sur cette machine (où aucun tunnel n'est monté — mesuré) :
 *   1. avec le portier allumé, une adresse de site n'est PAS chargée : la page du tunnel s'affiche et
 *      aucun document du site n'apparaît (rien n'est parti sur le réseau) ;
 *   2. une page interne (les réglages) reste joignable — sinon on ne pourrait plus éteindre le portier ;
 *   3. portier éteint depuis les réglages, la même adresse s'ouvre normalement.
 *
 * Usage : node outils/eprouver-tunnel.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const PORT = 19486;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-tunnel-"));
const ADRESSE = "https://example.com";
const PAGE_REGLAGES = "trame://localhost/reglages.html";

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

// Le portier est allumé avant le lancement : c'est l'etat qu'on veut eprouver.
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

/** Les pages ouvertes (hors surfaces), telles que le moteur les voit. */
const pages = async () =>
  (await cibles(PORT))
    .filter((t) => t.type === "page" && !t.url.includes("surface=") && !t.url.startsWith("devtools"))
    .map((t) => t.url);

/** Les dernieres lignes utiles du journal (diagnostic quand une etape ne fait pas ce qu'on croit). */
const journal = async (motif) => {
  try {
    const { readFileSync } = await import("node:fs");
    return readFileSync(join(profil, "trame.log"), "utf8")
      .split(/\r?\n/)
      .filter((l) => new RegExp(motif, "i").test(l))
      .slice(-6);
  } catch {
    return ["(journal illisible)"];
  }
};

try {
  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  const barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000);

  const saisir = async (valeur) => {
    await barre.evaluer(`(() => {
      const c = document.getElementById("couture");
      c.focus();
      c.value = ${JSON.stringify(valeur)};
      c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
    })()`);
    await pause(3500);
  };

  // 1. Portier allumé : l'adresse du site n'est pas chargée.
  await saisir(ADRESSE);
  const apres1 = await pages();
  console.log("\n1. portier allumé, adresse saisie :", JSON.stringify(apres1));
  for (const l of await journal("tunnel|portier|navigu|reglages")) console.log("   " + l);
  ok(
    apres1.some((u) => u.includes("tunnel.html")),
    `la page du tunnel s'affiche (${JSON.stringify(apres1)})`
  );
  ok(
    !apres1.some((u) => u.includes("example.com") && !u.includes("tunnel.html")),
    "aucun document du site n'a été chargé (rien n'est parti sur le réseau)"
  );

  // 2. Une page interne reste joignable : les réglages.
  await saisir(PAGE_REGLAGES);
  const apres2 = await pages();
  console.log("2. réglages demandés :", JSON.stringify(apres2));
  ok(
    apres2.some((u) => u.includes("reglages.html")),
    `les réglages restent accessibles (${JSON.stringify(apres2)})`
  );

  // 2 bis. Le RÉSEAU LOCAL aussi : il ne traverse pas le tunnel, il n'a rien à y faire.
  // (Constat de Clément le 27/11 : Proxmox et le tableau de bord du home lab bloqués.)
  await saisir("http://10.255.255.1:9/");
  const apres2bis = await pages();
  console.log("2 bis. adresse du réseau local :", JSON.stringify(apres2bis));
  ok(
    !apres2bis.some((u) => u.includes("tunnel.html")),
    `une adresse privée n'est pas remplacée par la page du tunnel (${JSON.stringify(apres2bis)})`
  );
  // On revient aux réglages pour la suite.
  await saisir(PAGE_REGLAGES);

  // 2 ter. Le cas de Clément : un onglet DÉJÀ chargé, qu'on recharge alors que le portier est
  // allumé. Il ne doit pas rester noir — il doit montrer la page du tunnel, qui porte la cible.
  const portier = async (allumer) => {
    const cible = (await cibles(PORT)).find((t) => t.type === "page" && t.url.includes("reglages.html"));
    if (!cible) throw new Error("page des réglages introuvable");
    const p = await connecter(cible.webSocketDebuggerUrl);
    await p.envoyer("Runtime.enable");
    await pause(700);
    const etat = await p.evaluer(`(() => {
      const b = document.getElementById("bascule-tunnel");
      if (!b) return "absente";
      const allume = b.className.includes("on");
      if (allume !== ${allumer}) b.click();
      return b.className;
    })()`);
    await pause(1500);
    p.fermer();
    return etat;
  };

  await portier(false);
  await saisir(ADRESSE);
  const chargee = await pages();
  console.log("2 ter. page chargée portier éteint :", JSON.stringify(chargee));
  ok(chargee.some((u) => u.includes("example.com")), "la page du site est chargée avant le blocage");

  await saisir(PAGE_REGLAGES);
  await portier(true);
  // Recharger depuis la barre : c'est ce que fait F5, sur un onglet déjà chargé.
  await saisir(ADRESSE);
  await pause(2500);
  const rechargee = await pages();
  console.log("2 ter. après rechargement, portier allumé :", JSON.stringify(rechargee));
  ok(
    rechargee.some((u) => u.includes("tunnel.html")),
    `un onglet déjà chargé ne reste pas noir : il montre la page du tunnel (${JSON.stringify(rechargee)})`
  );

  // 3. On éteint le portier depuis les réglages, dans un SECOND onglet : le premier reste sur la
  // page du tunnel, et c'est lui qui doit REPARTIR tout seul.
  await rail.evaluer(`document.getElementById("btn-nouvel").click()`);
  await pause(1500);
  await saisir(PAGE_REGLAGES);
  const cibleReglages = (await cibles(PORT)).find((t) => t.type === "page" && t.url.includes("reglages.html"));
  if (!cibleReglages) throw new Error("page des réglages introuvable");
  const reglages = await connecter(cibleReglages.webSocketDebuggerUrl);
  await reglages.envoyer("Runtime.enable");
  await pause(1200);
  const bascule = await reglages.evaluer(`(() => {
    const b = document.getElementById("bascule-tunnel");
    if (!b) return "absente";
    b.click();
    return b.className;
  })()`);
  console.log("3. bascule du portier cliquée :", bascule);
  await pause(3500);
  const repris = await pages();
  console.log("3. portier éteint :", JSON.stringify(repris));
  ok(
    !repris.some((u) => u.includes("tunnel.html")),
    `l'onglet bloqué a REPRIS tout seul (plus aucune page du tunnel : ${JSON.stringify(repris)})`
  );
  ok(
    repris.some((u) => u.includes("example.com")),
    `la page visée est revenue (${JSON.stringify(repris)})`
  );
  await saisir(ADRESSE);
  const apres3 = await pages();
  console.log("3. adresse saisie portier éteint :", JSON.stringify(apres3));
  ok(
    apres3.some((u) => u.includes("example.com") && !u.includes("tunnel.html")),
    "portier éteint : l'adresse s'ouvre normalement"
  );

  // Le journal : la trace du refus.
  tuer();
  ok(await fini(), "le processus a quitté");
  try {
    const lignes = (await import("node:fs")).readFileSync(join(profil, "trame.log"), "utf8")
      .split(/\r?\n/)
      .filter((l) => /tunnel|portier/i.test(l))
      .slice(-8);
    console.log("journal :");
    for (const l of lignes) console.log("   " + l);
  } catch {
    console.log("(journal illisible)");
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

console.log(echecs === 0 ? "\nPortier tunnel : il bloque, il s'éteint, et il laisse passer les pages internes." : `\n${echecs} vérification(s) en échec.`);
process.exitCode = echecs === 0 ? 0 : 1;
