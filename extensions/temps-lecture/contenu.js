/* Temps de lecture — exemple d'extension Trame.
   Montre : lecture du DOM, injection d'un élément, et stockage persistant (chrome.storage.local).

   INTERACTION : c'est la petite croix qui masque la pastille, pas la pastille entière.
   Masquer sur un simple clic était un piège : la pastille disparaissait pour de bon, sans
   moyen de la faire revenir. On peut aussi tout remettre à zéro depuis les réglages
   (Extensions → « Réinitialiser »), ce qui efface le stockage de l'extension. */

const MEMOIRE = "masque";

function textePrincipal() {
  const zones = [
    document.querySelector("article"),
    document.querySelector("main"),
    document.querySelector('[role="main"]'),
    document.body,
  ];
  for (const zone of zones) {
    if (!zone) continue;
    const texte = (zone.innerText || "").trim();
    if (texte.length > 400) return texte;
  }
  return "";
}

async function principal() {
  const stockage = await chrome.storage.local.get([MEMOIRE]);
  if (stockage && stockage[MEMOIRE]) return;

  const texte = textePrincipal();
  if (!texte) return;

  const mots = texte.split(/\s+/).filter(Boolean).length;
  if (mots < 120) return;                      // trop court pour valoir une estimation
  const minutes = Math.max(1, Math.round(mots / 220));

  if (document.getElementById("trame-temps-lecture")) return;

  const pastille = document.createElement("div");
  pastille.id = "trame-temps-lecture";
  Object.assign(pastille.style, {
    position: "fixed",
    right: "16px",
    bottom: "16px",
    zIndex: "2147483647",
    display: "flex",
    alignItems: "center",
    gap: "8px",
    padding: "7px 8px 7px 12px",
    borderRadius: "999px",
    background: "rgba(16, 21, 33, 0.94)",
    color: "#9df0e0",
    border: "1px solid rgba(94, 234, 212, 0.35)",
    boxShadow: "0 10px 26px rgba(0,0,0,0.45)",
    font: "600 12px/1.2 'Segoe UI', system-ui, sans-serif",
    userSelect: "none",
    backdropFilter: "blur(6px)",
  });

  const mesure = document.createElement("span");
  mesure.textContent = `≈ ${minutes} min · ${mots.toLocaleString("fr-FR")} mots`;
  mesure.title = "Estimation à 220 mots/minute.";
  pastille.appendChild(mesure);

  // La croix, et seulement elle, masque la pastille definitivement.
  const croix = document.createElement("button");
  croix.textContent = "×";
  croix.title = "Masquer cette pastille définitivement (Réglages → Extensions → Réinitialiser pour la retrouver)";
  Object.assign(croix.style, {
    all: "unset",
    cursor: "pointer",
    width: "16px",
    height: "16px",
    display: "grid",
    placeItems: "center",
    borderRadius: "50%",
    color: "#6f8b86",
    fontSize: "13px",
    lineHeight: "1",
  });
  croix.addEventListener("mouseenter", () => { croix.style.color = "#ffb4b4"; croix.style.background = "rgba(248,113,113,0.18)"; });
  croix.addEventListener("mouseleave", () => { croix.style.color = "#6f8b86"; croix.style.background = "transparent"; });
  croix.addEventListener("click", async (evenement) => {
    evenement.stopPropagation();
    await chrome.storage.local.set({ [MEMOIRE]: true });
    pastille.remove();
  });
  pastille.appendChild(croix);

  document.body.appendChild(pastille);
}

principal();
