#!/usr/bin/env node
/**
 * Banc d'essai du REDEMARRAGE demande par l'interface (0.4.84).
 *
 * Decision de Clement (2026-11-28) : des que le VPN est active avec une configuration validee — et
 * de meme quand il est coupe — Trame doit PROPOSER de redemarrer tout seul. La raison est
 * mecanique : le mandataire du tunnel est pose sur la webview a sa creation
 * (`with_proxy_config`), donc un changement ne peut pas s'appliquer a chaud.
 *
 * Ce banc verifie sur le vrai binaire ce que l'interface ne peut pas prouver, en deux temps :
 *
 *   A. sans tunnel — la demande (`{m:"redemarrerTrame"}`) arrete l'ancien processus, cet arret est
 *      PROPRE (le journal note la fermeture par l'interface, et la session est ecrite : sinon les
 *      onglets seraient perdus), un nouveau processus demarre avec un autre pid, attend la fin de
 *      l'ancien, et reprend la session ;
 *   B. avec le tunnel de Clement monte (portier arme + configuration reelle) — apres le
 *      redemarrage, le nouveau Trame REMONTE le tunnel : la ligne « proxy … pose sur la webview »
 *      reapparait. Sans cela, les pages repartiraient en clair alors que le portier est arme.
 *
 * Usage : node outils/eprouver-redemarrage.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const REEL = join(process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local"), "Trame");

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};
const note = (quoi) => console.log(`  note  ${quoi}`);

const vivant = (pid) => {
  if (!pid) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};
/** Attend qu'un predicat (synchrone) soit vrai, au plus `ms`. */
const attendre = async (predicat, ms) => {
  const fin = Date.now() + ms;
  while (Date.now() < fin) {
    if (predicat()) return true;
    await pause(200);
  }
  return false;
};
const tuer = (pid) => {
  if (!pid) return;
  try {
    execFileSync("taskkill", ["/PID", String(pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
};

/** Un profil d'essai : dossier jetable, reglages donnes, et la configuration du tunnel si demandee. */
function preparerProfil(nom, reglages, avecTunnel) {
  const profil = mkdtempSync(join(tmpdir(), `trame-${nom}-`));
  if (avecTunnel) {
    for (const f of ["tunnel.json", "tunnel.cle"]) {
      const source = join(REEL, f);
      if (!existsSync(source)) return null;
      cpSync(source, join(profil, f));
    }
  }
  writeFileSync(join(profil, "reglages.json"), JSON.stringify(reglages), "utf8");
  return profil;
}

/**
 * Un scenario complet : lance Trame, demande le redemarrage, et rend tout ce qui a ete mesure.
 * `tunnelAttendu` : le nouveau Trame doit-il remonter le tunnel ?
 */
async function scenario({ nom, port, reglages, avecTunnel, tunnelAttendu }) {
  const profil = preparerProfil(nom, reglages, avecTunnel);
  if (!profil) {
    note("pas de configuration de tunnel sur ce poste : scenario « tunnel monte » saute");
    return;
  }
  const journal = () => {
    const f = join(profil, "trame.log");
    return existsSync(f) ? readFileSync(f, "utf8") : "";
  };
  const demarrages = () => {
    const lignes = journal().match(/Trame demarre \(profil [^)]*?, pid (\d+)\)/g) || [];
    return lignes
      .map((l) => {
        const m = /pid (\d+)\)/.exec(l);
        return m ? Number(m[1]) : 0;
      })
      .filter((n) => n > 0);
  };
  const session = join(profil, "session.json");

  let ancien = null;
  let nouveau = null;
  try {
    ancien = spawn(BINAIRE, [], {
      env: {
        ...process.env,
        TRAME_PROFIL: profil,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
      },
      stdio: "ignore",
    });
    const cible = await attendreCible(port, "surface=rail");
    const rail = await connecter(cible.webSocketDebuggerUrl);
    await rail.envoyer("Runtime.enable");
    // Le tunnel demande une poignee de main avec le pair : on laisse le temps de la faire.
    await pause(avecTunnel ? 9000 : 2500);

    const pidAvant = ancien.pid;
    ok(vivant(pidAvant), `[${nom}] l'instance de depart tourne (pid ${pidAvant})`);
    ok(demarrages().includes(pidAvant), `[${nom}] le journal note son pid`);
    if (tunnelAttendu) {
      ok(
        journal().includes("tunnel : proxy") && journal().includes("posé sur la webview"),
        `[${nom}] le tunnel est monte au depart (proxy pose sur la webview)`
      );
    }

    // La demande, telle que l'interface l'envoie apres la confirmation.
    await rail.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "redemarrerTrame" })); true`);
    ok(await attendre(() => !vivant(pidAvant), 20000), `[${nom}] l'ancien processus s'arrete`);
    ok(
      journal().includes(`redemarrage demande : nouveau Trame lance, ce processus (${pidAvant})`),
      `[${nom}] le journal note la demande et le pid remplace`
    );
    ok(
      journal().includes("arret demande par l'interface, fermeture propre"),
      `[${nom}] l'arret passe par la fermeture propre (pas une sortie brutale)`
    );
    const taille = existsSync(session) ? statSync(session).size : 0;
    ok(taille > 2, `[${nom}] la session est ecrite avant l'arret (${taille} octets)`);
    let lue = null;
    try {
      lue = JSON.parse(readFileSync(session, "utf8"));
    } catch {
      lue = null;
    }
    ok(lue !== null, `[${nom}] la session ecrite est du JSON lisible`);

    // Le nouveau : autre pid, vivant, du meme profil, et il attend la fin de l'ancien.
    ok(
      await attendre(() => {
        const pids = demarrages();
        nouveau = pids[pids.length - 1] || 0;
        return nouveau !== 0 && nouveau !== pidAvant && vivant(nouveau);
      }, 30000),
      `[${nom}] un nouveau Trame demarre sur le meme profil (pid ${nouveau})`
    );
    ok(
      journal().includes(`relance : attente de la fin du processus ${pidAvant}`),
      `[${nom}] le nouveau attend la fin de l'ancien`
    );
    ok(!vivant(pidAvant), `[${nom}] l'ancien ne tourne plus (aucun doublon)`);

    // Le nouveau va jusqu'au bout de son demarrage.
    ok(
      await attendre(() => /session : \d+ onglet/.test(journal().split(`pid ${nouveau})`)[1] || ""), 20000),
      `[${nom}] le nouveau Trame reprend la session (onglets revenus)`
    );

    if (tunnelAttendu) {
      const apres = journal().split(`pid ${nouveau})`)[1] || "";
      ok(
        await attendre(() => apres.includes("posé sur la webview") || journal().split(`pid ${nouveau})`)[1]?.includes("posé sur la webview"), 25000),
        `[${nom}] apres le redemarrage, le tunnel est remonte`
      );
      const suite = journal().split(`pid ${nouveau})`)[1] || "";
      ok(/tunnel : (poignee|handshake|tunnel etabli|proxy)/.test(suite) || suite.includes("tunnel :"), `[${nom}] le nouveau journal parle du tunnel`);
    }

    // La webview du nouveau est joignable : information, pas une exigence — le port de debogage
    // peut rester tenu un instant par les processus WebView2 de l'ancien.
    try {
      const pages = (await cibles(port)).filter((t) => t.type === "page");
      note(`[${nom}] cibles CDP apres redemarrage : ${pages.length}`);
    } catch (e) {
      note(`[${nom}] cibles CDP non relues (${e.message})`);
    }
  } catch (e) {
    console.error(`  ECHEC du scenario ${nom} : ${e.message}`);
    echecs += 1;
  } finally {
    tuer(nouveau);
    tuer(ancien && ancien.pid);
  }
}

console.log("A. redemarrage sans tunnel");
await scenario({ nom: "redemarrage", port: 19491, reglages: { tunnel_oblige: false }, avecTunnel: false, tunnelAttendu: false });

console.log("\nB. redemarrage avec le tunnel monte");
await scenario({ nom: "redemarrage-tunnel", port: 19492, reglages: { tunnel_oblige: true }, avecTunnel: true, tunnelAttendu: true });

console.log(`\n${echecs === 0 ? "tous les controles passent" : `${echecs} controle(s) en echec`}`);
process.exit(echecs === 0 ? 0 : 1);
