// Extension Chrome (MV3) de controle. Si cette banniere apparait, c'est bien le moteur
// d'extensions de WebView2 qui a execute ce fichier — pas l'injecteur maison de Trame,
// qui ne lit que des manifestes au format Trame.
(function () {
  if (document.getElementById("preuve-chrome")) return;
  const banniere = document.createElement("div");
  banniere.id = "preuve-chrome";
  banniere.textContent =
    "Extension Chrome chargée par WebView2 — " +
    (typeof chrome !== "undefined" && chrome.runtime && chrome.runtime.id
      ? "id " + chrome.runtime.id.slice(0, 8)
      : "runtime absent");
  (document.body || document.documentElement).appendChild(banniere);
  console.log("preuve-chrome :", chrome && chrome.runtime ? chrome.runtime.id : "sans runtime");
})();
