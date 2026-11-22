# Extensions livrées

Trois exemples, choisis pour couvrir trois mécanismes différents :

| Dossier | Ce qu'il montre |
|---|---|
| `nuit-douce/` | **CSS seul** : aucun script, juste une feuille injectée. Le cas le plus simple. |
| `temps-lecture/` | **JS + stockage** : lit le DOM, estime une durée, retient le choix de l'utilisateur. |
| `compteur-visites/` | **stockage persistant par site** : un compteur qui survit aux redémarrages. |

Elles sont copiées dans `%APPDATA%\Trame\extensions\` au premier lancement, puis chargées depuis
là. Modifier un exemple dans ce dépôt ne change donc rien tant que la copie du profil existe :
éditez la copie du profil, ou supprimez-la pour qu'elle soit recopiée.

## Écrire la sienne

```
mon-extension/
  manifest.json
  contenu.js
  style.css
```

Déposez le dossier dans `%APPDATA%\Trame\extensions\`, ouvrez la Couture (`Ctrl+K`), tapez
`>extensions`, validez : elles sont rechargées. La spécification complète est dans
[`../docs/FORMAT-EXTENSION.md`](../docs/FORMAT-EXTENSION.md).

## Avertissement

Il n'y a pas encore de modèle de permissions : une extension active peut lire toutes les pages
qu'elle cible et écrire dans son propre stockage. N'installez que du code que vous avez lu.
