/* Teintes de la coquille — UNE table, lue par la coquille (chrome.js) et par les réglages.
 *
 * Pourquoi ici : la table vivait dans chrome.js et les réglages dessinaient leurs pastilles
 * avec d'autres couleurs, écrites à la main. « Ciel » y était même identique à « Défaut ».
 * Constat de Clément le 2026-11-23 : « les modifications sont minimes ». Une teinte change
 * désormais les fonds ET les surfaces ET l'accent, et chaque bleu de chrome.css passe par
 * `--accent-rgb` au lieu d'être écrit en dur.
 *
 * `defaut` reprend exactement les valeurs de `:root` (chrome.css) : choisir « Défaut » retire
 * les variables posées, et l'aspect d'origine revient tel quel.
 */
(function () {
  const TEINTES = {
    defaut: {
      nom: "Défaut",
      fond: "#080b11", fond2: "#0b0f17", surface: "#101521", surface2: "#151c2a", surface3: "#1b2434",
      accent: "#60a5fa", accent2: "#a78bfa",
    },
    menthe: {
      nom: "Menthe",
      fond: "#07100e", fond2: "#0a1513", surface: "#0d1a18", surface2: "#122321", surface3: "#182d2a",
      accent: "#34d399", accent2: "#6ee7b7",
    },
    ciel: {
      nom: "Ciel",
      fond: "#060d14", fond2: "#09121b", surface: "#0c1824", surface2: "#112131", surface3: "#162a3d",
      accent: "#38bdf8", accent2: "#7dd3fc",
    },
    violet: {
      nom: "Violet",
      fond: "#0c0914", fond2: "#110c1b", surface: "#161027", surface2: "#1e1633", surface3: "#261d40",
      accent: "#a78bfa", accent2: "#f0abfc",
    },
    ambre: {
      nom: "Ambre",
      fond: "#0f0b05", fond2: "#141008", surface: "#1a1408", surface2: "#241c0d", surface3: "#2e2412",
      accent: "#fbbf24", accent2: "#fde68a",
    },
    ardoise: {
      nom: "Ardoise",
      fond: "#0b0c0f", fond2: "#0f1115", surface: "#14181f", surface2: "#1b2029", surface3: "#232934",
      accent: "#94a3b8", accent2: "#cbd5e1",
    },
  };
  // « Dynamique » : l'accent du site affiché (`<meta name="theme-color">`), sur les surfaces
  // d'origine — une couleur de site en fond de navigateur casserait le contraste.
  const DYNAMIQUE = { nom: "Dynamique" };

  /** « r, g, b » d'une couleur CSS quelconque (#abc, #aabbcc, rgb(), hsl()…), ou null. */
  function rgb(couleur) {
    if (!couleur) return null;
    const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})([0-9a-f]{2})?$/i.exec(couleur.trim());
    if (hex) {
      let h = hex[1];
      if (h.length === 3) h = h.split("").map((c) => c + c).join("");
      return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)).join(", ");
    }
    // Le navigateur sait lire le reste : on lui fait calculer la couleur.
    const sonde = document.createElement("span");
    sonde.style.color = couleur;
    if (!sonde.style.color) return null;
    sonde.style.display = "none";
    document.documentElement.appendChild(sonde);
    const lu = getComputedStyle(sonde).color;
    sonde.remove();
    const m = /rgba?\((\d+),\s*(\d+),\s*(\d+)/.exec(lu);
    return m ? `${m[1]}, ${m[2]}, ${m[3]}` : null;
  }

  const VARIABLES = [
    ["--fond", "fond"], ["--fond-2", "fond2"], ["--surface", "surface"],
    ["--surface-2", "surface2"], ["--surface-3", "surface3"],
    ["--accent", "accent"], ["--accent-2", "accent2"],
  ];

  /** Pose la teinte `id` sur la page (variables CSS de :root). `site` sert au mode dynamique. */
  function appliquer(id, site) {
    const racine = document.documentElement.style;
    let t = null;
    if (id === "dynamique") {
      const r = rgb(site);
      t = r ? { accent: site, accent2: site } : null;
    } else if (id && id !== "defaut" && TEINTES[id]) {
      t = TEINTES[id];
    }
    for (const [variable, cle] of VARIABLES) {
      if (t && t[cle]) racine.setProperty(variable, t[cle]);
      else racine.removeProperty(variable);
    }
    const a = t && rgb(t.accent);
    const b = t && rgb(t.accent2);
    if (a) racine.setProperty("--accent-rgb", a); else racine.removeProperty("--accent-rgb");
    if (b) racine.setProperty("--accent-2-rgb", b); else racine.removeProperty("--accent-2-rgb");
    document.documentElement.dataset.teinte = id || "defaut";
  }

  window.TrameTeintes = { TEINTES, DYNAMIQUE, appliquer, rgb };
})();
