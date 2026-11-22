/**
 * Coup d'oeil : la petite barre de l'apercu flottant (Alt + clic sur un lien).
 *
 * Injectee dans la page affichee par l'apercu. Elle vit dans un Shadow DOM : le style de la page
 * ne peut ni la deformer ni la masquer, et elle ne touche pas au style de la page. Deux actions,
 * comme dans Arc et Zen : garder la page comme onglet, ou refermer l'apercu (Echap aussi).
 *
 * Embarque a la compilation (`include_str!` dans `app.rs`) et teste par `outils/tester-ui.mjs`.
 */
(() => {
  if (window.__trame_apercu) return;
  window.__trame_apercu = true;

  const envoi = (o) => {
    try {
      window.ipc.postMessage(JSON.stringify(o));
    } catch (e) {
      /* sans pont, rien a faire */
    }
  };

  window.addEventListener(
    "keydown",
    (e) => {
      const ctrl = e.ctrlKey || e.metaKey;
      if (e.key === "Escape" || (ctrl && (e.key || "").toLowerCase() === "w")) {
        e.preventDefault();
        e.stopImmediatePropagation();
        envoi({ m: "apercuFermer" });
      } else if (ctrl && e.key === "Enter") {
        e.preventDefault();
        e.stopImmediatePropagation();
        envoi({ m: "apercuOuvrir" });
      }
    },
    true
  );

  const poser = () => {
    if (document.getElementById("__trame-apercu")) return;
    const hote = document.createElement("div");
    hote.id = "__trame-apercu";
    hote.style.cssText = "all: initial; position: fixed; top: 10px; right: 10px; z-index: 2147483647;";
    const ombre = hote.attachShadow({ mode: "closed" });
    ombre.innerHTML = `
      <style>
        .barre { display: flex; gap: 4px; padding: 4px; border-radius: 12px;
          background: rgba(16, 21, 33, 0.92); border: 1px solid rgba(255, 255, 255, 0.14);
          box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35); font: 12px/1 "Segoe UI", system-ui, sans-serif;
          backdrop-filter: blur(8px); }
        button { all: unset; cursor: pointer; display: inline-flex; align-items: center; gap: 6px;
          padding: 7px 10px; border-radius: 8px; color: #e9eefa; }
        button:hover { background: rgba(255, 255, 255, 0.10); }
        button.garder { background: rgba(96, 165, 250, 0.22); color: #dbe9ff; }
        button.garder:hover { background: rgba(96, 165, 250, 0.34); }
        svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 1.9;
          stroke-linecap: round; stroke-linejoin: round; }
      </style>
      <div class="barre" role="toolbar" aria-label="Coup d'oeil">
        <button class="garder" title="Garder comme onglet (Ctrl+Entree)">
          <svg viewBox="0 0 24 24"><path d="M14 4h6v6M20 4l-8.5 8.5M18 14v4.5a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 4 18.5v-11A1.5 1.5 0 0 1 5.5 6H10"/></svg>
          <span>Ouvrir en onglet</span>
        </button>
        <button class="fermer" title="Fermer l'apercu (Echap)">
          <svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></svg>
        </button>
      </div>`;
    ombre.querySelector(".garder").addEventListener("click", () => envoi({ m: "apercuOuvrir" }));
    ombre.querySelector(".fermer").addEventListener("click", () => envoi({ m: "apercuFermer" }));
    (document.body || document.documentElement).appendChild(hote);
  };
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", poser);
  } else {
    poser();
  }
  // Une page qui reconstruit son <body> (applications web) ne doit pas faire disparaitre la barre.
  setInterval(poser, 1500);
})();
