# ACP Excel — Analyse en Composantes Principales d'un fichier CSV

Application Excel (macros VBA) qui importe un fichier CSV, laisse choisir les colonnes à analyser et produit une ACP complète : tableaux, graphiques et interprétation rédigée automatiquement en français.

Aucun complément ni outil externe : tout est calculé en VBA (diagonalisation de Jacobi), dans Excel 2010 ou plus récent sous Windows.

## Installation

### Méthode automatique (recommandée)

1. Téléchargez le dossier `excel-acp` (avec son sous-dossier `src`).
2. Double-cliquez sur **`construire_classeur.bat`**.
   Le script ouvre Excel en arrière-plan, importe les 5 modules VBA, construit l'interface et enregistre **`ACP_Excel.xlsm`** dans le même dossier.
3. Ouvrez `ACP_Excel.xlsm` et cliquez sur **« Activer le contenu »** (bandeau jaune) pour autoriser les macros.

> Le script active temporairement l'option « Accès approuvé au modèle d'objet du projet VBA » (nécessaire pour importer du code) puis la remet dans son état d'origine.
> Si Windows a bloqué les fichiers téléchargés : clic droit sur le `.zip` → Propriétés → cocher « Débloquer » avant d'extraire.

### Méthode manuelle

1. Ouvrez un nouveau classeur Excel, puis **Alt + F11** (éditeur VBA).
2. **Fichier → Importer un fichier…** et importez les 5 fichiers de `src\` : `modInstall.bas`, `modCSV.bas`, `modMatrice.bas`, `modGraphiques.bas`, `modACP.bas`.
3. Fermez l'éditeur, puis **Alt + F8** → exécutez **`InstallerApplication`**.
4. Enregistrez le classeur au format **« Classeur Excel (prenant en charge les macros) (*.xlsm) »**.

## Utilisation

La feuille **Accueil** regroupe tout :

1. **Importer un fichier CSV** — séparateur (`;`, `,`, tabulation, `|`), séparateur décimal (virgule ou point) et encodage (UTF-8, Windows-1252, UTF-16) sont détectés automatiquement ; on peut les forcer avec les listes déroulantes. Les champs entre guillemets (avec séparateurs, guillemets doublés ou retours à la ligne) sont gérés. Les données sont copiées dans la feuille **Données**.
2. **Choisir le rôle de chaque colonne** dans le tableau (liste déroulante) :

   | Rôle | Effet |
   |---|---|
   | Active | Variable numérique utilisée pour construire les axes (au moins 2) |
   | Supplémentaire | Variable numérique projetée sur le cercle des corrélations sans influencer les axes |
   | Étiquette des individus | Nom affiché pour chaque ligne (sinon `L2`, `L3`…, numéro de ligne de la feuille Données) |
   | Groupe (couleur) | Variable qualitative : colore les individus, calcule barycentres et rapport de corrélation (8 modalités au plus, les suivantes sont regroupées en « Autres ») |
   | Ignorée | Non utilisée |

   Par défaut, les colonnes numériques sont « Active » et la première colonne texte à valeurs uniques devient l'étiquette. Les boutons « Activer les numériques » et « Tout ignorer » accélèrent la sélection. Les choix sont conservés si on actualise la liste.
3. **Paramètres** : ACP normée (centrée-réduite, cas usuel) ou non normée (centrée) ; valeurs manquantes exclues ou remplacées par la moyenne ; axes des graphiques (plan 1-2 par défaut) ; nombre d'axes détaillés ; nombre maximal d'individus nommés sur la carte.
4. **Lancer l'ACP**.

On peut aussi coller directement des données dans la feuille **Données** (noms de colonnes en ligne 1) puis cliquer sur « Actualiser la liste des colonnes ».

## Résultats

### Feuille `ACP_Synthèse`
1. Données analysées (individus retenus, exclusions, imputations, variables).
2. Adéquation des données : **test de sphéricité de Bartlett**, **indice KMO** global (avec appréciation) et variables à MSA faible, paires de variables fortement corrélées.
3. **Valeurs propres** : valeur propre, % d'inertie, % cumulé, seuil du bâton brisé + **éboulis** (axes retenus en foncé).
4. **Nombre d'axes à retenir** : critères de Kaiser, du bâton brisé et des 80 % d'inertie.
5. **Interprétation automatique de chaque axe** : variables corrélées côté positif / négatif, opposition ou effet taille, variables et individus les plus contributifs, individus extrêmes, individus atypiques.
6. Plan factoriel affiché : variables bien / mal représentées, individus mal représentés, variables supplémentaires, séparation des groupes.
7. Remarques sur le traitement et guide de lecture.

### Feuille `ACP_Variables`
- **Cercle des corrélations** (flèches foncées = variables bien représentées sur le plan, pointillés = variables supplémentaires).
- **Contributions** des variables aux deux axes du plan (seuil de contribution moyenne en pointillé).
- Statistiques descriptives (moyenne, écart-type, min, max, coefficient de variation, manquants, MSA), **matrice des corrélations** colorée, coordonnées, corrélations variables-axes, cos², contributions, vecteurs propres, variables supplémentaires.

### Feuille `ACP_Individus`
- **Carte des individus** (couleur par groupe, barycentres des groupes, étiquettes des individus les plus contributifs au-delà du nombre maximal choisi).
- 15 individus les plus contributifs à chacun des deux axes.
- Barycentres et eta² des groupes ; tableau filtrable de tous les individus : coordonnées, cos², contributions, distance au centre, qualité de représentation sur le plan.

### Choisir les dimensions des graphiques

Le plan affiché au départ est celui de la feuille Accueil (« Axe horizontal / vertical », 1-2 par défaut). Ensuite, en haut des feuilles `ACP_Variables` et `ACP_Individus`, une barre permet de choisir **l'axe horizontal et l'axe vertical** (listes déroulantes) puis de cliquer sur **« Actualiser les graphiques »** : le cercle des corrélations, la carte des individus et les graphiques de contributions sont redessinés pour ce plan (par exemple Dim.1-Dim.3 ou Dim.2-Dim.3), sans relancer l'ACP. Chaque feuille peut afficher un plan différent.

Au moins les 10 premières dimensions (ou toutes si l'ACP en compte moins) sont disponibles ; pour aller plus loin, augmentez « Nombre d'axes détaillés » et relancez l'ACP. Les tableaux, eux, ne changent pas (la colonne « Qualité plan » reste celle du plan initial, mais les cos² de chaque dimension y figurent).

Les résultats nécessaires sont conservés dans la feuille masquée `ACP_Stock` : le changement de plan fonctionne aussi après avoir fermé et rouvert le classeur. Les données des graphiques sont stockées dans les feuilles masquées `ACP_GraphSyn`, `ACP_GraphVar` et `ACP_GraphInd`. « Effacer les résultats » supprime les feuilles `ACP_*`.

## Méthode de calcul

Pour *n* individus et *p* variables actives :

- Centrage (et réduction si ACP normée) avec l'écart-type à diviseur *n*, comme dans les logiciels d'analyse de données (FactoMineR, SPAD…).
- Diagonalisation de la matrice des corrélations (ou des covariances) par la méthode de **Jacobi** ; orientation des axes : la composante de plus grande valeur absolue de chaque vecteur propre est positive.
- Coordonnées des individus `F = Z·u`, cos² = `F² / d²`, contributions = `F² / (n·λ)`.
- Coordonnées des variables `u·√λ` (= corrélations en ACP normée), contributions = `u²`.
- Bartlett : `χ² = −(n − 1 − (2p + 5)/6) · ln|R|`, `p(p−1)/2` ddl. KMO à partir des corrélations partielles (inverse de *R*).

Les calculs ont été vérifiés contre numpy (écarts de l'ordre de 1e-15) sur le fichier d'exemple.

## Exemple

`exemple/climat_villes.csv` (données fictives, `;` et virgule décimale) : 30 villes, 7 indicateurs climatiques, une colonne « Zone » à utiliser comme groupe et une colonne « Population » à mettre en supplémentaire.

## Fichiers

| Fichier | Contenu |
|---|---|
| `src/modInstall.bas` | Création des feuilles, de l'interface et des boutons |
| `src/modCSV.bas` | Lecture du CSV (encodage, séparateurs, guillemets), conversion des nombres, tableau des colonnes |
| `src/modMatrice.bas` | Diagonalisation de Jacobi, tris, corrélations |
| `src/modACP.bas` | Calcul de l'ACP, tests, tableaux, interprétation automatique |
| `src/modGraphiques.bas` | Éboulis, cercle des corrélations, carte des individus, contributions |
| `construire_classeur.bat` / `.ps1` | Construction automatique de `ACP_Excel.xlsm` |

Les fichiers `.bas` sont encodés en Windows-1252 avec fins de ligne CRLF, l'encodage attendu par l'éditeur VBA pour conserver les accents.

## Limites

- Windows uniquement pour l'import automatique de l'encodage (objet `ADODB.Stream`) ; sur Mac, les fichiers sont lus selon la page de code du système.
- Le nombre de lignes est limité par Excel (1 048 575 individus) ; au-delà de quelques dizaines de milliers d'individus, la création des graphiques et la mise en forme ralentissent (les échelles de couleur des tableaux d'individus sont désactivées au-delà de 20 000 lignes).
- Le cercle des corrélations affiche au plus 220 variables actives et 30 supplémentaires (limite de séries d'un graphique Excel).
