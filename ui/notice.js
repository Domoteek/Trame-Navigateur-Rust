/*
 * La notice de Trame : UNE seule source pour toute l'aide.
 *
 * Lue par :
 *  - la page Notice (`aide.html`, F1) et sa version imprimable (`aide.html?impression`, le PDF) ;
 *  - le calque des raccourcis (Ctrl+/, dans `chrome.js`) ;
 *  - l'hôte, pour l'aide de la Couture et les astuces (`src/aide.rs`, `include_str!`).
 *
 * Ce qui suit le signe « = » est du JSON STRICT (guillemets doubles, pas de virgule finale) :
 * Rust le relit tel quel. Un test (`cargo test aide`) le vérifie, et `outils/tester-ui.mjs`
 * vérifie que chaque raccourci réel de `raccourcis.js` figure dans « raccourcis ».
 *
 * Écrire pour Clément : des phrases simples, ce que ça fait, comment le lancer.
 * Une `commande` d'action est un identifiant de `App::commande` (src/app.rs).
 */
window.NOTICE = {
  "chapitres": [
    {
      "id": "premiers-pas",
      "titre": "Premiers pas",
      "chapeau": "Trame range tes onglets tout seul et les endort quand tu ne t'en sers plus. Trois endroits à connaître : le rail à gauche, la barre en haut, le menu ⋯ de la page.",
      "etapes": [
        { "titre": "Le rail, à gauche", "texte": "Tes onglets, rangés en fils (un fil = un sujet). Clic pour afficher, clic droit pour tout ce qu'on peut faire d'un onglet. Ctrl+B le replie en une colonne d'icônes." },
        { "titre": "La barre, en haut", "texte": "Tape une adresse, une recherche, ou le nom d'un onglet déjà ouvert : c'est la Couture (Ctrl+K)." },
        { "titre": "Le menu ⋯", "texte": "Tout ce qui concerne la page affichée : favori, impression, côte à côte, rendez-vous, guet." }
      ],
      "conseils": [
        "F1 ouvre cette notice, sur la fiche qui va avec ce que tu regardes.",
        "Ctrl+/ affiche tous les raccourcis d'un coup d'œil.",
        "Dans la barre, tape une question (« écran en deux », « sortir la vidéo ») : la réponse s'affiche en tête."
      ],
      "actions": [
        { "libelle": "Voir les raccourcis", "commande": "raccourcis" },
        { "libelle": "Ouvrir la Couture", "commande": "couture" }
      ],
      "mots": ["aide", "notice", "commencer", "débuter", "comment", "mode d'emploi", "utiliser", "manuel", "tutoriel"],
      "voir": ["onglets-fils", "couture", "raccourcis"]
    },
    {
      "id": "onglets-fils",
      "titre": "Onglets et fils",
      "chapeau": "Chaque onglet rejoint un fil : celui de la page d'où il vient, ou celui de son site. Un fil garde son nom et ses onglets d'un démarrage à l'autre.",
      "etapes": [
        { "titre": "Renommer un fil", "texte": "Double-clic sur le nom du fil, dans le rail." },
        { "titre": "Ranger les fils", "texte": "Fais glisser la tête d'un fil (son nom) au-dessus ou au-dessous d'un autre. Un trait montre où il tombera." },
        { "titre": "Clic droit sur un onglet", "texte": "Fermer, fermer les autres du fil, dupliquer, couper le son, ancrer, déplacer vers un autre fil, copier l'adresse." }
      ],
      "conseils": [
        "Un onglet dont tu ne te sers plus s'endort : il ne prend plus de mémoire, et un clic le réveille là où tu en étais.",
        "Ancrer un onglet (clic droit) l'empêche de s'endormir.",
        "Ctrl+Maj+T rouvre le dernier onglet fermé. Ctrl+1 à Ctrl+9 vont à l'onglet n° 1 à 9."
      ],
      "actions": [
        { "libelle": "Rouvrir le dernier fermé", "commande": "rouvrir" },
        { "libelle": "Nouvel onglet", "commande": "nouvel-onglet" }
      ],
      "mots": ["onglet", "onglets", "fil", "fils", "groupe", "renommer", "ranger", "veille", "endormi", "dormir", "ancrer", "rouvrir", "fermé", "dupliquer", "réveiller"],
      "voir": ["rail-acces", "trame", "rendez-vous"]
    },
    {
      "id": "rail-acces",
      "titre": "Le rail, les accès rapides et les favoris",
      "chapeau": "Deux sortes de raccourcis vers des sites, à ne pas confondre : l'accès rapide ouvre un SITE, le favori retrouve une PAGE précise.",
      "etapes": [
        { "titre": "Accès rapide = un site", "texte": "Les icônes sous le logo du rail (10 places). Un clic ouvre toujours le site dans un nouvel onglet. Clic droit sur un onglet → « Ajouter le site aux accès rapides »." },
        { "titre": "Favori = une page", "texte": "Ctrl+D ou l'étoile de la barre. Le favori ramène à l'onglet où la page est ouverte, sinon il l'ouvre." },
        { "titre": "Replier le rail", "texte": "Ctrl+B : le rail devient une colonne d'icônes. Son aspect se choisit dans Réglages → Apparence." }
      ],
      "conseils": [
        "Les favoris sortent aussi dans la barre dès que tu tapes leur nom, et en tuiles sur la page Nouvel onglet."
      ],
      "actions": [
        { "libelle": "Replier ou déplier le rail", "commande": "rail" }
      ],
      "mots": ["rail", "accès rapide", "acces", "favori", "favoris", "étoile", "marque-page", "signet", "replier", "barre latérale"],
      "voir": ["onglets-fils", "sauvegarde"]
    },
    {
      "id": "couture",
      "titre": "La Couture : la barre qui cherche partout",
      "chapeau": "Une seule saisie pour tout : adresse, recherche sur le web, onglets ouverts, fils, historique, favoris, commandes et aide.",
      "etapes": [
        { "titre": "Entrée", "texte": "Va à l'adresse ou lance la recherche dans l'onglet affiché." },
        { "titre": "Alt + Entrée", "texte": "Ouvre dans un nouvel onglet du même fil, juste sous la page où tu es." },
        { "titre": "Maj + Entrée", "texte": "Ouvre à côté de la page affichée (côte à côte)." }
      ],
      "conseils": [
        "Commence par « > » pour les commandes de Trame (veille, capsule, trame…).",
        "Commence par « ? » pour retrouver une page lue : « ? électrolyte solide » montre la page et le passage.",
        "Alt + clic sur un lien ouvre un coup d'œil : la page s'affiche par-dessus, sans quitter la tienne.",
        "Le moteur de recherche se change dans les Réglages (Ctrl+,)."
      ],
      "actions": [
        { "libelle": "Ouvrir la Couture", "commande": "couture" }
      ],
      "mots": ["couture", "barre", "adresse", "recherche", "chercher", "historique", "commande", "moteur", "coup d'oeil", "coup d'œil", "aperçu", "mémoire de lecture", "lu"],
      "voir": ["cote-a-cote", "onglets-fils"]
    },
    {
      "id": "cote-a-cote",
      "titre": "Côte à côte : deux pages dans la fenêtre",
      "resume": "Maj + clic sur un onglet du rail, ou Maj + Entrée dans la barre. La séparation se glisse.",
      "chapeau": "Deux pages dans la même fenêtre, pour comparer deux produits, lire en prenant des notes ou suivre une vidéo pendant que tu cherches autre chose.",
      "etapes": [
        { "titre": "Maj + clic sur un onglet", "texte": "L'onglet s'ouvre à droite de la page que tu regardes. Le plus rapide." },
        { "titre": "Maj + Entrée dans la barre", "texte": "Ce que tu viens de taper s'ouvre à droite, ta page reste à gauche." },
        { "titre": "Menu ⋯ → Côte à côte", "texte": "Met à côté le dernier onglet que tu as regardé." }
      ],
      "conseils": [
        "Glisse la séparation pour changer les largeurs (20 à 80 %).",
        "Un clic dans une page la rend active : la barre d'adresse et les raccourcis visent celle-là.",
        "Revenir à une seule page : clique dans la page à garder, puis menu ⋯ → Côte à côte → Quitter. C'est toujours la page active (celle où tu as cliqué en dernier) qui reste en pleine page.",
        "Cliquer sur un autre onglet du rail l'affiche seul, mais ne défait pas le côte à côte : recliquer sur l'une des deux pages les ramène ensemble. Seul « Quitter » les sépare pour de bon.",
        "Menu ⋯ → Côte à côte : Inverser (échange gauche et droite), Lier le défilement (les deux pages défilent ensemble), Quitter.",
        "Il faut au moins deux onglets ouverts. Deux pages au plus."
      ],
      "actions": [
        { "libelle": "Essayer maintenant", "commande": "cote" }
      ],
      "mots": ["côte à côte", "cote a cote", "split", "split view", "écran partagé", "partager l'écran", "diviser", "deux pages", "deux écrans", "écran en deux", "comparer", "séparer", "moitié", "à côté", "quitter", "défaire", "pleine page", "une seule page", "annuler"],
      "voir": ["video", "couture", "raccourcis"]
    },
    {
      "id": "video",
      "titre": "Vidéo flottante : sortir une vidéo de la page",
      "resume": "Souris sur la vidéo : « Vidéo seule » (Alt+P, l'image seule) ou « Fenêtre à part » (Alt+Maj+P, toute la page et ses commandes). Les deux laissent changer d'onglet.",
      "chapeau": "Passe la souris sur une vidéo : une pastille apparaît en haut de l'image, avec deux boutons. Les deux te laissent changer d'onglet pendant que la vidéo continue. La différence : ce qui sort.",
      "etapes": [
        { "titre": "« Vidéo seule » (Alt+P)", "texte": "Seule l'image sort, dans une petite fenêtre au-dessus de tout, que tu poses où tu veux (même sur un autre écran). Commandes : lecture et pause, rien d'autre. La page reste dans son onglet." },
        { "titre": "« Fenêtre à part » (Alt+Maj+P)", "texte": "Toute la page de la vidéo part dans sa propre fenêtre, toujours au premier plan. Tu gardes toutes les commandes du lecteur : avancer, volume, qualité, vidéo suivante. L'onglet quitte le rail le temps du voyage." },
        { "titre": "Ramener", "texte": "« Vidéo seule » : bouton de retour de la petite fenêtre, ou Alt+P. « Fenêtre à part » : bouton « Ramener » en haut de la fenêtre (au survol), Alt+Maj+P, ou Alt+F4." }
      ],
      "conseils": [
        "Pour regarder YouTube en naviguant ailleurs avec toutes les commandes : « Fenêtre à part ».",
        "Pour juste garder un coin d'image pendant que tu travailles : « Vidéo seule ».",
        "Une vidéo sortie n'est jamais endormie par Trame, même si tu changes longtemps d'onglet.",
        "Plein écran d'une vidéo : Réglages → Apparence → « Vidéo en plein écran » choisit s'il prend la fenêtre (par défaut), tout l'écran, ou seulement la page. Échap en sort."
      ],
      "actions": [],
      "mots": ["vidéo", "video", "flottante", "sortir", "fenêtre", "mini", "incrustation", "picture in picture", "pip", "regarder", "lecteur", "plein écran", "par-dessus", "au premier plan", "détacher"],
      "voir": ["cote-a-cote", "zappeur"]
    },
    {
      "id": "rendez-vous",
      "titre": "Rendez-vous et fermeture programmée",
      "chapeau": "Mettre une page de côté jusqu'à plus tard, ou la laisser partir toute seule.",
      "etapes": [
        { "titre": "Rendez-vous", "texte": "Menu ⋯ → « Dans 1 h », « ce soir », « demain matin », « ce week-end », « lundi ». L'onglet quitte le rail et s'endort, puis revient seul à l'heure dite, marqué « revenu »." },
        { "titre": "Fermeture programmée", "texte": "Menu ⋯ → « Se ferme dans 1 h / 3 h / ce soir ». L'onglet part tout seul ; Ctrl+Maj+T le rouvre." },
        { "titre": "Ceux qui attendent", "texte": "Une ligne en bas du rail liste les rendez-vous à venir." }
      ],
      "conseils": [
        "Un onglet en rendez-vous ne prend aucune mémoire pendant l'attente."
      ],
      "actions": [],
      "mots": ["rendez-vous", "rappel", "plus tard", "lire plus tard", "demain", "ce soir", "minuterie", "fermeture", "fermer automatiquement", "programmer"],
      "voir": ["onglets-fils", "guetteur"]
    },
    {
      "id": "guetteur",
      "titre": "Le guetteur : surveiller un prix, un stock",
      "chapeau": "Tu désignes un morceau de page (un prix, « en stock », un résultat) et Trame te prévient quand il change.",
      "etapes": [
        { "titre": "Désigner", "texte": "Menu ⋯ → « Guetter un élément », puis clique sur le prix ou le texte à surveiller." },
        { "titre": "Trame relit", "texte": "Toutes les 30 minutes, dans une page cachée et muette, jamais quand le PC manque de mémoire." },
        { "titre": "Il te prévient", "texte": "Message « 39,90 € → 34,90 € », clignotement dans la barre des tâches, pastille sur ⋯." }
      ],
      "conseils": [],
      "actions": [],
      "mots": ["guetter", "guetteur", "surveiller", "prix", "stock", "alerte", "baisse", "changement", "promo"],
      "voir": ["rendez-vous"]
    },
    {
      "id": "zappeur",
      "titre": "Zappeur et saute-pub",
      "chapeau": "Faire disparaître ce qui gêne dans une page, et passer les pubs de YouTube.",
      "etapes": [
        { "titre": "Le zappeur", "texte": "L'icône œil barré ouvre un panneau à droite : clique sur un élément de la page et il disparaît, sur ce site seulement." },
        { "titre": "Les règles", "texte": "Chaque règle se relit, se coupe ou se supprime dans le même panneau. Trame en livre pour YouTube (Shorts, annonces)." },
        { "titre": "Le saute-pub", "texte": "Extension livrée : retire les pubs de YouTube avant qu'elles démarrent et saute les passages sponsorisés." }
      ],
      "conseils": [],
      "actions": [
        { "libelle": "Ouvrir le zappeur", "commande": "zap" }
      ],
      "mots": ["zap", "zappeur", "masquer", "cacher", "pub", "publicité", "annonce", "shorts", "bloquer", "saute-pub", "sponsor", "élément"],
      "voir": ["extensions", "video"]
    },
    {
      "id": "prive",
      "titre": "Navigation privée",
      "chapeau": "Un fil à part, qui ne garde rien : ni connexions, ni cookies, ni historique.",
      "etapes": [
        { "titre": "Ouvrir", "texte": "Ctrl+Maj+N, le bouton « Privé » en bas du rail, ou le Nouvel onglet." },
        { "titre": "Ce qui est oublié", "texte": "Les onglets privés ne sont jamais sauvegardés, rien n'entre dans l'historique ni dans la mémoire de lecture." },
        { "titre": "Effacé", "texte": "Tout ce que la navigation privée a écrit est effacé au démarrage suivant." }
      ],
      "conseils": [
        "Pour garder des comptes séparés de façon durable, un fil cloisonné (bouclier dans l'en-tête du fil) a son propre profil et ses propres extensions."
      ],
      "actions": [
        { "libelle": "Ouvrir un onglet privé", "commande": "prive" }
      ],
      "mots": ["privé", "prive", "incognito", "anonyme", "secret", "cookies", "cloisonné", "profil", "compte"],
      "voir": ["onglets-fils", "extensions"]
    },
    {
      "id": "trame",
      "titre": "La trame : la vue d'ensemble",
      "chapeau": "Tous tes fils d'un coup d'œil, tissés dans le temps.",
      "etapes": [
        { "titre": "Ouvrir", "texte": "Ctrl+Espace. La même touche la referme." },
        { "titre": "Lire", "texte": "Chaque fil est une bande, le temps va de gauche à droite, un gros nœud = un onglet que tu utilises beaucoup." },
        { "titre": "Ranger", "texte": "Glisse un nœud sur une autre bande pour changer l'onglet de fil." }
      ],
      "conseils": [],
      "actions": [
        { "libelle": "Voir la trame", "commande": "trame" }
      ],
      "mots": ["trame", "vue", "ensemble", "tissée", "carte", "chronologie"],
      "voir": ["onglets-fils"]
    },
    {
      "id": "extensions",
      "titre": "Extensions",
      "chapeau": "Trame accepte les extensions de Chrome (Bitwarden, uBlock…) et dit ce que chacune coûte en mémoire.",
      "etapes": [
        { "titre": "Installer", "texte": "Ouvre la fiche de l'extension sur le magasin de Chrome dans Trame et clique « Ajouter à Chrome ». Ou Réglages → Extensions Chrome." },
        { "titre": "Le puzzle", "texte": "Le bouton puzzle de la barre liste tes extensions : clic sur une ligne pour l'ouvrir, interrupteur pour l'éteindre." },
        { "titre": "Épingler", "texte": "L'épingle met l'icône de l'extension dans la barre, à gauche du puzzle." }
      ],
      "conseils": [
        "Pas besoin de redémarrer : une extension installée marche tout de suite."
      ],
      "actions": [
        { "libelle": "Ouvrir les réglages", "commande": "reglages" }
      ],
      "mots": ["extension", "extensions", "module", "plugin", "puzzle", "installer", "chrome web store", "magasin", "épingler"],
      "voir": ["zappeur", "prive"]
    },
    {
      "id": "sauvegarde",
      "titre": "Sauvegarde, téléchargements et mises à jour",
      "chapeau": "Garder ses fils et ses réglages en lieu sûr, retrouver ses fichiers, rester à jour.",
      "etapes": [
        { "titre": "Sauvegarde", "texte": "Réglages → Sauvegarde du profil : un fichier qui emporte réglages, fils, favoris et accès. La sauvegarde automatique en écrit un toutes les N heures dans le dossier choisi." },
        { "titre": "Téléchargements", "texte": "Ctrl+J. Les fichiers vont dans le dossier de Trame, jamais écrasés ; le dossier se change sur la même page." },
        { "titre": "Mises à jour", "texte": "Trame propose seul une nouvelle version (fenêtre en bas à droite) : « Installer » suffit." }
      ],
      "conseils": [
        "Les favoris s'exportent et s'importent au format de Chrome, Edge et Firefox."
      ],
      "actions": [
        { "libelle": "Téléchargements", "commande": "telechargements" },
        { "libelle": "Réglages", "commande": "reglages" }
      ],
      "mots": ["sauvegarde", "sauvegarder", "restaurer", "exporter", "importer", "téléchargement", "téléchargements", "fichier", "mise à jour", "version", "réglages", "paramètres", "options"],
      "voir": ["rail-acces", "extensions"]
    },
    {
      "id": "depannage",
      "titre": "Quand quelque chose ne va pas",
      "resume": "Réglages → Général → Dépannage : « Réparer Trame », puis redémarrer. Si ça ne suffit pas : « Rapport pour Claude ».",
      "chapeau": "Une page qui ne bouge plus, un lecteur vidéo figé, un menu bizarre : Trame se répare sans rien perdre, et sait écrire un rapport pour trouver la cause.",
      "etapes": [
        { "titre": "Réparer Trame", "texte": "Réglages → Général → Dépannage → Réparer. Vide les caches du moteur, remet à neuf les extensions livrées et les réglages de mémoire et de veille. Fils, favoris, accès rapides et connexions restent. Redémarre Trame ensuite." },
        { "titre": "Rapport pour Claude", "texte": "Même endroit, « Écrire le rapport » : un fichier dans tes téléchargements (versions, extensions, réglages, fin du journal), sans les pages visitées. Envoie-le à Claude." },
        { "titre": "Après chaque mise à jour", "texte": "Trame vérifie tout seul ses fichiers et ses extensions au premier démarrage, et dit ce qu'il a remis à neuf." }
      ],
      "conseils": [
        "Plus besoin de désinstaller et de tout effacer : « Réparer Trame » fait le même ménage sans perdre tes fils ni tes connexions.",
        "Aussi depuis la barre : tape « > réparer » ou « > rapport »."
      ],
      "actions": [
        { "libelle": "Écrire le rapport", "commande": "rapport" },
        { "libelle": "Ouvrir les réglages", "commande": "reglages" }
      ],
      "mots": ["dépannage", "réparer", "réparation", "bug", "problème", "panne", "marche pas", "fonctionne pas", "bloqué", "figé", "rapport", "réinstaller", "désinstaller", "cache", "lent"],
      "voir": ["sauvegarde", "extensions"]
    },
    {
      "id": "raccourcis",
      "titre": "Tous les raccourcis",
      "resume": "Ctrl+/ les affiche tous, par-dessus la page.",
      "chapeau": "Ctrl+/ les affiche par-dessus la page, à tout moment.",
      "etapes": [],
      "conseils": [],
      "actions": [
        { "libelle": "Afficher le calque des raccourcis", "commande": "raccourcis" }
      ],
      "mots": ["raccourci", "raccourcis", "clavier", "touche", "touches", "ctrl"],
      "voir": ["premiers-pas", "couture"]
    }
  ],

  "raccourcis": [
    { "groupe": "Onglets", "touche": "Ctrl+T", "effet": "Nouvel onglet", "action": "nouvelOnglet" },
    { "groupe": "Onglets", "touche": "Ctrl+W", "effet": "Fermer l'onglet", "action": "fermerOnglet" },
    { "groupe": "Onglets", "touche": "Ctrl+Maj+T", "effet": "Rouvrir le dernier fermé", "action": "reouverture" },
    { "groupe": "Onglets", "touche": "Ctrl+Tab", "effet": "Onglet suivant (Maj : précédent)", "action": "ongletSuivant" },
    { "groupe": "Onglets", "touche": "Ctrl+1…9", "effet": "Onglet n° 1 à 9", "action": "onglet:" },
    { "groupe": "Onglets", "touche": "Ctrl+Maj+N", "effet": "Navigation privée", "action": "prive" },
    { "groupe": "Onglets", "touche": "Maj+clic", "effet": "Afficher l'onglet à côté" },
    { "groupe": "Onglets", "touche": "Clic droit", "effet": "Menu de l'onglet" },

    { "groupe": "Pages", "touche": "Ctrl+R / F5", "effet": "Recharger", "action": "recharger" },
    { "groupe": "Pages", "touche": "Échap", "effet": "Arrêter le chargement", "action": "arret" },
    { "groupe": "Pages", "touche": "Ctrl+F", "effet": "Chercher dans la page", "action": "chercherPage" },
    { "groupe": "Pages", "touche": "Ctrl+D", "effet": "Ajouter ou retirer des favoris", "action": "favori" },
    { "groupe": "Pages", "touche": "Ctrl+P", "effet": "Imprimer ou enregistrer en PDF", "action": "imprimer" },
    { "groupe": "Pages", "touche": "Ctrl+ + / -", "effet": "Zoom avant / arrière", "action": "zoomPlus" },
    { "groupe": "Pages", "touche": "Ctrl+0", "effet": "Zoom à 100 %", "action": "zoomZero" },
    { "groupe": "Pages", "touche": "Alt+clic (lien)", "effet": "Coup d'œil sur le lien" },
    { "groupe": "Pages", "touche": "Alt+P", "effet": "Vidéo seule, par-dessus tout" },
    { "groupe": "Pages", "touche": "Alt+Maj+P", "effet": "Vidéo dans une fenêtre à part" },

    { "groupe": "Barre", "touche": "Ctrl+K / Ctrl+L", "effet": "La Couture", "action": "couture" },
    { "groupe": "Barre", "touche": "Entrée", "effet": "Aller ou chercher" },
    { "groupe": "Barre", "touche": "Alt+Entrée", "effet": "Ouvrir dans un nouvel onglet du fil" },
    { "groupe": "Barre", "touche": "Maj+Entrée", "effet": "Ouvrir à côté de la page" },

    { "groupe": "Trame", "touche": "Ctrl+Espace", "effet": "La trame (vue d'ensemble)", "action": "trame" },
    { "groupe": "Trame", "touche": "Ctrl+B", "effet": "Replier le rail", "action": "rail" },
    { "groupe": "Trame", "touche": "Ctrl+,", "effet": "Réglages", "action": "reglages" },
    { "groupe": "Trame", "touche": "Ctrl+J", "effet": "Téléchargements", "action": "telechargements" },
    { "groupe": "Trame", "touche": "F1", "effet": "Notice", "action": "aide" },
    { "groupe": "Trame", "touche": "Ctrl+/", "effet": "Ce calque des raccourcis", "action": "raccourcisCalque" },
    { "groupe": "Trame", "touche": "F12", "effet": "Inspecter la page (version de développement)", "action": "devtools" }
  ],

  "astuces": [
    {
      "id": "cote",
      "texte": "Tu passes souvent entre ces deux onglets. Maj + clic sur l'un d'eux les affiche côte à côte, dans la même fenêtre.",
      "faire": { "libelle": "Les mettre côte à côte", "commande": "cote" },
      "chapitre": "cote-a-cote"
    },
    {
      "id": "video",
      "texte": "La vidéo de l'onglet que tu viens de quitter continue en fond. Pour la garder sous les yeux pendant que tu navigues : passe la souris sur la vidéo, bouton « Fenêtre à part » (Alt+Maj+P).",
      "faire": { "libelle": "Voir comment", "chapitre": "video" },
      "chapitre": "video"
    },
    {
      "id": "rouvrir",
      "texte": "Tu viens de rouvrir une page fermée il y a un instant. Ctrl+Maj+T le fait d'un coup : il rouvre le dernier onglet fermé, à sa place.",
      "faire": { "libelle": "Voir les raccourcis", "commande": "raccourcis" },
      "chapitre": "onglets-fils"
    }
  ],

  "nouveautes": [
    {
      "version": "0.4.65",
      "lignes": [
        "YouTube : un voile gris laissé par une fenêtre de YouTube cachée (« les bloqueurs de publicités… ») ne bloque plus la vidéo.",
        "Le rapport pour Claude compte aussi les pubs sautées et les voiles retirés."
      ]
    },
    {
      "version": "0.4.64",
      "lignes": [
        "Après chaque mise à jour, Trame vérifie le contenu de ses extensions et remet à neuf ce qui a changé.",
        "Réglages → Général → Dépannage : « Réparer Trame » (sans rien perdre) et « Rapport pour Claude ».",
        "Nouvelle fiche de la Notice : « Quand quelque chose ne va pas »."
      ]
    },
    {
      "version": "0.4.63",
      "lignes": [
        "Les menus tiennent dans les petites fenêtres (portables) : le menu ⋯ n'est plus coupé, celui des commandes défile.",
        "Redimensionner la fenêtre referme proprement un menu ouvert, au lieu de laisser un panneau vide.",
        "Ctrl+/ ne se referme plus tout seul quand le curseur est déjà dans la barre.",
        "La Notice dit comment revenir à une seule page après un côte à côte."
      ]
    },
    {
      "version": "0.4.62",
      "lignes": [
        "Une notice dans Trame : F1, le bouton ? de la barre, ou le menu ⋯.",
        "La barre répond aux questions : tape « écran en deux » ou « sortir la vidéo ».",
        "Ctrl+/ affiche tous les raccourcis.",
        "Des astuces, une à la fois, quand un raccourci te ferait gagner du temps (se coupent dans les Réglages).",
        "Vidéo flottante : boutons « Vidéo seule » et « Fenêtre à part », et une vidéo sortie n'est plus jamais endormie."
      ]
    }
  ]
};
