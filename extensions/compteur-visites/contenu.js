/* Compteur de visites — exemple d'extension Trame avec stockage persistant.
   Le stockage est propre à l'extension (namespace par identifiant de dossier) et survit
   aux redémarrages : il vit dans ext-storage.json du profil. */

const CLE = "visites";

function domaine() {
  try {
    return location.hostname.replace(/^www\./, "");
  } catch (e) {
    return "";
  }
}

async function principal() {
  const site = domaine();
  if (!site) return;

  const stockage = await chrome.storage.local.get([CLE]);
  const visites = (stockage && stockage[CLE]) || {};
  const compte = (visites[site] || 0) + 1;
  visites[site] = compte;
  await chrome.storage.local.set({ [CLE]: visites });

  if (compte < 2) return; // la première visite ne mérite pas un mot

  const bulle = document.createElement("div");
  bulle.id = "trame-compteur-visites";
  bulle.textContent = `${compte}ᵉ visite ici`;
  bulle.title = `Compteur tenu par l'extension « Compteur de visites » pour ${site}.\nCliquer pour fermer`;
  document.body.appendChild(bulle);

  // 15 s à l'écran, ou jusqu'au clic (choix de Clement, 2026-11-23 : à 5 s, elle partait
  // avant d'être lue).
  let partie = false;
  const partir = () => {
    if (partie) return;
    partie = true;
    bulle.classList.add("parti");
    setTimeout(() => bulle.remove(), 800);
  };
  bulle.addEventListener("click", partir);
  setTimeout(partir, 15000);
}

principal();
