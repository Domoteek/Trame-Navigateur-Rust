/**
 * Éprouve le **réduire** : la fenêtre envoyée dans la barre des tâches, et Trame doit survivre.
 *
 * Mesure du 2026-11-22 : réduire la fenêtre **tuait** Trame (code de sortie 101, c'est-à-dire une
 * panique Rust : `min > max` dans `shell::geometrie` — sur une fenêtre réduite, dont la hauteur
 * n'est plus que de 19 px, le plafond de la barre (72 % = 13,68 px) tombait sous son minimum
 * (46 px), et `f64::clamp` panique quand le minimum dépasse le plafond). Rien dans le journal :
 * une panique n'écrit que sur stderr, et la version release n'a pas de console.
 *
 * Ce banc d'essai lance le binaire sur un **profil jetable**, réduit sa fenêtre par
 * `WM_SYSCOMMAND`/`SC_MINIMIZE` (le geste exact du bouton de la barre des tâches, envoyé par
 * `outils/piloter-fenetre.ps1`), vérifie que le processus est toujours vivant, restaure, vérifie
 * encore, puis relit le journal.
 *
 * Usage :
 *   node outils/eprouver-reduire.mjs                       # target/release/trame.exe
 *   node outils/eprouver-reduire.mjs --exe "C:\…\trame.exe"
 */
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ICI = path.dirname(fileURLToPath(import.meta.url));
const RACINE = path.resolve(ICI, "..");
const args = process.argv.slice(2);
const option = (nom, defaut) => {
  const i = args.indexOf(`--${nom}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : defaut;
};
const EXE = option("exe", path.join(RACINE, "target", "release", "trame.exe"));
const PILOTE = path.join(ICI, "piloter-fenetre.ps1");

const pause = (ms) => new Promise((r) => setTimeout(r, ms));
const vivant = (pid) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

/** Envoie une action a la fenetre du processus, et rend l'etat imprime par le pilote. */
function piloter(pid, action) {
  const r = spawnSync(
    "powershell",
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", PILOTE, "-Processus", String(pid), action],
    { encoding: "utf8", windowsHide: true }
  );
  const sortie = `${r.stdout || ""}${r.stderr || ""}`.trim();
  const minimisee = /minimisee=(True|False)/.exec(sortie);
  return { code: r.status, sortie, minimisee: minimisee ? minimisee[1] === "True" : null };
}

async function principale() {
  if (!fs.existsSync(EXE)) {
    console.error(`Binaire absent : ${EXE}\nConstruire d'abord : cargo build --release`);
    process.exit(2);
  }
  const profil = fs.mkdtempSync(path.join(os.tmpdir(), "trame-reduire-"));
  const journal = path.join(profil, "trame.log");
  const enfant = spawn(EXE, [], {
    cwd: RACINE,
    env: { ...process.env, TRAME_PROFIL: profil },
    stdio: "ignore",
  });
  let ok = true;
  const dire = (texte) => console.log(texte);
  try {
    dire(`binaire  : ${EXE}`);
    dire(`profil   : ${profil}`);
    // La fenetre doit exister : on attend que la page soit chargee, pas seulement le processus.
    const limite = Date.now() + 40000;
    let demarre = false;
    while (Date.now() < limite) {
      if (fs.existsSync(journal) && /premier chargement termine/.test(fs.readFileSync(journal, "utf8"))) {
        demarre = true;
        break;
      }
      await pause(500);
    }
    if (!demarre) throw new Error("la fenetre de Trame n'a pas fini de demarrer");
    await pause(1500);

    dire(`\n--- reduire (WM_SYSCOMMAND / SC_MINIMIZE) ---`);
    const reduction = piloter(enfant.pid, "-Reduire");
    dire(`pilote   : ${reduction.sortie}`);
    await pause(4000);
    const vitApresReduction = vivant(enfant.pid);
    dire(`vivant apres reduction : ${vitApresReduction}`);
    dire(`fenetre minimisee      : ${reduction.minimisee}`);
    if (!vitApresReduction || reduction.minimisee !== true) ok = false;

    if (vitApresReduction) {
      dire(`\n--- restaurer ---`);
      const restauration = piloter(enfant.pid, "-Restaurer");
      dire(`pilote   : ${restauration.sortie}`);
      await pause(2500);
      const vitApresRestauration = vivant(enfant.pid);
      dire(`vivant apres restauration : ${vitApresRestauration}`);
      dire(`fenetre minimisee         : ${restauration.minimisee}`);
      if (!vitApresRestauration || restauration.minimisee !== false) ok = false;
    }

    const texte = fs.existsSync(journal) ? fs.readFileSync(journal, "utf8") : "";
    const paniques = texte.split(/\r?\n/).filter((l) => /PANIQUE|panicked/.test(l));
    dire(`\npanique dans le journal : ${paniques.length === 0 ? "aucune" : paniques.join(" / ")}`);
    if (paniques.length) ok = false;

    // Seconde preuve, celle qui manquait le 2026-11-22 : une panique doit laisser une trace dans
    // le journal (sinon un plantage est indiscernable d'une fermeture propre). La sonde
    // `TRAME_SONDE_PANIQUE` panique volontairement au demarrage.
    dire(`\n--- sonde de panique (le journal doit en garder la trace) ---`);
    const profilSonde = fs.mkdtempSync(path.join(os.tmpdir(), "trame-panique-"));
    const journalSonde = path.join(profilSonde, "trame.log");
    const sonde = spawn(EXE, [], {
      cwd: RACINE,
      env: { ...process.env, TRAME_PROFIL: profilSonde, TRAME_SONDE_PANIQUE: "1" },
      stdio: "ignore",
    });
    await new Promise((r) => sonde.on("exit", r));
    const texteSonde = fs.existsSync(journalSonde) ? fs.readFileSync(journalSonde, "utf8") : "";
    const traceSonde = texteSonde.split(/\r?\n/).find((l) => l.includes("PANIQUE")) || "";
    dire(`code de sortie : ${sonde.exitCode} (101 = panique)`);
    dire(`journal        : ${traceSonde.trim() || "(aucune trace)"}`);
    if (!traceSonde.includes("PANIQUE") || !traceSonde.includes("sonde")) ok = false;
    try {
      fs.rmSync(profilSonde, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
    } catch (e) {
      console.warn(`(profil de la sonde garde : ${profilSonde} — ${e.code || e.message})`);
    }

    dire(ok ? "\nRECAP : reduire ne tue pas Trame, et une panique laisse une trace" : "\nRECAP : ECHEC (voir ci-dessus)");
    return ok;
  } finally {
    spawn("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
    await pause(1200);
    try {
      fs.rmSync(profil, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
    } catch (e) {
      console.warn(`(profil de test garde : ${profil} — ${e.code || e.message})`);
    }
  }
}

principale()
  .then((ok) => process.exit(ok ? 0 : 1))
  .catch((e) => {
    console.error(`ECHEC : ${e.message}`);
    process.exit(1);
  });
