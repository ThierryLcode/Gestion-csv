//! Espace de travail (fichiers sur disque + métadonnées en mémoire) et recettes
//! (enregistrées durablement dans un fichier JSON).

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use crate::csvio::{build_index, Res, Scratch, Written};
use crate::sql::SqlEngine;
use crate::steps::Step;
use crate::text::fold;

/// Nombre de lignes entre deux points d'index pour la pagination.
pub const INDEX_STEP: u64 = 1000;

pub struct Entry {
    pub id: String,
    name: RwLock<String>,
    /// Provenance : import, fusion, découpage, jointure, comparaison, sql, ou le type d'étape.
    pub origin: String,
    pub delimiter: u8,
    pub encoding: String,
    pub headers: Vec<String>,
    pub rows: u64,
    pub size: u64,
    pub path: PathBuf,
    /// Étapes appliquées depuis la dernière source (import ou opération multi-fichiers) :
    /// c'est ce qui permet d'enregistrer une recette à partir d'un résultat.
    pub steps: Vec<Step>,
    /// Nom de table utilisable dans les requêtes SQL.
    pub sql_name: String,
    index: OnceLock<Vec<csv::Position>>,
}

impl Entry {
    pub fn name(&self) -> String {
        self.name.read().unwrap().clone()
    }

    pub fn index(&self) -> Res<&[csv::Position]> {
        if let Some(i) = self.index.get() {
            return Ok(i);
        }
        let idx = build_index(&self.path, INDEX_STEP)?;
        let _ = self.index.set(idx);
        Ok(self.index.get().unwrap())
    }
}

impl Drop for Entry {
    /// Le fichier disparaît quand plus personne ne l'utilise.
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Serialize)]
pub struct FileInfo {
    pub id: String,
    pub name: String,
    pub origin: String,
    pub delimiter: String,
    pub encoding: String,
    pub columns: Vec<String>,
    pub row_count: u64,
    pub size: u64,
    pub steps: Vec<Step>,
    pub sql_name: String,
}

