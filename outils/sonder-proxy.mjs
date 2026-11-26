#!/usr/bin/env node
/**
 * SONDE — un proxy passé au moteur est-il réellement utilisé ?
 *
 * Question de Clément (2026-11-27) : « est-ce qu'on peut prévoir l'utilisation de VPN ? »
 * Avant de promettre un réglage, on mesure le mécanisme : un faux serveur SOCKS5 local, Trame
 * lancée avec `--proxy-server=socks5://127.0.0.1:PORT` (c'est exactement ce que wry ajoute quand on
 * lui donne `ProxyConfig::Socks5`), et on regarde si le moteur vient frapper le proxy — et avec
 * quel nom d'hôte (c'est ce qui dit si la résolution DNS passe par le proxy ou fuit en local).
 *
 * Usage : node outils/sonder-proxy.mjs
 */
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:net";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const PORT_CDP = 19484;
const PORT_SOCKS = 19485;
const BINAIRE = join(process.cwd(), "target", "release", "trame.exe");
const profil = mkdtempSync(join(tmpdir(), "trame-sonde-proxy-"));

/** Un SOCKS5 minimal qui NOTE la demande et refuse poliment (0x01 = échec général). */
const demandes = [];
const socks = createServer((soc) => {
  let etape = 0;
  let tampon = Buffer.alloc(0);
  soc.on("data", (d) => {
    tampon = Buffer.concat([tampon, d]);
    if (etape === 0 && tampon.length >= 2) {
      // Poignée de main : 05, nombre de méthodes, méthodes. On accepte « sans authentification ».
      const n = tampon[1];
      if (tampon.length < 2 + n) return;
      tampon = tampon.subarray(2 + n);
      soc.write(Buffer.from([0x05, 0x00]));
      etape = 1;
    } else if (etape === 1 && tampon.length >= 5) {
      // Requête : 05, 01 (connect), 00, type, puis l'adresse.
      const type = tampon[3];
      let hote = "";
      let reste;
      if (type === 0x03) {
        const longueur = tampon[4];
        if (tampon.length < 5 + longueur + 2) return;
        hote = tampon.subarray(5, 5 + longueur).toString("latin1");
        reste = tampon.subarray(5 + longueur);
      } else if (type === 0x01) {
        if (tampon.length < 4 + 4 + 2) return;
        hote = Array.from(tampon.subarray(4, 8)).join(".");
        reste = tampon.subarray(8);
      } else {
        soc.destroy();
        return;
      }
      const port = reste.readUInt16BE(0);
      demandes.push({ hote, port });
      console.log(`  le moteur demande : ${hote}:${port}`);
      // Refus : 05, 01 (échec), 00, 01 (IPv4), 0.0.0.0:0
      soc.write(Buffer.from([0x05, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]));
      soc.end();
      etape = 2;
    }
  });
  soc.on("error", () => {
    /* le client a coupé */
  });
});
await new Promise((r) => socks.listen(PORT_SOCKS, "127.0.0.1", r));

const enfant = spawn(BINAIRE, [], {
  env: {
    ...process.env,
    TRAME_PROFIL: profil,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP} --proxy-server=socks5://127.0.0.1:${PORT_SOCKS}`,
  },
  stdio: "ignore",
});
const tuer = () => {
  try {
    execFileSync("taskkill", ["/PID", String(enfant.pid)], { stdio: "ignore" });
  } catch {
    /* deja parti */
  }
};

try {
  const rail = await connecter((await attendreCible(PORT_CDP, "surface=rail")).webSocketDebuggerUrl);
  await rail.envoyer("Runtime.enable");
  const barre = await connecter((await attendreCible(PORT_CDP, "surface=barre")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await pause(3000);

  console.log("Trame lancée avec --proxy-server=socks5://127.0.0.1:" + PORT_SOCKS);
  await barre.evaluer(`(() => {
    const c = document.getElementById("couture");
    c.focus();
    c.value = "https://example.com";
    c.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", altKey: false, bubbles: true, cancelable: true }));
  })()`);
  await pause(8000);

  console.log("\ndemandes reçues par le proxy :", JSON.stringify(demandes));
  if (demandes.length === 0) {
    console.log("VERDICT : le moteur n'est PAS passé par le proxy — le réglage serait inutile en l'état.");
  } else {
    const nom = demandes.find((d) => d.hote.includes("example.com"));
    console.log(
      nom
        ? `VERDICT : le moteur passe par le proxy ET lui donne le NOM (${nom.hote}:${nom.port}) — résolution DNS distante, pas de fuite locale sur ce cas.`
        : "VERDICT : le moteur passe par le proxy, mais pas pour example.com — à regarder."
    );
  }

  barre.fermer();
  rail.fermer();
} finally {
  tuer();
  socks.close();
  await pause(1000);
  try {
    rmSync(profil, { recursive: true, force: true });
  } catch {
    /* le ménage de Windows reprendra */
  }
}
