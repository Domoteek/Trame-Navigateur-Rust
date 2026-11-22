/**
 * Le peu de CDP dont les bancs d'essai de Trame ont besoin : un client minimal (une promesse
 * par message), la liste des cibles d'un Trame lançé avec `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`,
 * et l'attente d'une cible.
 *
 * Sans dépendance : le WebSocket natif de Node (>= 22) suffit, et les outils du projet n'ont
 * jamais eu de `node_modules`.
 */

export const pause = (ms) => new Promise((r) => setTimeout(r, ms));

/** Un client CDP minimal : une promesse par message, par identifiant. */
export class Cdp {
  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.attente = new Map();
    ws.addEventListener("message", (e) => {
      const m = JSON.parse(e.data);
      if (m.id && this.attente.has(m.id)) {
        const { resoudre, rejeter } = this.attente.get(m.id);
        this.attente.delete(m.id);
        if (m.error) rejeter(new Error(JSON.stringify(m.error)));
        else resoudre(m.result);
      }
    });
  }

  envoyer(method, params = {}) {
    this.id += 1;
    const id = this.id;
    return new Promise((resoudre, rejeter) => {
      this.attente.set(id, { resoudre, rejeter });
      this.ws.send(JSON.stringify({ id, method, params }));
      setTimeout(() => {
        if (this.attente.has(id)) {
          this.attente.delete(id);
          rejeter(new Error(`CDP ${method} : pas de reponse`));
        }
      }, 20000);
    });
  }

  /** Evalue une expression dans la page et rend sa valeur. */
  async evaluer(expression) {
    const r = await this.envoyer("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (r.exceptionDetails) {
      throw new Error(`page : ${r.exceptionDetails.text} ${r.exceptionDetails.exception?.description || ""}`);
    }
    return r.result.value;
  }

  /** Attend qu'une expression devienne vraie. */
  async attendre(expression, { delai = 60000, pas = 500, quoi = expression } = {}) {
    const fin = Date.now() + delai;
    let derniere = "";
    while (Date.now() < fin) {
      try {
        if (await this.evaluer(expression)) return true;
      } catch (e) {
        derniere = e.message;
      }
      await pause(pas);
    }
    throw new Error(`attente expiree : ${quoi}${derniere ? ` (${derniere})` : ""}`);
  }

  fermer() {
    try {
      this.ws.close();
    } catch {
      /* deja ferme */
    }
  }
}

/** Les cibles CDP du port, ou une liste vide si le port n'ecoute pas (encore). */
export async function cibles(port) {
  try {
    const r = await fetch(`http://127.0.0.1:${port}/json/list`);
    return await r.json();
  } catch {
    return [];
  }
}

/** La premiere cible « page » dont l'adresse contient `contient`. */
export async function attendreCible(port, contient, delai = 45000) {
  const fin = Date.now() + delai;
  while (Date.now() < fin) {
    const c = (await cibles(port)).find((t) => t.type === "page" && t.url.includes(contient));
    if (c) return c;
    await pause(400);
  }
  throw new Error(`aucune cible CDP ne contient ${contient} (Trame a-t-elle demarre ?)`);
}

/** Ouvre une session sur une cible. */
export function connecter(url) {
  return new Promise((resoudre, rejeter) => {
    const ws = new WebSocket(url);
    ws.addEventListener("open", () => resoudre(new Cdp(ws)));
    ws.addEventListener("error", () => rejeter(new Error(`WebSocket refuse : ${url}`)));
  });
}
