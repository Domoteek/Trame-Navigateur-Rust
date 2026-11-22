// Nuit douce — garde-fou.
//
// Une feuille injectee ne sait pas si la page est deja sombre. Deux assombrissements superposes
// donnent des aplats noirs laids et illisibles. On mesure donc la luminance du fond du site, et
// si la page est deja sombre, on RETIRE notre propre feuille (reperee par son attribut
// data-trame-ext, pose par chrome.dom.style).

(function () {
  function luminance(couleur) {
    const nombres = String(couleur).match(/[\d.]+/g);
    if (!nombres || nombres.length < 3) return null;
    const [r, g, b] = nombres.map(Number);
    // Formule WCAG, ramenee dans [0, 1].
    return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  }

  const fond = getComputedStyle(document.documentElement).backgroundColor;
  const fondCorps = getComputedStyle(document.body || document.documentElement).backgroundColor;
  const l = luminance(fondCorps === "rgba(0, 0, 0, 0)" ? fond : fondCorps);

  // Deja sombre (ou lisible nulle part) : on ne touche a rien.
  if (l !== null && l < 0.35) {
    document
      .querySelectorAll('style[data-trame-ext="' + chrome.runtime.id + '"]')
      .forEach((feuille) => feuille.remove());
    console.log("nuit-douce : page deja sombre, feuille retiree (luminance", l.toFixed(2) + ")");
  }
})();
