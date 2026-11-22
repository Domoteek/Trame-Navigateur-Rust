#!/usr/bin/env node
/**
 * Tests de l'application REELLE (binaire release, vrai WebView2), sur un profil jetable.
 *
 * Pourquoi : `cargo test` couvre la logique pure et `outils/tester-ui.mjs` couvre l'interface
 * hors WebView2, mais ni l'un ni l'autre ne demarre l'application. Ici on lance le binaire,
 * on lui donne un profil neuf ou casse, et on lit ce qu'il ecrit dans `trame.log` — la seule
 * trace disponible en release (la console est masquee).
 *
 * Lancement :
 *   node outils/verifier-app.mjs                (tous les scenarios)
 *   node outils/verifier-app.mjs --scenario b   (un seul)
 *
 * Chaque scenario cree son propre dossier sous %TEMP%, ne touche JAMAIS au profil reel
 * (`%APPDATA%\Trame`), et supprime le dossier a la fin sauf en cas d'echec (pour examen).
 */

import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import os from "node:os";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ICI = path.dirname(fileURLToPath(import.meta.url));
const RACINE = path.resolve(ICI, "..");
const EXE = path.join(RACINE, "target", "release", "trame.exe");
const ATTENTE_MS = 9000;

const args = process.argv.slice(2);
const filtre = (() => {
  const i = args.indexOf("--scenario");
  return i >= 0 ? args[i + 1].toLowerCase() : null;
})();

const resultats = [];

function dossierJetable(nom) {
  const d = path.join(os.tmpdir(), `trame-verif-${nom}-${process.pid}`);
  fs.rmSync(d, { recursive: true, force: true });
  fs.mkdirSync(d, { recursive: true });
  return d;
}

/**
 * Supprime un dossier jetable, en tolerant un verrou passager.
 *
 * `taskkill` rend la main avant que les processus WebView2 soient tous morts : leur profil reste
 * quelques centaines de millisecondes sous verrou, et un `rmSync` sec echouait en `EPERM` — le
 * scenario etait alors compte en echec alors que tout ce qu'il verifiait passait. Un dossier qui
 * resiste n'est pas une panne : on le signale, on ne fait pas echouer le test.
 */