impl From<&Entry> for FileInfo {
    fn from(e: &Entry) -> Self {
        FileInfo {
            id: e.id.clone(),
            name: e.name(),
            origin: e.origin.clone(),
            delimiter: (e.delimiter as char).to_string(),
            encoding: e.encoding.clone(),
            columns: e.headers.clone(),
            row_count: e.rows,
            size: e.size,
            steps: e.steps.clone(),
            sql_name: e.sql_name.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recipe {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub created_at: u64,
}

pub struct Inner {
    pub scratch: Scratch,
    files: RwLock<Vec<Arc<Entry>>>,
    recipes: RwLock<Vec<Recipe>>,
    recipes_path: PathBuf,
    pub sql: Mutex<SqlEngine>,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

/// Paramètres décrivant un fichier à ajouter à l'espace de travail.
pub struct NewFile {
    pub name: String,
    pub origin: String,
    pub delimiter: u8,
    pub encoding: String,
    pub steps: Vec<Step>,
}

const SQL_KEYWORDS: [&str; 16] = [
    "select", "from", "where", "group", "order", "by", "join", "on", "as", "table", "union", "values", "index", "limit",
    "having", "and",
];

fn sql_base(name: &str) -> String {
    let stem = match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    };
    let mut s = String::new();
    for c in fold(stem).chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('_') {
            s.push('_');
        }
    }
    let s = s.trim_matches('_').to_string();
    if s.is_empty() {
        "table".into()
    } else if s.as_bytes()[0].is_ascii_digit() {
        format!("t_{s}")
    } else if SQL_KEYWORDS.contains(&s.as_str()) {
        format!("{s}_t")
    } else {
        s
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl AppState {
    /// Prépare le répertoire de données : `travail/` est vidé à chaque démarrage,
    /// `recettes.json` est conservé.
    pub fn open(data_dir: &Path, sort_budget: usize) -> Res<Self> {
        let work = data_dir.join("travail");
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).map_err(|e| format!("Impossible de créer {} : {e}", work.display()))?;
        let recipes_path = data_dir.join("recettes.json");
        let recipes: Vec<Recipe> = match std::fs::read_to_string(&recipes_path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| format!("{} illisible : {e}", recipes_path.display()))?,
            Err(_) => Vec::new(),
        };
        let sql = SqlEngine::open(&work.join("requetes.sqlite"))?;
        Ok(AppState(Arc::new(Inner {
            scratch: Scratch { dir: work, sort_budget },
            files: RwLock::new(Vec::new()),
            recipes: RwLock::new(recipes),
            recipes_path,
            sql: Mutex::new(sql),
        })))
    }

    pub fn add(&self, w: Written, f: NewFile) -> Arc<Entry> {
        let mut files = self.files.write().unwrap();
        let base = sql_base(&f.name);
        let mut sql_name = base.clone();
        let mut n = 2;
        while files.iter().any(|e| e.sql_name == sql_name) {
            sql_name = format!("{base}_{n}");
            n += 1;
        }
        let entry = Arc::new(Entry {
            id: uuid::Uuid::new_v4().simple().to_string(),
            name: RwLock::new(f.name),
            origin: f.origin,
            delimiter: f.delimiter,
            encoding: f.encoding,
            headers: w.headers,
            rows: w.rows,
            size: w.size,
            path: w.path,
            steps: f.steps,
            sql_name,
            index: OnceLock::new(),
        });
        files.push(entry.clone());
        entry
    }

    pub fn get(&self, id: &str) -> Option<Arc<Entry>> {
        self.files.read().unwrap().iter().find(|e| e.id == id).cloned()
    }

    pub fn list(&self) -> Vec<Arc<Entry>> {
        self.files.read().unwrap().clone()
    }

    pub fn remove(&self, id: &str) -> bool {
        let removed = {
            let mut files = self.files.write().unwrap();
            let pos = files.iter().position(|e| e.id == id);
            pos.map(|p| files.remove(p))
        };
        match removed {
            Some(e) => {
                self.sql.lock().unwrap().forget(&e.sql_name);
                true
            }
            None => false,
        }
    }

    pub fn clear(&self) {
        let all: Vec<Arc<Entry>> = std::mem::take(&mut *self.files.write().unwrap());
        let mut sql = self.sql.lock().unwrap();
        for e in all {
            sql.forget(&e.sql_name);
        }
    }

    pub fn rename(&self, id: &str, name: String) -> Option<Arc<Entry>> {
        let e = self.get(id)?;
        *e.name.write().unwrap() = name;
        Some(e)
    }

    // --- Recettes -----------------------------------------------------------

    pub fn recipes(&self) -> Vec<Recipe> {
        self.recipes.read().unwrap().clone()
    }

    pub fn recipe(&self, id: &str) -> Option<Recipe> {
        self.recipes.read().unwrap().iter().find(|r| r.id == id).cloned()
    }

    fn save_recipes(&self, recipes: &[Recipe]) -> Res<()> {
        let json = serde_json::to_string_pretty(recipes).map_err(|e| e.to_string())?;
        let tmp = self.recipes_path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("Enregistrement des recettes impossible : {e}"))?;
        std::fs::rename(&tmp, &self.recipes_path).map_err(|e| format!("Enregistrement des recettes impossible : {e}"))
    }

    pub fn add_recipe(&self, mut r: Recipe) -> Res<Recipe> {
        if r.name.trim().is_empty() {
            return Err("Donnez un nom à la recette".into());
        }
        if r.steps.is_empty() {
            return Err("La recette ne contient aucune étape".into());
        }
        r.name = r.name.trim().to_string();
        r.id = uuid::Uuid::new_v4().simple().to_string();
        if r.created_at == 0 {
            r.created_at = now();
        }
        let mut recipes = self.recipes.write().unwrap();
        recipes.push(r.clone());
        if let Err(e) = self.save_recipes(&recipes) {
            recipes.pop();
            return Err(e);
        }
        Ok(r)
    }

    pub fn remove_recipe(&self, id: &str) -> Res<bool> {
        let mut recipes = self.recipes.write().unwrap();
        let before = recipes.len();
        recipes.retain(|r| r.id != id);
        if recipes.len() == before {
            return Ok(false);
        }
        self.save_recipes(&recipes)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_sql() {
        assert_eq!(sql_base("Ventes 2024 (été).csv"), "ventes_2024_ete");
        assert_eq!(sql_base("2024.csv"), "t_2024");
        assert_eq!(sql_base("select.csv"), "select_t");
        assert_eq!(sql_base("***"), "table");
    }
}
