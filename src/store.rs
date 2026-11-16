//! Espace de travail en mémoire : fichiers importés et résultats d'opérations.

use std::sync::{Arc, RwLock};

use serde::Serialize;

use crate::csvdata::Table;

#[derive(Clone)]
pub struct Entry {
    pub id: String,
    pub name: String,
    /// Provenance : « import », « fusion », « découpage », « jointure », « filtre ».
    pub origin: String,
    pub delimiter: u8,
    pub encoding: String,
    pub table: Arc<Table>,
}

#[derive(Serialize)]
pub struct FileInfo {
    pub id: String,
    pub name: String,
    pub origin: String,
    pub delimiter: String,
    pub encoding: String,
    pub columns: Vec<String>,
    pub row_count: usize,
}

impl From<&Entry> for FileInfo {
    fn from(e: &Entry) -> Self {
        FileInfo {
            id: e.id.clone(),
            name: e.name.clone(),
            origin: e.origin.clone(),
            delimiter: (e.delimiter as char).to_string(),
            encoding: e.encoding.clone(),
            columns: e.table.headers.clone(),
            row_count: e.table.rows.len(),
        }
    }
}

#[derive(Clone, Default)]
pub struct AppState {
    files: Arc<RwLock<Vec<Entry>>>,
}

impl AppState {
    pub fn insert(&self, name: String, origin: &str, delimiter: u8, encoding: String, table: Table) -> Entry {
        let entry = Entry {
            id: uuid::Uuid::new_v4().simple().to_string(),
            name,
            origin: origin.to_string(),
            delimiter,
            encoding,
            table: Arc::new(table),
        };
        self.files.write().unwrap().push(entry.clone());
        entry
    }

    pub fn get(&self, id: &str) -> Option<Entry> {
        self.files.read().unwrap().iter().find(|e| e.id == id).cloned()
    }

    pub fn list(&self) -> Vec<Entry> {
        self.files.read().unwrap().clone()
    }

    pub fn remove(&self, id: &str) -> bool {
        let mut files = self.files.write().unwrap();
        let before = files.len();
        files.retain(|e| e.id != id);
        files.len() != before
    }

    pub fn clear(&self) {
        self.files.write().unwrap().clear();
    }

    pub fn rename(&self, id: &str, name: String) -> Option<Entry> {
        let mut files = self.files.write().unwrap();
        let e = files.iter_mut().find(|e| e.id == id)?;
        e.name = name;
        Some(e.clone())
    }
}
