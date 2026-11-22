#!/usr/bin/env node
/**
 * Mesure les ressources de Trame : CPU au repos, mémoire de l'arbre de processus, taille du
 * binaire, et délai de démarrage.
 *
 * Pourquoi un outil. « Plus rapide » et « plus léger » ne veulent rien dire sans chiffres, et
 * un navigateur se juge sur son état au repos (une page ouverte qu'on ne touche pas). Ce script
 * mesure donc trois choses, de la même façon avant et après une optimisation :
 *
 *   1. **CPU au repos** : `TotalProcessorTime` de Trame et de tous ses descendants, mesuré sur
 *      une fenêtre, divisé par la durée et par le nombre de cœurs → un pourcentage ;
 *   2. **mémoire** : somme des `WorkingSet` de l'arbre (c'est la seule mesure honnête : WebView2
 *      ne donne pas la mémoire par onglet) ;
 *   3. **démarrage** : temps entre le lancement et la dernière surface créée dans `trame.log`.
 *
 * Usage :
 *   node outils/mesurer-ressources.mjs                (mesure l'instance en cours, 30 s)
 *   node outils/mesurer-ressources.mjs --duree 60
 *   node outils/mesurer-ressources.mjs --demarrer     (lance une instance sur un profil jetable)
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ICI = path.dirname(fileURLToPath(import.meta.url));
const RACINE = path.resolve(ICI, "..");
const EXE = path.join(RACINE, "target", "release", "trame.exe");

const args = process.argv.slice(2);
const duree = (() => {
  const i = args.indexOf("--duree");
  return i >= 0 ? Number(args[i + 1]) : 30;
})();
const demarrer = args.includes("--demarrer");

const COEURS = os.cpus().length;

/** Rend { pids, cpu, memoire } pour Trame et tous ses descendants. */
function arbre() {
  const r = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `$tous = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, WorkingSetSize, KernelModeTime, UserModeTime
$racines = @($tous | Where-Object { $_.ProcessId -in (Get-Process trame -ErrorAction SilentlyContinue).Id } | ForEach-Object { $_.ProcessId })
$vus = New-Object System.Collections.Generic.HashSet[int]
$pile = New-Object System.Collections.Generic.Stack[int]
foreach ($r in $racines) { $pile.Push([int]$r) }
$cpu = 0.0; $memoire = 0.0; $n = 0
$cpuRacine = 0.0; $memoireRacine = 0.0
foreach ($r in $racines) {
  $pr = $tous | Where-Object { $_.ProcessId -eq $r } | Select-Object -First 1
  if ($null -ne $pr) {
    $cpuRacine += ([double]$pr.KernelModeTime + [double]$pr.UserModeTime) / 10000000.0
    $memoireRacine += [double]$pr.WorkingSetSize
  }
}
while ($pile.Count -gt 0) {
  $p = $pile.Pop()
  if (-not $vus.Add($p)) { continue }
  $proc = $tous | Where-Object { $_.ProcessId -eq $p } | Select-Object -First 1
  if ($null -eq $proc) { continue }
  $n++
  $memoire += [double]$proc.WorkingSetSize
  $cpu += ([double]$proc.KernelModeTime + [double]$proc.UserModeTime) / 10000000.0
  foreach ($e in ($tous | Where-Object { $_.ParentProcessId -eq $p })) { $pile.Push([int]$e.ProcessId) }
}
Write-Output ("processus=" + $n)
Write-Output ("cpu_s=" + $cpu.ToString('F3'))
Write-Output ("memoire_mo=" + ($memoire / 1MB).ToString('F1'))
Write-Output ("cpu_racine_s=" + $cpuRacine.ToString('F3'))
Write-Output ("memoire_racine_mo=" + ($memoireRacine / 1MB).ToString('F1'))`,
    ],
    { encoding: "utf8", windowsHide: true, maxBuffer: 16 * 1024 * 1024 }
  );
  const valeurs = {};
  for (const ligne of (r.stdout || "").split(/\r?\n/)) {
    const [cle, ...reste] = ligne.split("=");
    if (cle && reste.length) valeurs[cle.trim()] = reste.join("=").trim();
  }
  // PowerShell ecrit « 1,234 » en culture francaise : on normalise avant de convertir.
  const nombre = (v) => Number(String(v || "0").replace(",", "."));
  return {
    processus: nombre(valeurs.processus),
    cpu: nombre(valeurs.cpu_s),
    memoire: nombre(valeurs.memoire_mo),
    cpuRacine: nombre(valeurs.cpu_racine_s),
    memoireRacine: nombre(valeurs.memoire_racine_mo),
  };
}

function attendre(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function main() {
  console.log(`Mesure de Trame — ${duree} s, ${COEURS} cœurs\n`);

  let enfant = null;
  let profil = null;
  if (demarrer) {
    profil = path.join(os.tmpdir(), `trame-mesure-${process.pid}`);
    fs.rmSync(profil, { recursive: true, force: true });
    const debut = Date.now();
    enfant = spawn(EXE, [], {
      env: { ...process.env, TRAME_PROFIL: profil },
      stdio: "ignore",
    });
    // Délai de démarrage : de la lancement à la dernière surface prête.
    let pret = null;
    for (let i = 0; i < 120 && pret === null; i += 1) {
      await attendre(100);
      const journal = path.join(profil, "trame.log");
      if (fs.existsSync(journal)) {
        const texte = fs.readFileSync(journal, "utf8");
        if (texte.includes("surface barre creee")) pret = Date.now() - debut;
      }
    }
    console.log(`démarrage : ${pret === null ? "non mesuré" : `${pret} ms`} (profil ${profil})`);
    await attendre(4000); // laisser WebView2 finir de s'installer
  }

  const debut = arbre();
  if (debut.processus === 0) {
    console.error("aucune instance de Trame trouvée — lancer l'application ou --demarrer");
    process.exit(2);
  }
  await attendre(duree * 1000);
  const fin = arbre();

  const deltaCpu = fin.cpu - debut.cpu;
  const pourcentage = (deltaCpu / duree / COEURS) * 100;
  const memoire = fin.memoire;

  const deltaCoquille = fin.cpuRacine - debut.cpuRacine;
  const pourcentageCoquille = (deltaCoquille / duree / COEURS) * 100;
  const taille = fs.statSync(EXE).size;
  console.log(`processus          : ${fin.processus}`);
  console.log(`mémoire (arbre)    : ${memoire.toFixed(1)} Mo`);
  console.log(`  dont coquille    : ${fin.memoireRacine.toFixed(1)} Mo`);
  console.log(`CPU au repos       : ${deltaCpu.toFixed(2)} s sur ${duree} s → ${(deltaCpu / duree * 100).toFixed(2)} % d'un cœur`);
  console.log(`  dont coquille    : ${deltaCoquille.toFixed(2)} s → ${(deltaCoquille / duree * 100).toFixed(3)} % d'un cœur`);
  console.log(`binaire            : ${(taille / 1048576).toFixed(2)} Mo`);

  if (enfant) {
    try {
      spawnSync("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
    } catch {
      enfant.kill();
    }
    fs.rmSync(profil, { recursive: true, force: true });
  }
}

main().catch((e) => {
  console.error("mesure interrompue :", e);
  process.exit(2);
});
