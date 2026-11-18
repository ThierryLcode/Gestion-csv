# Projection retraites (application Android)

*Développeur : Thierry LORGEOU*

Application Android qui projette, de 2025 à 2070, l'effet de mesures sur l'équilibre du système de retraite français, à partir des hypothèses démographiques et économiques du **Conseil d'orientation des retraites (COR)**, rapport annuel de juin 2026 (projections de population Insee 2026).

## Ce que l'on peut faire

| Onglet | Contenu |
|---|---|
| **Résultats** | Sélecteur d'unité : % du PIB ou montants en Md€ constants de 2025. Solde du système, dépenses et ressources, cotisants par retraité, pension moyenne relative, âge moyen de départ. Chaque courbe compare *avec* et *sans* mesures ; les losanges ◇ sont les valeurs publiées par le COR. Effort nécessaire pour équilibrer en 2070 (cotisations, pensions ou âge). Export CSV. |
| **Mesures** | Mesures types (départ à 60 ans, retour à 62 ans, âge légal à 65/66 ans, année blanche, sous-indexation, indexation sur les salaires, +1 point de cotisation, baisse des nouvelles pensions) et réglages combinables : décalage de l'âge de départ et rythme, revalorisation des pensions, cotisations, calcul des nouvelles pensions. |
| **Capitalisation** | Pilier par capitalisation (x points de cotisation, en complément ou en substitution des cotisations de répartition, avec coût de transition) et fonds de réserve (capital initial, dotations, placement des excédents, couverture des déficits jusqu'à épuisement). Rendement réel réglable (2 % par défaut). Graphiques des actifs, des rentes, de la pension totale et du solde après fonds. |
| **Hypothèses** | Scénario central Insee 2026 / COR et variantes (fécondité 1,2 ou 1,7 ; solde migratoire +70 000 ou +230 000 ; espérance de vie ; anciennes hypothèses Insee 2021), productivité (0,4 / 0,7 / 1,0 %), chômage, convention de ressources. Import d'une pyramide des âges au format CSV. |
| **Démographie** | Pyramide des âges 2025 comparée à une année au choix, rapport 20-64 / 65 ans ou plus, population par grand groupe d'âge, naissances, espérance de vie. |
| **Méthode** | À propos (développeur, version), sources, méthode et limites. |

Tous les calculs sont faits sur le téléphone ; l'application n'utilise pas le réseau.

## Note technique

La méthodologie et toutes les formules sont décrites dans la note technique Word [`docs/Note-technique-projection-retraites.docx`](docs/Note-technique-projection-retraites.docx), générée par `docs/note-technique.js` (`node note-technique.js Note-technique-projection-retraites.docx`, module npm `docx`).

## Hypothèses de référence (COR, juin 2026)

- Fécondité : 1,45 enfant par femme à partir de 2028 (1,56 en 2025).
- Solde migratoire : +150 000 personnes par an.
- Espérance de vie à la naissance en 2070 : 86,7 ans (hommes), 89,5 ans (femmes) ; 80,3 et 85,9 ans en 2025.
- Productivité du travail : convergence vers 0,7 % par an ; chômage de long terme 7 %.
- Résultats du COR utilisés pour le calage : dépenses 14,1 % du PIB (2025), 14,2 % (2045), 15,3 % (2070) ; ressources 13,9 % → 12,9 % ; solde −0,2 % (2025 et 2030), −0,9 % (2045), −2,4 % (2070).

## Modèle

Le moteur (`modele/`, Kotlin pur, testé) comporte :

1. **Une projection démographique par composantes**, par sexe et âge détaillé (0 à 110 ans et plus) : mortalité paramétrique ajustée chaque année à l'espérance de vie visée, calendrier de fécondité par âge, solde migratoire réparti par âge.
2. **Un modèle du système de retraite** fondé sur l'équation comptable du COR :
   dépenses / PIB = (retraités / cotisants) × (pension moyenne / revenu d'activité moyen) × part des revenus d'activité dans le PIB.
   La part de retraités à chaque âge dépend de l'âge moyen de départ ; les pensions sont suivies par âge (nouvelles pensions liées aux salaires, pensions en cours de service revalorisées sur les prix sauf mesure contraire).

Calage obtenu (tests automatiques) :

| | COR 2026 | Modèle |
|---|---|---|
| Dépenses 2070, productivité 0,7 % | 15,3 % | 15,3 % |
| Dépenses 2070, productivité 0,4 % | 16,1 % | 16,2 % |
| Dépenses 2070, productivité 1,0 % | 14,5 % | 14,5 % |
| Solde 2070 | −2,4 % | −2,4 % |
| Effet d'une fécondité de 1,2 sur le solde 2070 | ≈ −1 pt | −1,1 pt |

3. **La capitalisation** : comptes par âge alimentés selon l'emploi, placés au rendement réel choisi et convertis en rente viagère indexée sur les prix au départ ; en substitution, la répartition perd immédiatement les points basculés et les droits futurs baissent à proportion. Le fonds de réserve capitalise dotations et excédents, puis couvre les déficits jusqu'à épuisement.

**Limites.** Modèle simplifié à visée pédagogique : pas de distinction des régimes, carrières, réversion ou minima ; l'effet d'une mesure d'âge sur l'âge effectif est un ordre de grandeur ; la pyramide intégrée est reconstruite à partir des naissances depuis 1914 et calée sur les agrégats Insee au 1er janvier 2025 (on peut importer la pyramide exacte) ; l'effet du solde migratoire est probablement surestimé. Les résultats ne remplacent pas les projections officielles du COR.

## Importer la pyramide Insee

Fichier CSV (UTF-8) avec une ligne par âge : `âge;hommes;femmes`. Séparateur `;`, `,` ou tabulation ; ligne d'en-tête facultative ; l'âge peut s'écrire `100+` ; décimales avec virgule ou point.

## Compilation

- **Installer depuis le téléphone** : ouvrez https://github.com/ThierryLcode/Gestion-csv/releases/latest/download/ProjectionRetraites.apk dans le navigateur du téléphone, puis touchez le fichier téléchargé (autorisez l'installation depuis le navigateur, puis « Installer quand même » si Play Protect avertit). Android 8.0 ou plus récent.
- **Publication** : le workflow `.github/workflows/android.yml` teste le modèle, compile l'APK à chaque envoi modifiant `retraites/` et le publie dans la version GitHub `retraites-v<versionName>` (créée si besoin, sinon l'APK y est remplacé). Pour une nouvelle version, augmentez `versionName` et `versionCode` dans `app/build.gradle.kts`. L'APK est aussi disponible en artefact « ProjectionRetraites-apk » dans l'onglet **Actions**.
- **En local** : Android Studio (ouvrir le dossier `retraites/`), ou en ligne de commande avec un SDK Android : `./gradlew :app:assembleDebug`.
- **Tests du modèle seuls** (sans SDK Android) : `./gradlew :modele:test`.

Android 8.0 (API 26) ou plus récent.
