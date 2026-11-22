/**
 * Barre de recherche dans la page (Ctrl+F), injectee dans l'onglet actif par Trame.
 *
 * Contraintes de depart :
 * - elle vit DANS la page visitee : aucun cadre a nous, donc elle doit se dessiner elle-meme
 *   et rester discrete (coin haut droit, ombre, `position: fixed`, z-index maximal) ;
 * - elle ne doit pas etre reecrite si elle existe deja : `Ctrl+F` deux fois de suite doit
 *   juste remettre le focus dans le champ, pas empiler deux barres ;
 * - elle utilise `window.find`, l'API de recherche native de WebView2 : elle selectionne le
 *   texte dans la page comme le ferait le navigateur, sans toucher au DOM du site ;
 * - `Echap` referme et **rend le focus a la page**, sinon le clavier reste captif.
 *
 * Le fichier est embarque a la compilation (`include_str!` dans `app.rs`) : il ne depend pas
 * du dossier `ui/` au moment du raccourci.
 */
(() => {
  const ID = "trame-recherche";
  const existante = document.getElementById(ID);
  if (existante) {
    const champ = existante.querySelector("input");
    if (champ) {
      champ.focus();
      champ.select();
    }
    return;
  }

  const boite = document.createElement("div");
  boite.id = ID;
  // Styles ecrits en une fois : `all: initial` isole la barre des feuilles du site.
  boite.style.cssText = [
    "all: initial",
    "position: fixed",
    "top: 12px",
    "right: 14px",
    "z-index: 2147483647",
    "display: flex",
    "align-items: center",
    "gap: 6px",
    "padding: 6px 8px",
    "border-radius: 10px",
    "background: #12161f",
    "border: 1px solid rgba(160, 180, 210, 0.35)",
    "box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45)",
    "font: 13px/1.4 'Segoe UI', system-ui, sans-serif",
    "color: #e6ecf6",
  ].join(";");

  const champ = document.createElement("input");
  champ.type = "text";
  champ.placeholder = "Chercher dans la page";
  champ.setAttribute("aria-label", "Chercher dans la page");
  champ.style.cssText = [
    "all: initial",
    "font: 13px 'Segoe UI', system-ui, sans-serif",
    "color: #e6ecf6",
    "background: rgba(255, 255, 255, 0.06)",
    "border: 1px solid rgba(160, 180, 210, 0.25)",
    "border-radius: 7px",
    "padding: 5px 8px",
    "width: 220px",
    "outline: none",
  ].join(";");

  const compte = document.createElement("span");
  compte.textContent = "";
  compte.style.cssText = [
    "all: initial",
    "font: 11px 'Segoe UI', system-ui, sans-serif",
    "color: #8fa3bf",
    "min-width: 54px",
    "text-align: right",
  ].join(";");

  const fermer = document.createElement("button");
  fermer.type = "button";
  fermer.textContent = "✕";
  fermer.title = "Fermer (Échap)";
  fermer.style.cssText = [
    "all: initial",
    "font: 13px 'Segoe UI', system-ui, sans-serif",
    "color: #a9b7cc",
    "background: transparent",
    "border: 0",
    "cursor: pointer",
    "padding: 2px 4px",
    "border-radius: 6px",
  ].join(";");

  let trouves = 0;
  let dernier = "";

  function afficher() {
    compte.textContent = trouves > 0 ? `${trouves} trouvé(s)` : dernier ? "aucun" : "";
  }

  function chercher(texte, arriere) {
    if (!texte) {
      trouves = 0;
      dernier = "";
      afficher();
      return;
    }
    if (texte !== dernier) {
      dernier = texte;
      trouves = 0;
    }
    // `window.find(texte, sensibleCasse, arriere, wrapAround, ...)` : API non standard mais
    // presente dans Chromium, et c'est exactement le comportement attendu (selection + defile).
    const trouve =
      typeof window.find === "function" &&
      window.find(texte, false, !!arriere, true, false, false, false);
    if (trouve) {
      trouves += 1;
    } else if (trouves === 0) {
      compte.textContent = "aucun";
      return;
    }
    afficher();
  }

  champ.addEventListener("input", () => chercher(champ.value, false));
  champ.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      chercher(champ.value, e.shiftKey);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      quitter();
    }
  });

  function quitter() {
    boite.remove();
    // Sans cela le clavier reste dans un champ detruit : les raccourcis de Trame ne
    // repondraient plus.
    if (document.body) document.body.focus();
    if (typeof window.focus === "function") window.focus();
  }

  fermer.addEventListener("click", quitter);

  boite.appendChild(champ);
  boite.appendChild(compte);
  boite.appendChild(fermer);
  (document.body || document.documentElement).appendChild(boite);
  champ.focus();
  champ.select();
})();
