# Gestion CSV

Application Web écrite en Rust pour manipuler des fichiers CSV : **fusionner**, **découper**, **joindre** et **filtrer**, avec gestion des séparateurs et des encodages.

Le serveur ([axum](https://github.com/tokio-rs/axum)) embarque l'interface (HTML/CSS/JS sans dépendance) dans un binaire unique : aucune installation côté navigateur, aucun fichier envoyé sur Internet.

## Démarrage

```bash
cargo run --release
# puis ouvrir http://127.0.0.1:8080
```

Variables d'environnement :

| Variable        | Défaut      | Rôle                                            |
|-----------------|-------------|-------------------------------------------------|
| `HOST`          | `127.0.0.1` | Adresse d'écoute (`0.0.0.0` pour le réseau local) |
| `PORT`          | `8080`      | Port d'écoute                                   |
| `MAX_UPLOAD_MB` | `512`       | Taille maximale d'un envoi                      |

Tests : `cargo test`

## Fonctionnalités

### Import et séparateurs
- Import multiple par glisser-déposer.
- **Séparateur** détecté automatiquement (`,` `;` tabulation `|` `:`) ou imposé (y compris un caractère quelconque).
- **Encodage** détecté (UTF-8, BOM UTF-8/UTF-16, repli Windows-1252 pour les exports Excel) ou imposé.
- Fichiers avec ou sans ligne d'en-têtes, suppression optionnelle des espaces, lignes de longueur irrégulière complétées, noms de colonnes vides ou dupliqués corrigés.

### Export
Séparateur de sortie (ou identique à la source), encodage (UTF-8, UTF-8 avec BOM pour Excel, Windows-1252, ISO-8859-15), guillemets systématiques ou non, fin de ligne LF/CRLF, avec ou sans en-têtes.

### Fusionner
Empile 2 fichiers ou plus, colonnes alignées :
- **Union** (toutes les colonnes), **Intersection** (colonnes communes), **Stricte** (erreur si différentes), **Par position**.
- Colonne optionnelle indiquant le fichier d'origine, suppression optionnelle des doublons.

### Découper
Un fichier par valeur (ou combinaison de valeurs) d'une ou plusieurs colonnes, estimation du nombre de fichiers, téléchargement individuel ou en **archive ZIP**.

### Joindre
Sur une ou plusieurs paires de colonnes (noms différents autorisés), types : **interne**, **gauche**, **droite**, **complète**, **gauche sans correspondance**, **droite sans correspondance**, **semi-jointure**. Options : ignorer la casse, ignorer les espaces, suffixe des colonnes en doublon.

### Filtrer
Conditions combinées en ET / OU : égal à l'une des valeurs (avec sélection parmi les valeurs existantes et leurs effectifs), différent, contient, commence/finit par, vide / non vide, `>` `≥` `<` `≤`, entre (nombres au format français « 1 234,5 » ou dates AAAA-MM-JJ), expression régulière. Respect de la casse au choix.

### Enchaînement
Chaque résultat est ajouté à l'espace de travail : il peut être prévisualisé, renommé, téléchargé ou réutilisé dans une autre opération (ex. filtrer puis joindre puis découper).

> L'espace de travail est conservé **en mémoire** tant que le serveur tourne.

## API

| Méthode | Route | Description |
|---|---|---|
| `GET` / `POST` / `DELETE` | `/api/files` | Lister / importer (multipart : `file`, `delimiter`, `encoding`, `has_headers`, `trim`) / tout supprimer |
| `GET` / `PATCH` / `DELETE` | `/api/files/{id}` | Infos / renommer `{name}` / supprimer |
| `GET` | `/api/files/{id}/rows?offset=&limit=` | Lignes paginées |
| `GET` | `/api/files/{id}/values?column=` | Valeurs distinctes et effectifs |
| `GET` | `/api/files/{id}/download?delimiter=&encoding=&quote=&line_ending=&headers=` | Téléchargement CSV |
| `GET` | `/api/zip?ids=a,b&name=` (+ options d'export) | Archive ZIP |
| `POST` | `/api/merge` | `{ids, mode, source_column?, dedupe, name?}` |
| `POST` | `/api/split` | `{id, columns, remove_columns}` |
| `POST` | `/api/join` | `{left_id, right_id, left_columns, right_columns, kind, case_insensitive, trim, right_suffix?, name?}` |
| `POST` | `/api/filter` | `{id, conditions: [{column, operator, values, case_sensitive}], combine: "all"\|"any", name?}` |

## Structure

```
src/main.rs     démarrage du serveur
src/api.rs      routes HTTP
src/csvdata.rs  lecture/écriture, détection séparateur et encodage
src/ops.rs      fusion, découpage, jointure, filtre
src/store.rs    espace de travail en mémoire
static/         interface Web (embarquée dans le binaire)
```
