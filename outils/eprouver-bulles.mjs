/**
 * Éprouve les infobulles de la barre dans la **vraie application** (`ui/bulles.js`).
 *
 * Une bulle qui ne tient pas dans la barre (45 à 85 px de haut) devient une carte sous la barre :
 * l'interface demande la colonne du panneau (`panneau` + `zone`), l'hôte agrandit la webview de
 * la barre et ne découpe que la carte (`App::superposer`). Le navigateur des essais d'interface
 * ne sait pas faire cette moitié-là ; ce banc la mesure sur le binaire :
 *
 *   1. Trame d'essai, profil jetable, fenêtre cachée, port CDP ;
 *   2. un bouton à bulle longue posé en bas de la barre, survolé par une VRAIE souris CDP ;
 *   3. la barre doit grandir (au moins 150 px) et montrer la carte ; la souris partie, elle
 *      doit revenir à sa hauteur.
 *
 * Usage : node outils/eprouver-bulles.mjs [--exe chemin\trame.exe] [--capture fichier.png]
 */
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { attendreCible, connecter, pause } from "./cdp.mjs";

const RACINE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const option = (nom, defaut) => {
  const i = args.indexOf(`--${nom}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : defaut;
};
const EXE = path.resolve(RACINE, option("exe", path.join("target", "release", "trame.exe")));
const CAPTURE = option("capture", "");
const PORT = 9337;

async function principale() {
  if (!fs.existsSync(EXE)) {
    console.error(`Binaire absent : ${EXE}`);
    process.exit(2);
  }
  const profil = fs.mkdtempSync(path.join(os.tmpdir(), "trame-bulles-"));
  const enfant = spawn(EXE, [], {
    cwd: path.dirname(EXE),
    env: {
      ...process.env,
      TRAME_PROFIL: profil,
      TRAME_CACHEE: "1",
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
    },
    stdio: "ignore",
  });
  let cdp = null;
  let ok = true;
  const dire = (bon, texte) => {
    console.log(`${bon ? "ok  " : "ECHEC"} ${texte}`);
    if (!bon) ok = false;
  };
  try {
    cdp = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
    await cdp.envoyer("Runtime.enable");
    await cdp.attendre(`!!window.__trameBulles && innerHeight > 30`, { delai: 30000, quoi: "barre prete" });
    await pause(1500);
    const base = await cdp.evaluer("innerHeight");
    console.log(`barre    : ${base} px de haut`);

    // Un bouton à bulle longue, en bas à gauche de la barre : rien ne tient ni dessus ni dessous.
    const pt = await cdp.evaluer(`(() => {
      const b = document.createElement("button");
      b.id = "essai-bulle";
      b.textContent = "essai";
      b.title = "Compteur de visites v1.0.0\\nCompte combien de fois tu es venu sur ce site, et l'affiche discrètement.\\n*://*/*\\nCliquer pour désactiver";
      // Pose par le haut, comme les vraies puces : collé au bas, il descendrait avec la barre
      // agrandie et la souris le quitterait.
      b.style.cssText = "position:fixed;left:200px;top:" + (innerHeight - 22) + "px;height:20px;z-index:99";
      document.body.appendChild(b);
      const r = b.getBoundingClientRect();
      return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
    })()`);
    // Journal de bord : messages envoyes a l'hote, hauteurs, pertes de focus.
    await cdp.evaluer(`(() => {
      const t0 = performance.now();
      const j = (window.__journal = []);
      const note = (q) => j.push(Math.round(performance.now() - t0) + " " + q);
      const envoi = window.ipc.postMessage.bind(window.ipc);
      window.ipc.postMessage = (m) => { if (/panneau/.test(m)) note("ipc " + m); return envoi(m); };
      window.addEventListener("blur", () => note("blur"), true);
      window.addEventListener("resize", () => note("resize " + innerHeight));
      document.addEventListener("pointerout", (e) => note("out " + (e.target.id || e.target.className)), true);
    })()`);
    await cdp.envoyer("Input.dispatchMouseEvent", { type: "mouseMoved", x: pt.x, y: pt.y, buttons: 0 });
    await cdp.attendre(`innerHeight >= 150`, { delai: 5000, quoi: "la barre grandit" }).catch(() => {});
    const grande = await cdp.evaluer("innerHeight");
    dire(grande >= 150, `l'hôte agrandit la barre pour la carte : ${base} → ${grande} px`);
    console.log((await cdp.evaluer("window.__journal")).map((l) => "    " + l).join("\n"));
    const carte = await cdp.evaluer(`(() => {
      const b = document.querySelector(".trame-bulle");
      const r = b.getBoundingClientRect();
      return { classe: b.className, haut: Math.round(r.top), bas: Math.round(r.bottom), titre: b.querySelector(".b-titre")?.textContent };
    })()`);
    dire(/carte/.test(carte.classe) && /visible/.test(carte.classe), `carte visible ${JSON.stringify(carte)}`);
    dire(carte.haut > base && carte.bas <= grande, "la carte est sous la barre, dans la zone agrandie");
    if (CAPTURE) {
      const img = await cdp.envoyer("Page.captureScreenshot", { format: "png" });
      fs.writeFileSync(CAPTURE, Buffer.from(img.data, "base64"));
      console.log(`capture  : ${CAPTURE}`);
    }

    // La souris part en haut de la barre : la carte se rend, la barre reprend sa hauteur.
    await cdp.envoyer("Input.dispatchMouseEvent", { type: "mouseMoved", x: 700, y: 5, buttons: 0 });
    await cdp.attendre(`innerHeight === ${base}`, { delai: 5000, quoi: "la barre revient" }).catch(() => {});
    const apres = await cdp.evaluer("innerHeight");
    dire(apres === base, `la barre revient à ${apres} px`);
  } catch (e) {
    dire(false, e.message);
  } finally {
    if (cdp) cdp.fermer();
    enfant.kill();
    await pause(800);
    try {
      fs.rmSync(profil, { recursive: true, force: true });
    } catch {
      /* WebView2 garde parfois un verrou quelques secondes */
    }
  }
  console.log(ok ? "\nbulles : tout est bon" : "\nbulles : ECHEC");
  process.exit(ok ? 0 : 1);
}

principale();
