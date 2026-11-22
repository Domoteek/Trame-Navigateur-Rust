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
  bulle.title = `Compteur tenu par l'extension « Compteur de visites » pour ${site}.`;
  document.body.appendChild(bulle);

  setTimeout(() => bulle.classList.add("parti"), 5200);
  setTimeout(() => bulle.remove(), 6000);
}

principal();
