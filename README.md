# Gestion CSV

Application écrite en Rust pour manipuler des fichiers CSV, y compris très volumineux : fusionner, découper, joindre, filtrer, trier, dédoublonner, nettoyer, comparer deux versions, profiler, interroger en SQL et rejouer des suites d'opérations (recettes).

Deux interfaces, un seul moteur :

| | Application de bureau | Interface Web |
|---|---|---|
| Programme | `gestion-csv-bureau` (`.exe` sous Windows) | `gestion-csv` (serveur local) |
| Lancement | double-clic, rien d'autre à installer | ouvrir http://127.0.0.1:8080 dans un navigateur |
| Technologie | fenêtre native [egui](https://github.com/emilk/egui) | [axum](https://github.com/tokio-rs/axum) + HTML/CSS/JS embarqués |
| Données | `%LOCALAPPDATA%\GestionCSV` (Windows) | `donnees/` |

Les fichiers ne quittent jamais la machine. Les recettes ont le même format JSON dans les deux interfaces.

## Application de bureau (Windows)

Double-cliquez sur `gestion-csv-bureau.exe` : aucune installation, aucune DLL à fournir. Vous pouvez aussi glisser des fichiers CSV sur la fenêtre (ou sur l'icône du programme, ou utiliser « Ouvrir avec ») pour les importer.

- Mêmes onglets que l'interface Web ; les résultats s'enregistrent avec « Enregistrer sous… » selon les options d'export.
- L'aperçu est un tableau virtuel : on fait défiler (ou on saute directement à une ligne) dans des millions de lignes, seules les lignes visibles sont lues sur le disque.
- Les traitements tournent en arrière-plan avec un indicateur d'attente ; la fenêtre reste réactive.
- Thème clair, sombre ou celui du système ; zoom avec Ctrl + / Ctrl −.
- Rendu Direct3D 12 / Vulkan ; si la carte graphique ne le permet pas (machine virtuelle, bureau à distance, ancien pilote), le programme se relance automatiquement en OpenGL. Pour forcer OpenGL : variable d'environnement `GESTION_CSV_RENDU=opengl`.
- Dossier de données modifiable avec la variable `GESTION_CSV_DATA` ; mémoire de tri avec `SORT_MEMORY_MB`.

### Si Windows bloque le lancement

L'exécutable n'est pas signé numériquement (il faudrait un certificat de signature de code, payant). Selon le message affiché :

- **« Windows a protégé votre ordinateur » (SmartScreen)** : cliquez sur « Informations complémentaires » puis « Exécuter quand même ». Pour ne plus voir ce message : clic droit sur le fichier `.zip` téléchargé (avant de l'extraire) ou sur `GestionCSV.exe` → Propriétés → cochez « Débloquer » → OK.
- **L'antivirus (Microsoft Defender) supprime ou met en quarantaine le fichier** : il s'agit d'un faux positif fréquent pour les programmes non signés. Sécurité Windows → Protection contre les virus et menaces → Historique de protection → « Restaurer » / « Autoriser sur l'appareil ». Préférez l'exécutable compilé par GitHub avec le compilateur Microsoft (voir ci-dessous), moins souvent signalé.
- **« Une partie de cette application a été bloquée » (Smart App Control, Windows 11)** : ce contrôle n'accepte que les programmes signés ou connus de Microsoft et ne propose pas d'exception. Solutions : compiler le programme vous-même sur votre PC (voir ci-dessous), ou désactiver Smart App Control (Sécurité Windows → Contrôle des applications et du navigateur ; la réactivation nécessite de réinstaller Windows).

L'exécutable contient une icône, un manifeste d'application (exécution sans droits d'administrateur) et des informations de version visibles dans Propriétés → Détails.

### Exécutable compilé par GitHub

Le workflow `.github/workflows/windows.yml` compile l'application sous Windows avec le compilateur Microsoft (MSVC) à chaque envoi : l'exécutable est téléchargeable dans l'onglet **Actions** du dépôt (artefact « GestionCSV-windows-x64 »). Une étiquette `v*` (ex. `git tag v0.2.0 && git push --tags`) publie en plus une version GitHub avec le fichier ZIP.

### Compiler

```bash
# Sous Windows (Rust installé via https://rustup.rs, avec les « Build Tools » Visual Studio)
cargo build --release -p gestion-csv-bureau
# -> target\release\gestion-csv-bureau.exe

# Depuis Linux (compilation croisée, configurée dans .cargo/config.toml)
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
cargo build --release -p gestion-csv-bureau --target x86_64-pc-windows-gnu
# -> target/x86_64-pc-windows-gnu/release/gestion-csv-bureau.exe

# Linux / macOS (le même code fonctionne)
cargo run --release -p gestion-csv-bureau
```

## Interface Web

```bash
cargo run --release -p gestion-csv
# puis ouvrir http://127.0.0.1:8080
```

| Variable         | Défaut      | Rôle |
|------------------|-------------|------|
| `HOST`           | `127.0.0.1` | Adresse d'écoute (`0.0.0.0` pour le réseau local) |
| `PORT`           | `8080`      | Port d'écoute |
| `DATA_DIR`       | `donnees`   | Répertoire des données : `travail/` (vidé à chaque démarrage) et `recettes.json` (conservé) |
| `SORT_MEMORY_MB` | `256`       | Mémoire utilisée par bloc lors des tris externes |
| `MAX_UPLOAD_MB`  | `0`         | Taille maximale d'un envoi (`0` = illimitée) |

Tests : `cargo test --workspace`

## Fonctionnalités

### Import, séparateurs et encodages
- Import multiple par glisser-déposer, avec barre de progression.
- **Séparateur** détecté automatiquement (`,` `;` tabulation `|` `:`) ou imposé (n'importe quel caractère).
- **Encodage** détecté (UTF-8, BOM UTF-8/UTF-16, repli Windows-1252 pour les exports Excel) ou imposé.
- Avec ou sans ligne d'en-têtes, suppression optionnelle des espaces, lignes irrégulières complétées, noms de colonnes vides ou dupliqués corrigés.

### Export
Séparateur de sortie, encodage (UTF-8, UTF-8 avec BOM pour Excel, Windows-1252, ISO-8859-15), guillemets, fins de ligne LF/CRLF, avec ou sans en-têtes. Archives ZIP pour les découpages.

### Opérations sur un fichier
| Onglet | Rôle |
|---|---|
| **Profil** | Par colonne : type détecté (entier, décimal, date, booléen, texte), remplissage, valeurs distinctes, min/max, moyenne, longueurs, valeurs les plus fréquentes. Export du profil en CSV. |
| **Colonnes** | Conserver, supprimer, réordonner et renommer les colonnes. |
| **Trier** | Plusieurs critères ; ordre alphabétique (sans casse ni accents), numérique (« 1 234,5 »), naturel (« a2 » < « a10 ») ou chronologique ; vides en fin. |
| **Doublons** | Sur certaines colonnes ou la ligne entière : supprimer (garder la 1re ou la dernière occurrence), isoler les doublons, isoler les lignes écartées, ou ne garder que les lignes uniques. Casse et espaces ignorables. |
| **Filtrer** | Conditions ET / OU : égal à l'une des valeurs (choix parmi les valeurs existantes), contient, commence/finit par, vide, `>` `≥` `<` `≤`, entre (nombres et dates), expression régulière. |
| **Nettoyer & calculer** | Suite d'opérations : espaces superflus, casse, rechercher/remplacer (texte ou regex), conversion de dates (formats `JJ/MM/AAAA`, `AAAA-MM-JJ`…, détection auto), normalisation des nombres, concaténation, extraction par regex, colonne à valeur fixe. |
| **Découper** | Un fichier par valeur (ou combinaison de valeurs) d'une ou plusieurs colonnes, téléchargeables en ZIP. |

### Opérations sur plusieurs fichiers
- **Fusionner** : union, intersection, stricte ou par position ; colonne d'origine et dédoublonnage optionnels.
- **Joindre** : interne, gauche, droite, complète, sans correspondance (gauche/droite), semi-jointure ; clés multiples, casse/espaces ignorables.
- **Comparer** deux versions d'un fichier via une ou plusieurs colonnes identifiantes : lignes ajoutées, supprimées, modifiées (avec la liste des colonnes modifiées et les anciennes valeurs), colonnes apparues/disparues, identifiants en double signalés.

### SQL
Chaque fichier est une table (nom affiché dans l'onglet SQL). Dialecte SQLite, lecture seule, avec des fonctions utiles :
`num(x)` (nombre, format français accepté), `date_iso(x)` (date en `AAAA-MM-JJ`), `sans_accents(x)`, `x REGEXP 'motif'`.

```sql
SELECT c.ville, COUNT(*) AS commandes, SUM(num(o.qte)) AS quantite
FROM clients c JOIN commandes o ON o.client_id = c.id
GROUP BY c.ville ORDER BY quantite DESC
```

### Recettes
Chaque résultat mémorise les opérations appliquées depuis l'import (colonnes, tri, doublons, filtre, nettoyage). « Recette… » les enregistre sous un nom ; la recette peut ensuite être appliquée à d'autres fichiers en un clic, exportée ou importée en JSON. Les recettes sont conservées dans `DATA_DIR/recettes.json`.

### Enchaînement
Chaque résultat rejoint l'espace de travail : il peut être prévisualisé, renommé, téléchargé ou réutilisé dans une autre opération.

## Très gros fichiers

Les fichiers ne sont jamais chargés entièrement en mémoire :
- l'envoi est écrit sur disque au fil de l'eau, puis converti en un fichier normalisé (UTF-8, virgule) ;
- filtre, colonnes, nettoyage, fusion, doublons, découpage, profil et export sont traités ligne à ligne ;
- le tri est un **tri externe** (blocs triés de `SORT_MEMORY_MB` puis fusionnés) ;
- la comparaison trie les deux versions puis les parcourt en parallèle ;
- les requêtes SQL s'exécutent dans une base SQLite sur disque ;
- l'aperçu est paginé grâce à un index de positions ;
- les téléchargements sont envoyés en flux.

Seules exceptions : la jointure indexe en mémoire le fichier de **droite** (placez-y le plus petit) et le dédoublonnage garde une empreinte de 16 octets par identifiant distinct.

Mesures indicatives (version `--release`, fichier de 2 millions de lignes / 124 Mo en Windows-1252, `SORT_MEMORY_MB=32`) : import 2 s, profil 2,7 s, tri externe 4,5 s, doublons 2,7 s, filtre 1 s, comparaison 6 s, agrégat SQL 3 s (0,2 s une fois la table chargée), export converti 0,9 s ; **pic mémoire du serveur : 155 Mo**.

Prévoyez de l'espace disque dans `DATA_DIR` : chaque résultat est un nouveau fichier (supprimez ceux dont vous n'avez plus besoin).

## API

| Méthode | Route | Description |
|---|---|---|
| `GET` / `POST` / `DELETE` | `/api/files` | Lister / importer (multipart : `file`, `delimiter`, `encoding`, `has_headers`, `trim`) / tout supprimer |
| `GET` / `PATCH` / `DELETE` | `/api/files/{id}` | Infos / renommer `{name}` / supprimer |
| `GET` | `/api/files/{id}/rows?offset=&limit=` | Lignes paginées |
| `GET` | `/api/files/{id}/values?column=` | Valeurs distinctes et effectifs |
| `GET` | `/api/files/{id}/profile` | Profil des colonnes |
| `GET` | `/api/files/{id}/download?delimiter=&encoding=&quote=&line_ending=&headers=` | Téléchargement |
| `GET` | `/api/zip?ids=a,b&name=` (+ options d'export) | Archive ZIP |
| `POST` | `/api/step` | `{id, step, name?}` — étape `columns`, `sort`, `dedupe`, `filter` ou `transform` |
| `POST` | `/api/merge` | `{ids, mode, source_column?, dedupe, name?}` |
| `POST` | `/api/split` | `{id, columns, remove_columns}` |
| `POST` | `/api/join` | `{left_id, right_id, left_columns, right_columns, kind, case_insensitive, trim, right_suffix?, name?}` |
| `POST` | `/api/compare` | `{old_id, new_id, keys, columns?, include_unchanged, case_insensitive, trim}` |
| `POST` | `/api/sql` | `{query, name?}` |
| `GET` / `POST` | `/api/recipes` | Lister / créer `{name, description?, steps \| from_file}` |
| `GET` / `DELETE` | `/api/recipes/{id}` | Lire / supprimer |
| `POST` | `/api/recipes/{id}/apply` | `{ids}` |

Exemple d'étape (format également utilisé dans les recettes JSON) :

```json
{ "type": "sort", "keys": [{ "column": "montant", "mode": "number", "descending": true }] }
```

## Structure

```
moteur/   bibliothèque partagée (gestion-csv-moteur)
  src/csvio.rs    lecture/écriture en flux, import, export, détection séparateur et encodage
  src/text.rs     nombres français, comparaison sans accents, tri naturel, dates
  src/sort.rs     tri externe
  src/steps.rs    étapes sur un fichier : colonnes, doublons, filtre, nettoyage
  src/multi.rs    fusion, découpage, jointure, comparaison
  src/profile.rs  profil et valeurs distinctes
  src/sql.rs      moteur SQL (SQLite embarqué)
  src/store.rs    espace de travail et recettes
  src/service.rs  opérations de haut niveau utilisées par les deux interfaces
web/      interface Web (gestion-csv) : src/api.rs, static/
bureau/   application de bureau (gestion-csv-bureau)
  src/app.rs      fenêtre, tâches en arrière-plan, fichiers, aperçu virtuel, export
  src/panels.rs   onglets d'opérations
  src/widgets.rs  composants et descriptions des étapes
```