function supprimer(dossier) {
  try {
    fs.rmSync(dossier, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
  } catch (e) {
    console.warn(`        (dossier de test garde : ${dossier} — ${e.code || e.message})`);
  }
}

function sessionExemple(nbOnglets) {
  const maintenant = Date.now();
  const onglets = [];
  for (let i = 0; i < nbOnglets; i += 1) {
    onglets.push({
      id: i + 1,
      url: `https://exemple.fr/page-${i + 1}`,
      titre: `Page ${i + 1}`,
      fil: 1,
      parent: i === 0 ? null : 1,
      declencheur: "saisie",
      cree_le: maintenant - (nbOnglets - i) * 60_000,
      dernier_focus: maintenant,
      nb_focus: 1,
      temps_actif_ms: 0,
      defilement: 0,
      ancre: false,
      a_lire: false,
      vitalite: 0.8,
    });
  }
  return {
    onglets,
    fils: [{ id: 1, nom: "Fil de test", auto_nom: false, cree_le: maintenant, actif_le: maintenant, replie: false }],
    actif: 1,
    rail_replie: false,
    prochain_onglet: nbOnglets + 1,
    prochain_fil: 2,
  };
}

/** Lance le binaire sur un profil, attend, arrete, et rend le journal + la sortie. */
function lancer(profil, env = {}, attenteMs = ATTENTE_MS) {
  return new Promise((resoudre) => {
    const enfant = spawn(EXE, [], {
      cwd: RACINE,
      env: { ...process.env, TRAME_PROFIL: profil, ...env },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let sortie = "";
    enfant.stdout.on("data", (d) => {
      sortie += d.toString();
    });
    enfant.stderr.on("data", (d) => {
      sortie += d.toString();
    });
    const minuterie = setTimeout(() => {
      // On tue l'arbre : trame lance des processus WebView2 enfants.
      try {
        spawn("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
      } catch {
        enfant.kill("SIGKILL");
      }
    }, attenteMs);
    enfant.on("exit", () => {
      clearTimeout(minuterie);
      const journal = fs.existsSync(path.join(profil, "trame.log"))
        ? fs.readFileSync(path.join(profil, "trame.log"), "utf8")
        : "";
      resoudre({ journal, sortie, profil });
    });
  });
}

function verifier(condition, message) {
  if (!condition) throw new Error(message);
}

async function scenario(nom, fn) {
  // `--scenario b` selectionne « b. ... » : la comparaison porte sur la lettre, pas sur une
  // sous-chaine — sinon « c » apparait dans presque tous les noms.
  if (filtre && !nom.toLowerCase().startsWith(`${filtre}.`)) return;
  try {
    await fn();
    resultats.push({ nom, ok: true });
    console.log(`  ok    ${nom}`);
  } catch (e) {
    resultats.push({ nom, ok: false, erreur: e.message });
    console.log(`  ECHEC ${nom}\n        ${e.message}`);
  }
}

async function suite() {
  if (!fs.existsSync(EXE)) {
    console.error(`Binaire absent : ${EXE}\nConstruire d'abord : cargo build --release`);
    process.exit(2);
  }
  console.log("Tests de l'application reelle (profil jetable)");

  await scenario("a. profil neuf : demarrage, un onglet reveille, arret propre", async () => {
    const profil = dossierJetable("neuf");
    const { journal } = await lancer(profil, { TRAME_NAV: "https://exemple.fr/" });
    verifier(journal.includes("Trame demarre"), "le journal doit annoncer le demarrage");
    verifier(journal.includes("profil :"), "le profil doit etre annonce");
    verifier(/onglet \d+ reveille/.test(journal), "un onglet doit etre reveille");
    verifier(!/panicked|RUST_BACKTRACE/.test(journal), "aucune panique");
    // Le profil neuf doit avoir recu les extensions livrees et les regles par defaut.
    verifier(fs.existsSync(path.join(profil, "zap.json")), "zap.json doit etre cree");
    verifier(
      fs.existsSync(path.join(profil, "extensions")) &&
        fs.readdirSync(path.join(profil, "extensions")).length >= 3,
      "les trois extensions livrees doivent etre copiees"
    );
    supprimer(profil);
  });

  await scenario("b. session corrompue : le fichier est ecarte, pas detruit", async () => {
    const profil = dossierJetable("session-cassee");
    fs.writeFileSync(path.join(profil, "session.json"), '{"onglets": [ {"id": 1');
    const { journal } = await lancer(profil);
    verifier(journal.includes("session illisible"), `journal: ${journal.slice(0, 400)}`);
    verifier(journal.includes("ancien fichier garde en"), "le journal doit dire ou est la copie");
    const garde = path.join(profil, "session.json.casse");
    verifier(fs.existsSync(garde), "session.json.casse doit exister");
    verifier(
      fs.readFileSync(garde, "utf8").includes('{"onglets": [ {"id": 1'),
      "la copie doit contenir le fichier d'origine"
    );
    supprimer(profil);
  });

  await scenario("c. reglages corrompus : valeurs par defaut et copie gardee", async () => {
    const profil = dossierJetable("reglages-casses");
    fs.writeFileSync(path.join(profil, "reglages.json"), "pas du json");
    const { journal } = await lancer(profil);
    verifier(journal.includes("reglages illibles"), `journal: ${journal.slice(0, 400)}`);
    verifier(fs.existsSync(path.join(profil, "reglages.json.casse")), "copie gardee");
    verifier(journal.includes("moteur de recherche : Perplexity"), "on retombe sur le defaut");
    supprimer(profil);
  });

  await scenario("d. zap corrompu : les regles par defaut reviennent", async () => {
    const profil = dossierJetable("zap-casse");
    fs.writeFileSync(path.join(profil, "zap.json"), "{]");
    const { journal } = await lancer(profil);
    verifier(journal.includes("zap illisible"), `journal: ${journal.slice(0, 400)}`);
    verifier(fs.existsSync(path.join(profil, "zap.json.casse")), "copie gardee");
    const regles = JSON.parse(fs.readFileSync(path.join(profil, "zap.json"), "utf8"));
    verifier(regles.length >= 12, `12 regles attendues, ${regles.length}`);
    supprimer(profil);
  });

  await scenario("e. adresse interne refusee : la page reste en place", async () => {
    const profil = dossierJetable("adresse-interne");
    const { journal } = await lancer(profil, { TRAME_NAV: "chrome-search://local-ntp/local-ntp.html" });
    verifier(
      journal.includes("navigation refusee (adresse interne de Chromium)"),
      `journal: ${journal.slice(0, 500)}`
    );
    verifier(journal.includes("chrome-search://local-ntp/local-ntp.html"), "l'adresse est citee");
    supprimer(profil);
  });

  await scenario("f. 30 onglets en session : le budget de webviews est respecte", async () => {
    const profil = dossierJetable("budget");
    fs.writeFileSync(
      path.join(profil, "session.json"),
      JSON.stringify(sessionExemple(30), null, 2)
    );
    const { journal } = await lancer(profil);
    verifier(journal.includes("session : 30 onglet(s), 1 fil(s)"), `journal: ${journal.slice(0, 500)}`);
    const reveilles = (journal.match(/onglet \d+ reveille/g) || []).length;
    // Budget par defaut : 6 webviews d'onglets. Au demarrage, un seul doit etre reveille.
    verifier(reveilles <= 7, `${reveilles} onglets reveilles au demarrage, 6 attendus au plus`);
    // Le panneau droit prend 1 px : la geometrie doit rester coherente.
    verifier(!/panicked/.test(journal), "aucune panique");
    supprimer(profil);
  });

  await scenario("g. un fichier de session trop vieux (champs absents) se lit", async () => {
    const profil = dossierJetable("session-ancienne");
    fs.writeFileSync(
      path.join(profil, "session.json"),
      JSON.stringify({ onglets: [{ id: 3, fil: 1, cree_le: 1 }], fils: [{ id: 1, cree_le: 1 }] })
    );
    const { journal } = await lancer(profil);
    verifier(journal.includes("session : 1 onglet(s), 1 fil(s)"), `journal: ${journal.slice(0, 400)}`);
    verifier(!journal.includes("session illisible"), "un ancien fichier valide ne doit pas etre ecarte");
    supprimer(profil);
  });

  await scenario("h. deux instances sur le meme profil : la seconde refuse", async () => {
    const profil = dossierJetable("double");
    const [a, b] = await Promise.all([lancer(profil), lancer(profil)]);
    // Les deux processus ecrivent dans le MEME `trame.log` : on ne lit donc qu'une fois,
    // sinon chaque ligne de refus serait comptee deux fois.
    const journal = a.journal.length >= b.journal.length ? a.journal : b.journal;
    // Depuis que Trame peut etre le navigateur par defaut, la seconde instance transmet sa
    // demande (un lien, ou simplement revenir au premier plan) a la premiere, puis s'arrete.
    const refus = journal.match(/profil deja ouvert par le processus \d+ : demande transmise/g) || [];
    verifier(refus.length === 1, `un seul refus attendu, ${refus.length} trouve(s)`);
    verifier(
      (journal.match(/Trame demarre/g) || []).length >= 2,
      "les deux instances demarrent (puis l'une s'arrete)"
    );
    // La session ecrite doit rester lisible : aucune des deux ne l'a cassee.
    const session = path.join(profil, "session.json");
    if (fs.existsSync(session)) {
      JSON.parse(fs.readFileSync(session, "utf8"));
    }
    supprimer(profil);
  });

  await scenario("i. zoom applique par WebView2 (set_zoom_factor)", async () => {
    const profil = dossierJetable("zoom");
    const { journal } = await lancer(profil, { TRAME_ZOOM: "1.3" });
    verifier(journal.includes("zoom onglet"), `journal: ${journal.slice(0, 500)}`);
    verifier(journal.includes("130 %"), "le journal doit annoncer 130 %");
    verifier(!journal.includes("zoom impossible"), "l'API de zoom doit repondre");
    supprimer(profil);
  });

  await scenario("j. un telechargement arrive dans le dossier du profil", async () => {
    // Un vrai serveur local sert un fichier en piece jointe : c'est le seul moyen honnete
    // d'eprouver le gestionnaire de telechargement de WebView2 sans cliquer dans une page.
    const serveur = http.createServer((_req, rep) => {
      rep.writeHead(200, {
        "content-type": "application/pdf",
        "content-disposition": 'attachment; filename="preuve-trame.pdf"',
      });
      rep.end("%PDF-1.4\n% fichier de test Trame\n");
    });
    await new Promise((r) => serveur.listen(0, "127.0.0.1", r));
    const url = `http://127.0.0.1:${serveur.address().port}/preuve-trame.pdf`;
    const profil = dossierJetable("telechargement");
    const { journal } = await lancer(profil, { TRAME_NAV: url });
    serveur.close();
    verifier(journal.includes("telechargement :"), `journal: ${journal.slice(0, 800)}`);
    verifier(journal.includes("telechargement termine"), `journal: ${journal.slice(0, 800)}`);
    const dossier = path.join(profil, "telechargements");
    verifier(fs.existsSync(dossier), "le dossier telechargements doit exister");
    const fichiers = fs.readdirSync(dossier);
    verifier(fichiers.length >= 1, `un fichier attendu, ${fichiers.length} trouve(s)`);
    supprimer(profil);
  });

  await scenario("k. le zappeur masque vraiment l'element sur la page", async () => {
    // La page de test rapporte elle-meme le `display` calcule de l'element : c'est la seule
    // facon de savoir, depuis l'exterieur, si le CSS injecte par Trame a pris effet.
    let rapport = null;
    const serveur = http.createServer((req, rep) => {
      if (req.url.startsWith("/rapport")) {
        rapport = new URL(req.url, "http://x").searchParams.get("display");
        rep.writeHead(204).end();
        return;
      }
      rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      rep.end(`<!doctype html><html><body>
        <div id="cible">element a masquer</div>
        <p id="temoin">temoin visible</p>
        <script>
          setTimeout(() => {
            const d = getComputedStyle(document.getElementById("cible")).display;
            const t = getComputedStyle(document.getElementById("temoin")).display;
            fetch("/rapport?display=" + d + "&temoin=" + t).catch(() => {});
          }, 2500);
        </script>
      </body></html>`);
    });
    await new Promise((r) => serveur.listen(0, "127.0.0.1", r));
    const url = `http://127.0.0.1:${serveur.address().port}/page.html`;

    const profil = dossierJetable("zappeur");
    fs.writeFileSync(
      path.join(profil, "zap.json"),
      JSON.stringify([
        { domaine: "127.0.0.1", selecteur: "#cible", libelle: "Element de test", actif: true },
      ])
    );
    const { journal } = await lancer(profil, { TRAME_NAV: url });
    serveur.close();
    verifier(rapport !== null, `la page n'a rien rapporte. journal: ${journal.slice(0, 600)}`);
    verifier(rapport === "none", `#cible devrait etre masque, display=${rapport}`);
    supprimer(profil);
  });

  await scenario("l. installation d'une extension depuis un fichier .crx", async () => {
    // On fabrique un vrai CRX3 : un dossier zippe (PowerShell), precede de l'en-tete de
    // signature. Le manifeste porte une cle publique pour que l'identifiant soit celui de
    // Chromium — et on le recalcule avec le crypto de Node pour verifier notre SHA-256.
    const crypto = await import("node:crypto");
    const travail = dossierJetable("crx");
    const source = path.join(travail, "paquet");
    fs.mkdirSync(source, { recursive: true });
    const cle = crypto.randomBytes(96).toString("base64");
    fs.writeFileSync(
      path.join(source, "manifest.json"),
      JSON.stringify({
        manifest_version: 3,
        name: "Preuve d'installation",
        version: "1.2.3",
        description: "Extension fabriquee par le test",
        key: cle,
        action: { default_popup: "popup.html" },
        permissions: ["storage"],
      })
    );
    fs.writeFileSync(path.join(source, "popup.html"), "<!doctype html><title>preuve</title>ok");
    const zip = path.join(travail, "paquet.zip");
    await new Promise((resoudre, rejeter) => {
      const ps = spawn("powershell", [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        `Compress-Archive -Path '${source}\\*' -DestinationPath '${zip}' -Force`,
      ]);
      ps.on("exit", (code) => (code === 0 ? resoudre() : rejeter(new Error("Compress-Archive a echoue"))));
    });
    // Identifiant attendu, regle de Chromium : sha256(cle) -> 32 lettres a-p.
    const empreinte = crypto.createHash("sha256").update(Buffer.from(cle, "base64")).digest();
    let identifiant = "";
    for (const octet of empreinte.subarray(0, 16)) {
      identifiant += String.fromCharCode(97 + (octet >> 4));
      identifiant += String.fromCharCode(97 + (octet & 0x0f));
    }
    // CRX3 = "Cr24" | version 3 | taille d'en-tete | en-tete | zip
    const enTete = Buffer.alloc(64, 7);
    const crx = Buffer.concat([
      Buffer.from("Cr24", "ascii"),
      (() => {
        const b = Buffer.alloc(8);
        b.writeUInt32LE(3, 0);
        b.writeUInt32LE(enTete.length, 4);
        return b;
      })(),
      enTete,
      fs.readFileSync(zip),
    ]);
    const fichier = path.join(travail, "preuve.crx");
    fs.writeFileSync(fichier, crx);

    const profil = dossierJetable("crx-profil");
    const { journal } = await lancer(profil, { TRAME_CRX: fichier });
    verifier(journal.includes("extension installee"), `journal: ${journal.slice(0, 900)}`);
    verifier(journal.includes("CRX v3"), "la version du format doit etre journalisee");
    const installee = path.join(profil, "extensions-chrome", identifiant);
    verifier(
      fs.existsSync(path.join(installee, "manifest.json")),
      `dossier attendu ${installee}. Contenu : ${JSON.stringify(
        fs.existsSync(path.join(profil, "extensions-chrome"))
          ? fs.readdirSync(path.join(profil, "extensions-chrome"))
          : "aucun"
      )}`
    );
    const manifeste = JSON.parse(fs.readFileSync(path.join(installee, "manifest.json"), "utf8"));
    verifier(manifeste.version === "1.2.3", "le manifeste doit etre celui du paquet");
    verifier(fs.existsSync(path.join(installee, "popup.html")), "les fichiers doivent etre extraits");
    const sources = JSON.parse(
      fs.readFileSync(path.join(profil, "extensions-chrome-sources.json"), "utf8")
    );
    verifier(sources[identifiant] && sources[identifiant].source === "fichier", "provenance notee");
    // A chaud : le moteur de WebView2 confirme l'ajout sous l'identifiant attendu, puis le
    // liste comme active — dans la MEME execution, sans redemarrage.
    verifier(
      journal.includes(`moteur d'extensions : Ajout de ${identifiant} fait`),
      `le moteur doit confirmer l'ajout a chaud de ${identifiant} : ${journal.split("\n").filter((l) => l.includes("moteur")).join(" | ")}`
    );
    const listes = journal.split("\n").filter((l) => l.includes("moteur d'extensions :") && l.includes("chargee(s)"));
    verifier(
      listes.some((l) => l.includes("Preuve d'installation (active)")),
      `l'extension doit etre listee active par le moteur : ${listes.join(" | ")}`
    );
    supprimer(profil);
    supprimer(travail);
  });

  await scenario("s. memoire basse : les onglets caches y passent, jamais l'onglet visible", async () => {
    // Session de demonstration (11 onglets), veille automatique coupee pour que 6 restent
    // ouverts (budget), dont 5 caches. `TRAME_REVEILLER` les ouvre tous au demarrage.
    const profil = dossierJetable("memoire-basse");
    const graine = spawnSync("node", [path.join(RACINE, "outils", "graine-session.mjs"), profil], { encoding: "utf8" });
    verifier(graine.status === 0, `graine : ${graine.stderr}`);
    fs.writeFileSync(
      path.join(profil, "reglages.json"),
      JSON.stringify({ moteur: "perplexity", veille_auto: false, budget: 6, memoire_basse: true })
    );
    const session = JSON.parse(fs.readFileSync(path.join(profil, "session.json"), "utf8"));
    const { journal } = await lancer(profil, { TRAME_REVEILLER: "1" }, 30000);
    const bas = [...journal.matchAll(/onglet (\d+) : memoire basse/g)].map((m) => Number(m[1]));
    verifier(bas.length === 5, `5 onglets caches attendus en memoire basse, obtenu ${bas.length} : ${bas}`);
    verifier(!bas.includes(session.actif), `l'onglet visible (${session.actif}) ne doit jamais passer en memoire basse`);
    verifier(!journal.includes("niveau de memoire refuse"), "le moteur doit accepter le niveau");
    supprimer(profil);

    // Reglage coupe : aucun onglet n'y passe.
    const profil2 = dossierJetable("memoire-normale");
    spawnSync("node", [path.join(RACINE, "outils", "graine-session.mjs"), profil2]);
    fs.writeFileSync(
      path.join(profil2, "reglages.json"),
      JSON.stringify({ moteur: "perplexity", veille_auto: false, budget: 6, memoire_basse: false })
    );
    const coupe = await lancer(profil2, { TRAME_REVEILLER: "1" }, 30000);
    verifier(!coupe.journal.includes(": memoire basse"), "reglage coupe : aucun onglet en memoire basse");
    supprimer(profil2);
  });

  await scenario("u. memoire libre faible : les onglets froids s'endorment, sous le plafond", async () => {
    // 6 onglets ouverts (budget 6). Memoire libre simulee a 300 Mo sous la reserve de 1 500 :
    // 3 onglets doivent s'endormir (300 / 120 arrondi au-dessus), jamais l'onglet affiche.
    const profil = dossierJetable("memoire-juste");
    spawnSync("node", [path.join(RACINE, "outils", "graine-session.mjs"), profil]);
    fs.writeFileSync(
      path.join(profil, "reglages.json"),
      JSON.stringify({ veille_auto: false, budget: 6, budget_auto: true, memoire_basse: false })
    );
    const session = JSON.parse(fs.readFileSync(path.join(profil, "session.json"), "utf8"));
    const { journal } = await lancer(profil, { TRAME_REVEILLER: "1", TRAME_MEMOIRE_LIBRE: "1200" }, 16000);
    verifier(
      /memoire libre 1200 Mo : 3 onglet\(s\) ouvert\(s\) au plus \(plafond 6\)/.test(journal),
      `budget abaisse attendu : ${journal.split("\n").filter((l) => l.includes("memoire libre")).join(" | ")}`
    );
    const endormis = [...journal.matchAll(/onglet (\d+) endormi/g)].map((m) => Number(m[1]));
    // 11 reveilles par la sonde, 5 endormis par le plafond de 6, puis 3 de plus par la memoire ;
    // la memoire simulee ne remonte pas apres ces mises en veille (une vraie, si), donc la
    // mesure suivante redescend encore — jusqu'au plancher de 2 onglets ouverts, jamais moins.
    verifier(endormis.length === 9, `9 mises en veille attendues (plancher de 2), obtenu ${endormis.length}`);
    verifier(
      /memoire libre 1200 Mo : 2 onglet\(s\) ouvert\(s\) au plus/.test(journal),
      "le plancher de 2 onglets doit etre atteint et journalise"
    );
    verifier(!endormis.includes(session.actif), "l'onglet affiche n'est jamais endormi");
    supprimer(profil);

    // Reglage coupe : le plafond choisi s'applique, quelle que soit la memoire libre.
    const p2 = dossierJetable("memoire-ignoree");
    spawnSync("node", [path.join(RACINE, "outils", "graine-session.mjs"), p2]);
    fs.writeFileSync(path.join(p2, "reglages.json"), JSON.stringify({ veille_auto: false, budget: 6, budget_auto: false }));
    const r2 = await lancer(p2, { TRAME_REVEILLER: "1", TRAME_MEMOIRE_LIBRE: "1200" }, 14000);
    const n2 = [...r2.journal.matchAll(/onglet (\d+) endormi/g)].length;
    verifier(n2 === 5, `reglage coupe : 5 mises en veille (plafond seul), obtenu ${n2}`);
    supprimer(p2);
  });

  if (args.includes("--reseau")) {
    await scenario("v. le cout d'une extension est mesure a son allumage (reseau)", async () => {
      const profil = dossierJetable("cout-extension");
      const { journal } = await lancer(profil, { TRAME_EXT: "ddkjiahejlhfcafbddmgiahcphecmpfh" }, 60000);
      // uBlock Origin Lite (MV3) ne lance son processus que quand il a a faire : selon le moment,
      // il y a un ecart a mesurer, ou aucun. Les deux issues sont justes ; ce qui ne l'est pas,
      // c'est une mesure absente, ou un chiffre absurde.
      const lignes = journal.split("\n").filter((l) => l.includes("extension ddkjiahejlhfcafbddmgiahcphecmpfh :"));
      const mesuree = lignes.find((l) => l.includes("Mo mesures"));
      const neant = lignes.find((l) => l.includes("non significatif"));
      verifier(mesuree || neant, `la mesure doit avoir lieu : ${journal.split("\n").filter((l) => l.includes("extension")).slice(-6).join(" | ")}`);
      const reglages = JSON.parse(fs.readFileSync(path.join(profil, "reglages.json"), "utf8"));
      const cout = reglages.extensions_cout && reglages.extensions_cout.ddkjiahejlhfcafbddmgiahcphecmpfh;
      if (mesuree) {
        verifier(cout && cout.mo > 1 && cout.mo < 1000, `cout plausible note dans reglages.json : ${JSON.stringify(cout)}`);
      } else {
        verifier(!cout, `rien d'invente quand il n'y a pas d'ecart : ${JSON.stringify(cout)}`);
      }
      supprimer(profil);
    });
  }

  await scenario("w. confidentialite : adresse nettoyee, anti-pistage, pause, coup d'oeil", async () => {
    // Un serveur local note chaque adresse reellement demandee : c'est la preuve que le pistage
    // n'est pas parti au serveur.
    const demandes = [];
    const serveur = http.createServer((req, rep) => {
      demandes.push(req.url);
      rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      // La page /lien suit d'elle-meme un lien sale : c'est le cas d'un clic sur un lien de page,
      // que seule la garde de navigation peut intercepter.
      const suite = req.url.startsWith("/lien")
        ? `<script>setTimeout(() => { location.href = "/depuis-page?id=7&utm_campaign=x&gclid=y"; }, 1500);</script>`
        : "";
      rep.end(`<!doctype html><html><body><p>${req.url}</p>${suite}</body></html>`);
    });
    await new Promise((r) => serveur.listen(0, "127.0.0.1", r));
    const base = `http://127.0.0.1:${serveur.address().port}`;
    try {
      const profil = dossierJetable("confidentialite");
      const maintenant = Date.now();
      const onglet = (id, chemin) => ({
        id, url: `${base}${chemin}`, titre: `T${id}`, fil: 1, parent: null, declencheur: "saisie",
        cree_le: maintenant, dernier_focus: maintenant, nb_focus: 1, temps_actif_ms: 0,
        defilement: 0, ancre: false, a_lire: false, vitalite: 1,
      });
      fs.writeFileSync(path.join(profil, "session.json"), JSON.stringify({
        onglets: [onglet(1, "/actif"), onglet(2, "/cache-a"), onglet(3, "/cache-b")],
        fils: [{ id: 1, nom: "Essai", auto_nom: false, cree_le: maintenant, actif_le: maintenant, replie: false }],
        actif: 1, rail_replie: false, prochain_onglet: 4, prochain_fil: 2,
      }));
      fs.writeFileSync(path.join(profil, "reglages.json"), JSON.stringify({ veille_auto: false, budget: 6 }));
      const { journal } = await lancer(profil, {
        TRAME_REVEILLER: "1",
        TRAME_PAUSE_APRES_S: "3",
        TRAME_NAV: `${base}/lien?id=42&utm_source=lettre&utm_medium=mail&fbclid=abc`,
        TRAME_APERCU: `${base}/apercu?x=1&gclid=zzz`,
      }, 22000);
      const lignes = (motif) => journal.split("\n").filter((l) => l.includes(motif));
      // 1. Nettoyage : le serveur n'a recu que l'adresse propre.
      verifier(demandes.includes("/lien?id=42"), `adresse propre attendue cote serveur : ${JSON.stringify(demandes)}`);
      verifier(demandes.includes("/depuis-page?id=7"), `lien suivi par la page, nettoye : ${JSON.stringify(demandes)}`);
      verifier(!demandes.some((d) => d.includes("utm_") || d.includes("fbclid")), `aucun pistage ne doit atteindre le serveur : ${JSON.stringify(demandes)}`);
      verifier(lignes("adresse nettoyee").length >= 1, "le nettoyage est journalise");
      // 2. Anti-pistage stricte par defaut.
      verifier(lignes("protection anti-pistage : stricte").length >= 1, `anti-pistage : ${lignes("anti-pistage").join(" | ")}`);
      // 3. Pause : les onglets caches s'arretent, jamais l'onglet affiche.
      const pauses = [...journal.matchAll(/onglet (\d+) : en pause/g)].map((m) => Number(m[1]));
      verifier(pauses.includes(2) && pauses.includes(3), `pause des onglets caches : ${lignes("pause").join(" | ")}`);
      verifier(!pauses.includes(1), "l'onglet affiche n'est jamais mis en pause");
      // 4. Coup d'oeil : ouvert, et sa page chargee (propre, elle aussi).
      verifier(lignes("coup d'oeil :").length >= 1, `coup d'oeil : ${lignes("coup d").join(" | ")}`);
      verifier(demandes.includes("/apercu?x=1"), `la page de l'apercu, nettoyee : ${JSON.stringify(demandes)}`);
      supprimer(profil);
    } finally {
      serveur.close();
    }
  });

  await scenario("x. un lien envoye a Trame deja ouvert arrive dans un nouvel onglet", async () => {
    // Trame navigateur par defaut : Windows lance `trame.exe "<lien>"`. Si Trame tourne deja, la
    // seconde instance doit transmettre le lien et s'arreter, sans ouvrir une seconde fenetre.
    const demandes = [];
    const serveur = http.createServer((req, rep) => {
      demandes.push(req.url);
      rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      rep.end("<!doctype html><p>ok</p>");
    });
    await new Promise((r) => serveur.listen(0, "127.0.0.1", r));
    const lien = `http://127.0.0.1:${serveur.address().port}/depuis-un-mail?id=3`;
    try {
      const profil = dossierJetable("lien-externe");
      const premiere = lancer(profil, {}, 16000);
      await new Promise((r) => setTimeout(r, 6000));
      const seconde = spawn(EXE, [lien], { env: { ...process.env, TRAME_PROFIL: profil }, stdio: "ignore" });
      const codeSeconde = await new Promise((r) => seconde.on("exit", r));
      const { journal } = await premiere;
      verifier(codeSeconde === 0, `la seconde instance doit s'arreter proprement (code ${codeSeconde})`);
      verifier(journal.includes("demande transmise"), "la seconde instance transmet sa demande");
      verifier(journal.includes("lien externe : "), `la premiere ouvre le lien : ${journal.split("\n").filter((l) => l.includes("lien")).join(" | ")}`);
      verifier(demandes.includes("/depuis-un-mail?id=3"), `la page est chargee : ${JSON.stringify(demandes)}`);
      supprimer(profil);

      // Trame ferme : le lien s'ouvre au demarrage.
      const profil2 = dossierJetable("lien-demarrage");
      demandes.length = 0;
      const r2 = await new Promise((resoudre) => {
        const e = spawn(EXE, [lien.replace("depuis-un-mail", "au-demarrage")], { env: { ...process.env, TRAME_PROFIL: profil2 }, stdio: "ignore" });
        setTimeout(() => { spawn("taskkill", ["/PID", String(e.pid), "/T", "/F"], { stdio: "ignore" }); }, 12000);
        e.on("exit", () => resoudre(fs.readFileSync(path.join(profil2, "trame.log"), "utf8")));
      });
      verifier(r2.includes("lien externe : "), "au demarrage, le lien s'ouvre aussi");
      verifier(demandes.includes("/au-demarrage?id=3"), `page du demarrage chargee : ${JSON.stringify(demandes)}`);
      supprimer(profil2);
    } finally {
      serveur.close();
    }
  });

  await scenario("y. outils : rendez-vous, fermeture, guet, memoire de lecture, Alt/Maj+Entree", async () => {
    // Tout passe par de vraies pages locales et par la sonde TRAME_IPC (messages de la barre
    // rejoues), sans souris ni focus.
    const demandes = [];
    const article =
      "Les batteries a electrolyte solide promettent une densite double et une charge plus sure. " +
      "Ce texte est assez long pour etre garde dans la memoire de lecture de Trame. ".repeat(4);
    const serveur = http.createServer((req, rep) => {
      demandes.push(req.url);
      rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      if (req.url.startsWith("/a")) rep.end(`<!doctype html><title>Article</title><p>${article}</p>`);
      else if (req.url.startsWith("/prix"))
        // Le prix s'affiche en JavaScript, apres le chargement : le guet doit l'attendre.
        rep.end(`<!doctype html><title>Disque</title><span id="prix"></span><script>setTimeout(()=>{document.getElementById("prix").textContent="34,90 €"},700)</script>`);
      else rep.end(`<!doctype html><title>Page ${req.url}</title><p>ok</p>`);
    });
    await new Promise((r) => serveur.listen(0, "127.0.0.1", r));
    const origine = `http://127.0.0.1:${serveur.address().port}`;
    try {
      const profil = dossierJetable("outils");
      const maintenant = Date.now();
      const base = (id, url, extra = {}) => ({ id, url, titre: "", fil: 1, cree_le: maintenant, dernier_focus: maintenant - id * 1000, vitalite: 0.8, ...extra });
      fs.writeFileSync(
        path.join(profil, "session.json"),
        JSON.stringify({
          onglets: [
            base(1, `${origine}/a`),
            base(2, `${origine}/b?deux`),
            base(3, `${origine}/b?rdv`, { rendez_vous: maintenant - 1000 }),
            base(4, `${origine}/b?fermer`, { fermeture: maintenant - 1000 }),
          ],
          fils: [{ id: 1, nom: "Achats", auto_nom: false, cree_le: maintenant, actif_le: maintenant, replie: false }],
          actif: 1,
          prochain_onglet: 5,
          prochain_fil: 2,
        })
      );
      fs.writeFileSync(
        path.join(profil, "guets.json"),
        JSON.stringify([{ id: 1, url: `${origine}/prix`, selecteur: "#prix", libelle: "Disque", texte: "39,90 €", verifie_le: 0, intervalle_min: 30, vu: true }])
      );
      const sonde = [
        ...Array(5).fill({ m: "pret" }),
        { m: "requete", rid: 5, q: "?electrolyte solide" },
        { m: "naviguer", url: `${origine}/b?alt=1`, mode: "onglet" },
        { m: "naviguer", url: `${origine}/b?cote=1`, mode: "cote" },
      ];
      const { journal } = await lancer(profil, { TRAME_IPC: JSON.stringify(sonde), TRAME_CACHEE: "1" }, 34000);
      const lignes = (motif) => journal.split("\n").filter((l) => l.includes(motif));
      const session = JSON.parse(fs.readFileSync(path.join(profil, "session.json"), "utf8"));
      const guets = JSON.parse(fs.readFileSync(path.join(profil, "guets.json"), "utf8"));
      const lectures = fs.existsSync(path.join(profil, "lectures.jsonl"))
        ? fs.readFileSync(path.join(profil, "lectures.jsonl"), "utf8")
        : "";

      // 1. Rendez-vous echu : l'onglet revient ; fermeture echue : l'onglet part.
      verifier(lignes("rendez-vous : onglet 3 revient").length === 1, `rendez-vous : ${lignes("rendez-vous").join(" | ")}`);
      verifier(!session.onglets.some((o) => o.id === 4), "l'onglet a fermeture programmee est ferme");
      verifier(session.onglets.some((o) => o.id === 3 && !o.rendez_vous), "le rendez-vous est solde dans la session");
      // 2. Memoire de lecture : la page affichee est gardee, puis retrouvee par une phrase.
      verifier(lectures.includes(`${origine}/a`) && lectures.includes("electrolyte solide"), `lectures.jsonl : ${lectures.slice(0, 200)}`);
      verifier(lignes("1 page(s) retrouvee(s) (requete 5)").length === 1, `recherche : ${lignes("memoire de lecture").join(" | ")}`);
      // 3. Alt+Entree : nouvel onglet DANS le fil, juste apres la page d'origine.
      const ordre = session.onglets.map((o) => o.url);
      const iAlt = ordre.findIndex((u) => u.includes("alt=1"));
      const iCote = ordre.findIndex((u) => u.includes("cote=1"));
      verifier(iAlt === ordre.indexOf(`${origine}/a`) + 1, `ordre apres Alt+Entree : ${JSON.stringify(ordre)}`);
      verifier(session.onglets[iAlt].fil === 1 && session.fils.length === 1, `fil : ${JSON.stringify(session.fils)}`);
      // 4. Maj+Entree : la page s'affiche a cote de celle d'ou l'on vient.
      verifier(iCote === iAlt + 1, `ordre apres Maj+Entree : ${JSON.stringify(ordre)}`);
      verifier(
        JSON.stringify(session.cote) === JSON.stringify([session.onglets[iAlt].id, session.onglets[iCote].id]),
        `cote a cote : ${JSON.stringify(session.cote)}`
      );
      // 5. Guet : la page est relue en cache, le prix (affiche en JavaScript) a change.
      verifier(demandes.includes("/prix"), `la page guettee est relue : ${JSON.stringify(demandes)}`);
      verifier(guets[0].texte === "34,90 €" && guets[0].ancien === "39,90 €" && guets[0].vu === false, `guet : ${JSON.stringify(guets[0])}`);
      verifier(lignes("Guet « Disque » : 39,90 € → 34,90 €").filter((l) => !l.includes("toast")).length === 1, `annonce du guet : ${lignes("uet").join(" | ")}`);
      verifier(!/panicked/.test(journal), "aucune panique");
      supprimer(profil);
    } finally {
      serveur.close();
    }
  });

  await scenario("t. fil cloisonne : ses cookies et ses extensions sont a part", async () => {
    // Trois fils, un onglet chacun, sur la meme origine locale :
    //   fil 1 (principal) pose un cookie ;
    //   fil 2 (CLOISONNE, onglet affiche) et fil 3 (principal, temoin) le lisent apres 3 s,
    //   et disent si la banniere de l'extension de preuve est dans leur page.
    const rapports = {};
    const serveur = http.createServer((req, rep) => {
      const u = new URL(req.url, "http://x");
      if (u.pathname === "/rapport") {
        rapports[u.searchParams.get("f")] = {
          cookie: u.searchParams.get("cookie") === "1",
          banniere: u.searchParams.get("banniere") === "1",
        };
        rep.writeHead(204).end();
        return;
      }
      const f = u.searchParams.get("f");
      const entetes = { "content-type": "text/html; charset=utf-8" };
      if (u.pathname === "/pose") entetes["set-cookie"] = "trame_cloison=1; Path=/; Max-Age=600";
      rep.writeHead(200, entetes);
      rep.end(`<!doctype html><html><body><p>fil ${f}</p><script>
        setTimeout(() => {
          const c = document.cookie.includes("trame_cloison=1") ? 1 : 0;
          const b = document.getElementById("preuve-chrome") ? 1 : 0;
          fetch("/rapport?f=${f}&cookie=" + c + "&banniere=" + b).catch(() => {});
        }, 3000);
      </script></body></html>`);
    });
    await new Promise((r) => serveur.listen(0, "127.0.0.1", r));
    const base = `http://127.0.0.1:${serveur.address().port}`;
    const source = path.join(RACINE, "exemples-chrome", "preuve-chrome");

    const preparer = (nom, extensionsCloison, dossierExtension) => {
      const profil = dossierJetable(nom);
      const cible = path.join(profil, dossierExtension, "preuve-chrome");
      fs.mkdirSync(cible, { recursive: true });
      for (const f of fs.readdirSync(source)) fs.copyFileSync(path.join(source, f), path.join(cible, f));
      const maintenant = Date.now();
      const fil = (id, cloison) => ({
        id, nom: `Fil ${id}`, auto_nom: false, cree_le: maintenant, actif_le: maintenant, replie: false,
        ...(cloison ? { cloison: { profil: `cloison-${id}`, extensions: extensionsCloison } } : {}),
      });
      const onglet = (id, chemin) => ({
        id, url: `${base}${chemin}?f=${id}`, titre: `Fil ${id}`, fil: id, parent: null, declencheur: "saisie",
        cree_le: maintenant, dernier_focus: maintenant, nb_focus: 1, temps_actif_ms: 0,
        defilement: 0, ancre: false, a_lire: false, vitalite: 1,
      });
      fs.writeFileSync(path.join(profil, "session.json"), JSON.stringify({
        onglets: [onglet(1, "/pose"), onglet(2, "/lit"), onglet(3, "/lit")],
        fils: [fil(1, false), fil(2, true), fil(3, false)],
        actif: 2, rail_replie: false, prochain_onglet: 4, prochain_fil: 4,
      }));
      fs.writeFileSync(path.join(profil, "reglages.json"), JSON.stringify({ veille_auto: false, budget: 6 }));
      return profil;
    };

    try {
      // 1. Cloison sans extension, extension active dans le profil principal.
      const p1 = preparer("cloison-1", [], "extensions-chrome");
      const r1 = await lancer(p1, { TRAME_REVEILLER: "1" }, 16000);
      verifier(rapports["3"] && rapports["2"], `rapports attendus des fils 2 et 3 : ${JSON.stringify(rapports)} — ${r1.journal.split("\n").filter((l) => l.includes("cloison")).join(" | ")}`);
      verifier(rapports["3"].cookie, "temoin : le fil 3 (principal) voit le cookie du fil 1");
      verifier(!rapports["2"].cookie, "le fil cloisonne ne voit PAS le cookie du profil principal");
      verifier(rapports["3"].banniere, "temoin : l'extension tourne dans le profil principal");
      verifier(!rapports["2"].banniere, "l'extension ne tourne PAS dans la cloison qui ne l'a pas");
      supprimer(p1);

      // 2. L'inverse : extension coupee partout, voulue dans la seule cloison.
      for (const k of Object.keys(rapports)) delete rapports[k];
      const p2 = preparer("cloison-2", ["preuve-chrome"], "extensions-chrome-desactivees");
      const r2 = await lancer(p2, { TRAME_REVEILLER: "1" }, 20000);
      verifier(rapports["2"] && rapports["3"], `rapports attendus : ${JSON.stringify(rapports)}\n${r2.journal.split("\n").slice(8).join("\n")}`);
      verifier(rapports["2"].banniere, `l'extension tourne dans sa cloison : ${r2.journal.split("\n").filter((l) => l.includes("cloison")).join(" | ")}`);
      verifier(!rapports["3"].banniere, "et nulle part ailleurs");
      supprimer(p2);
    } finally {
      serveur.close();
    }
  });

  await scenario("r. desactiver puis reactiver une extension Chrome, a chaud", async () => {
    // Une extension Chrome deja installee dans le profil (dossier + cle), chargee au demarrage.
    const source = path.join(RACINE, "exemples-chrome", "preuve-chrome");
    verifier(fs.existsSync(path.join(source, "manifest.json")), `exemple absent : ${source}`);
    const profil = dossierJetable("chaud");
    const cible = path.join(profil, "extensions-chrome", "preuve-chrome");
    fs.mkdirSync(cible, { recursive: true });
    for (const f of fs.readdirSync(source)) fs.copyFileSync(path.join(source, f), path.join(cible, f));
    // `TRAME_EXT_CHROME` : sonde qui eteint puis rallume l'extension au fil des battements.
    const { journal } = await lancer(profil, { TRAME_EXT_CHROME: "preuve-chrome" }, 16000);
    const lignes = journal.split("\n").filter((l) => l.includes("moteur d'extensions"));
    verifier(
      lignes.some((l) => l.includes("Extinction de") && l.includes("fait")),
      `extinction a chaud attendue : ${lignes.join(" | ")}`
    );
    verifier(
      lignes.some((l) => l.includes("Allumage de") && l.includes("fait")),
      `rallumage a chaud attendu : ${lignes.join(" | ")}`
    );
    // Pas un faux succes (« deja absente ») : le moteur l'a vraiment listee eteinte. Ce dossier
    // n'a pas de cle, le moteur lui donne l'empreinte du chemin — Trame la retrouve par son nom.
    verifier(
      lignes.some((l) => l.includes("Preuve Chrome (eteinte)")),
      `le moteur doit lister l'extension eteinte : ${lignes.join(" | ")}`
    );
    verifier(
      fs.existsSync(path.join(profil, "extensions-chrome", "preuve-chrome", "manifest.json")),
      "apres rallumage, le dossier est revenu parmi les extensions actives"
    );
    supprimer(profil);
  });

  if (args.includes("--reseau")) {
    await scenario("m. installation depuis le Chrome Web Store (reseau)", async () => {
      // Extension volontairement petite : le but est le chemin complet (telechargement,
      // en-tete CRX, decompression), pas les 33 Mo de Bitwarden.
      const identifiant = "ddkjiahejlhfcafbddmgiahcphecmpfh"; // uBlock Origin Lite
      const profil = dossierJetable("magasin");
      // 60 s : le magasin a deja mis 29 s pour ces 9 Mo (mesure du 2026-11-21), et le
      // telechargement du champ « Installer » bloque la boucle d'evenements le temps du transfert.
      const { journal, sortie } = await lancer(profil, { TRAME_EXT: identifiant }, 60000);
      verifier(
        journal.includes("installation d'extension") && journal.includes(identifiant),
        `journal: ${journal.slice(0, 400)}`
      );
      const dossier = path.join(profil, "extensions-chrome", identifiant);
      verifier(
        fs.existsSync(path.join(dossier, "manifest.json")),
        `extension non installee. journal: ${journal.slice(-800)} sortie: ${sortie.slice(-400)}`
      );
      const sources = JSON.parse(
        fs.readFileSync(path.join(profil, "extensions-chrome-sources.json"), "utf8")
      );
      verifier(sources[identifiant] && sources[identifiant].source === "magasin", "provenance magasin");
      supprimer(profil);
    });
  }

  await scenario("n. tout ce dont le navigateur a besoin est dans le profil", async () => {
    const profil = dossierJetable("profil-complet");
    const { journal } = await lancer(profil);
    // 1. l'interface est installee dans le profil, pas seulement lue a cote de l'executable
    verifier(journal.includes("interface installee dans le profil"), `journal: ${journal.slice(0, 600)}`);
    const ui = path.join(profil, "ui");
    verifier(fs.existsSync(path.join(ui, "chrome.html")), "ui/chrome.html doit exister dans le profil");
    verifier(fs.existsSync(path.join(ui, "chrome.js")), "ui/chrome.js doit exister dans le profil");
    verifier(fs.existsSync(path.join(ui, "reglages.html")), "ui/reglages.html doit exister");
    // 2. le profil WebView2 (cookies, connexions, extensions) y est aussi
    verifier(journal.includes("profil WebView2 :"), `journal: ${journal.slice(0, 600)}`);
    verifier(
      fs.existsSync(path.join(profil, "webview2", "EBWebView")),
      `webview2/EBWebView attendu. Contenu : ${JSON.stringify(fs.readdirSync(profil))}`
    );
    // 3. rien n'est cree a cote de l'executable
    const voisin = path.join(RACINE, "target", "release", `${path.basename(EXE)}.WebView2`);
    verifier(
      !fs.existsSync(voisin),
      `le dossier WebView2 ne doit plus etre cree a cote de l'executable : ${voisin}`
    );
    // 4. les extensions livrees arrivent dans le profil (une seule fois)
    const extensions = fs.readdirSync(path.join(profil, "extensions"));
    verifier(extensions.length >= 3, `3 extensions attendues, ${extensions.length}`);
    // 5. au demarrage suivant, l'interface n'est pas recopiee (miroir sans churn)
    const second = await lancer(profil);
    verifier(
      second.journal.includes("interface deja a jour"),
      `le second demarrage doit dire « deja a jour » : ${second.journal.slice(0, 600)}`
    );
    supprimer(profil);
  });

  await scenario("o. l'activation d'une extension se memorise et s'applique au redemarrage", async () => {
    // Le cas qui se perdait : une extension livree ETEINTE (`enabled_by_default: false`).
    // L'allumer ne laissait aucune trace, donc le demarrage suivant la retrouvait eteinte.
    // Ici on l'allume, on redemarre, et on exige une preuve d'injection reelle : le script de
    // contenu ecrit dans `chrome.storage.local` l'adresse ou il s'est execute, et c'est
    // `ext-storage.json` (fichier du profil) qu'on relit. Puis on la coupe, et on verifie que
    // le silence tient aussi au redemarrage.
    const profil = dossierJetable("ext-memoire");
    const dossier = path.join(profil, "extensions", "essai");
    fs.mkdirSync(dossier, { recursive: true });
    fs.writeFileSync(
      path.join(dossier, "manifest.json"),
      JSON.stringify({
        name: "Essai",
        version: "1.0.0",
        matches: ["http://127.0.0.1/*"],
        js: ["contenu.js"],
        run_at: "document_end",
        enabled_by_default: false,
      })
    );
    fs.writeFileSync(
      path.join(dossier, "contenu.js"),
      "chrome.storage.local.set({ vu: location.href });"
    );

    const serveur = http.createServer((_req, rep) => {
      rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      rep.end("<!doctype html><title>essai</title><p>page de test</p>");
    });
    await new Promise((r) => serveur.listen(0, "127.0.0.1", r));
    const base = `http://127.0.0.1:${serveur.address().port}`;
    const reglages = () =>
      JSON.parse(fs.readFileSync(path.join(profil, "reglages.json"), "utf8"));
    const vu = () => {
      const f = path.join(profil, "ext-storage.json");
      if (!fs.existsSync(f)) return null;
      return (JSON.parse(fs.readFileSync(f, "utf8")).essai || {}).vu || null;
    };
    // Le journal s'ajoute d'un demarrage a l'autre : on ne juge que le dernier.
    const dernier = (journal) => {
      const i = journal.lastIndexOf("Trame demarre");
      return i < 0 ? journal : journal.slice(i);
    };

    try {
      // 1. on l'allume : le choix doit partir sur le disque
      const r1 = await lancer(profil, { TRAME_EXT_ETAT: "essai", TRAME_NAV: `${base}/page-1` });
      verifier(
        /extension essai : activee \(choix memorise\)/.test(dernier(r1.journal)),
        `l'allumage doit etre journalise. journal: ${r1.journal.slice(0, 900)}`
      );
      verifier(
        reglages().extensions_etat && reglages().extensions_etat.essai === true,
        `reglages.json doit porter le choix : ${JSON.stringify(reglages().extensions_etat)}`
      );
      verifier(vu() === `${base}/page-1`, `l'extension devrait s'injecter des le premier run : ${vu()}`);

      // 2. redemarrage SANS sonde : elle doit rester allumee, et s'injecter pour de vrai
      const r2 = await lancer(profil, { TRAME_NAV: `${base}/page-2` });
      const j2 = dernier(r2.journal);
      verifier(
        /extension essai : activee \(choix memorise\)/.test(j2),
        `le choix doit etre relu au demarrage. journal: ${j2.slice(0, 900)}`
      );
      verifier(
        /extensions : 1 active\(s\)/.test(j2),
        `elle doit compter parmi les actives. journal: ${j2.slice(0, 900)}`
      );
      verifier(
        vu() === `${base}/page-2`,
        `l'extension doit s'injecter apres redemarrage (vu = ${vu()})`
      );

      // 3. on la coupe : la coupure doit tenir aussi
      const r3 = await lancer(profil, { TRAME_EXT_ETAT: "essai=off", TRAME_NAV: `${base}/page-3` });
      verifier(
        /extension essai : coupee \(choix memorise\)/.test(dernier(r3.journal)),
        `la coupure doit etre journalisee. journal: ${dernier(r3.journal).slice(0, 900)}`
      );
      verifier(
        reglages().extensions_etat.essai === false,
        `reglages.json doit porter la coupure : ${JSON.stringify(reglages().extensions_etat)}`
      );
      verifier(vu() === `${base}/page-2`, `coupee, elle ne doit plus s'injecter (vu = ${vu()})`);

      // 4. et elle le reste apres un redemarrage : toujours aucune injection
      const r4 = await lancer(profil, { TRAME_NAV: `${base}/page-4` });
      const j4 = dernier(r4.journal);
      verifier(
        /extension essai : coupee \(choix memorise\)/.test(j4),
        `la coupure doit etre relue au demarrage. journal: ${j4.slice(0, 900)}`
      );
      verifier(
        /extensions : 0 active\(s\)/.test(j4),
        `aucune extension ne doit etre active. journal: ${j4.slice(0, 900)}`
      );
      verifier(vu() === `${base}/page-2`, `toujours pas d'injection apres redemarrage (vu = ${vu()})`);
    } finally {
      serveur.close();
      supprimer(profil);
    }
  });

  await scenario("p. un .crx telecharge hors du magasin n'est pas installe tout seul", async () => {
    // La regle : Trame n'installe pas en silence un paquet venu d'ailleurs. Un `.crx` telecharge
    // depuis un site ordinaire doit rester un fichier dans « telechargements ».
    let telecharge = false;
    const serveur = http.createServer((req, rep) => {
      if (req.url.endsWith(".crx")) {
        telecharge = true;
        // Un vrai debut de CRX3 : si Trame l'installait, le test s'en apercevrait.
        const debut = Buffer.alloc(12);
        debut.write("Cr24", 0, "latin1");
        debut.writeUInt32LE(3, 4);
        debut.writeUInt32LE(16, 8);
        rep.writeHead(200, {
          "content-type": "application/x-chrome-extension",
          "content-disposition": 'attachment; filename="essai.crx"',
        });
        rep.end(Buffer.concat([debut, Buffer.alloc(16), Buffer.from("PK\x03\x04zip")]));
        return;
      }
      rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      rep.end(
        `<!doctype html><title>telechargement</title>
         <script>setTimeout(() => { location.href = "/essai.crx"; }, 1200);</script>`
      );
    });
    await new Promise((r) => serveur.listen(0, "127.0.0.1", r));
    const url = `http://127.0.0.1:${serveur.address().port}/page.html`;
    const profil = dossierJetable("crx-hors-magasin");
    const { journal } = await lancer(profil, { TRAME_NAV: url });
    serveur.close();
    verifier(telecharge, `la page n'a pas declenche de telechargement. journal: ${journal.slice(0, 600)}`);
    // Le fichier est la preuve du telechargement (sur un paquet minuscule, l'evenement de fin
    // peut arriver avant que le gestionnaire soit branche : on ne juge pas sur le journal).
    const fichiers = fs.readdirSync(path.join(profil, "telechargements"));
    verifier(
      fichiers.some((f) => f.endsWith(".crx")),
      `le paquet doit rester telecharge : ${JSON.stringify(fichiers)}`
    );
    const installees = fs.existsSync(path.join(profil, "extensions-chrome"))
      ? fs.readdirSync(path.join(profil, "extensions-chrome"))
      : [];
    verifier(installees.length === 0, `aucune installation attendue, trouvé ${JSON.stringify(installees)}`);
    supprimer(profil);
  });

  if (args.includes("--reseau")) {
    await scenario("q. « Ajouter a Chrome » depuis le magasin installe l'extension", async () => {
      // Ce que fait vraiment le clic sur une fiche du magasin : Chromium ne sait pas installer,
      // il telecharge le `.crx`. Trame doit le reprendre — le scenario `m` ne couvrait pas ce
      // chemin, il passe par le champ « identifiant » des reglages.
      const identifiant = "eimadpbcbfnmbkopoojfekhnkhdbieeh"; // Dark Reader, ~860 Ko
      const url =
        "https://clients2.google.com/service/update2/crx?response=redirect&os=win&arch=x86-64" +
        "&os_arch=x86-64&nacl_arch=x86-64&prod=chromecrx&prodchannel=unknown&prodversion=141.0.0.0" +
        `&acceptformat=crx2,crx3&x=id%3D${identifiant}%26uc`;
      const profil = dossierJetable("ajouter-a-chrome");
      const { journal } = await lancer(profil, { TRAME_NAV: url }, 40000);
      verifier(
        /telechargement :/.test(journal),
        `le magasin doit declencher un telechargement. journal: ${journal.slice(-900)}`
      );
      const dossier = path.join(profil, "extensions-chrome", identifiant);
      verifier(
        fs.existsSync(path.join(dossier, "manifest.json")),
        `extension non installee. journal: ${journal.slice(-900)}`
      );
      // La cle publique du magasin est inscrite dans le manifeste : c'est elle qui fixe
      // l'identifiant que Chromium attribue (mesure : sans elle, l'identifiant vient du chemin).
      const manifeste = JSON.parse(fs.readFileSync(path.join(dossier, "manifest.json"), "utf8"));
      verifier(
        typeof manifeste.key === "string" && manifeste.key.length > 100,
        `cle publique attendue dans le manifeste : ${JSON.stringify(manifeste.key)}`
      );
      const sources = JSON.parse(
        fs.readFileSync(path.join(profil, "extensions-chrome-sources.json"), "utf8")
      );
      verifier(sources[identifiant] && sources[identifiant].source === "magasin", "provenance magasin");
      supprimer(profil);
    });
  }

  // Balayage final : un dossier qui resiste juste apres le `taskkill` est repris ici, une fois
  // les processus WebView2 vraiment morts — sinon `%TEMP%` se remplirait d'un profil par run.
  for (const nom of fs.readdirSync(os.tmpdir())) {
    if (nom.startsWith("trame-verif-") && nom.endsWith(`-${process.pid}`)) {
      supprimer(path.join(os.tmpdir(), nom));
    }
  }

  const echecs = resultats.filter((r) => !r.ok);
  console.log(
    `\n${resultats.length - echecs.length}/${resultats.length} scenario(s) passe(s)` +
      (echecs.length ? `, ${echecs.length} echec(s)` : "")
  );
  for (const e of echecs) console.log(`  - ${e.nom}: ${e.erreur}`);
  process.exit(echecs.length ? 1 : 0);
}

suite().catch((e) => {
  console.error("suite interrompue :", e);
  process.exit(2);
});
