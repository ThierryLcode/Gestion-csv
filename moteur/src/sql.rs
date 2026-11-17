//! Requêtes SQL sur les fichiers de l'espace de travail, via SQLite embarqué.
//!
//! Chaque fichier est chargé à la demande dans une base SQLite sur disque (dans
//! le répertoire de travail), ce qui permet d'interroger des fichiers plus gros
//! que la mémoire. Toutes les valeurs sont stockées comme texte ; des fonctions
//! utilitaires facilitent les conversions (voir `register_functions`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use regex::Regex;
use rusqlite::functions::FunctionFlags;
use rusqlite::types::ValueRef;
use rusqlite::Connection;

use crate::csvio::{reader, io_err, unique_headers, Res, Sink, Written};
use crate::text::{fold, parse_date_auto, parse_number, DateFormat};

pub struct SqlTable<'a> {
    pub name: &'a str,
    pub id: &'a str,
    pub path: &'a Path,
    pub headers: &'a [String],
}

pub struct SqlEngine {
    conn: Connection,
    /// Tables chargées : nom SQL → identifiant du fichier.
    loaded: HashMap<String, String>,
}

fn sql_err(e: rusqlite::Error) -> String {
    format!("Erreur SQL : {e}")
}

pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn register_functions(conn: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    // num(x) : convertit « 1 234,5 » en nombre (NULL si non numérique).
    conn.create_scalar_function("num", 1, flags, |ctx| {
        Ok(match ctx.get_raw(0) {
            ValueRef::Integer(i) => Some(i as f64),
            ValueRef::Real(f) => Some(f),
            ValueRef::Text(t) => parse_number(&String::from_utf8_lossy(t)),
            _ => None,
        })
    })?;
    // date_iso(x) : convertit une date (JJ/MM/AAAA, …) en AAAA-MM-JJ (NULL si non reconnue).
    let iso = DateFormat::parse("AAAA-MM-JJ").unwrap();
    conn.create_scalar_function("date_iso", 1, flags, move |ctx| {
        Ok(match ctx.get_raw(0) {
            ValueRef::Text(t) => parse_date_auto(&String::from_utf8_lossy(t)).map(|d| iso.write(&d)),
            _ => None,
        })
    })?;
    // sans_accents(x) : minuscules sans accents, pratique pour comparer.
    conn.create_scalar_function("sans_accents", 1, flags, |ctx| {
        Ok(match ctx.get_raw(0) {
            ValueRef::Text(t) => Some(fold(&String::from_utf8_lossy(t))),
            _ => None,
        })
    })?;
    // x REGEXP 'motif' (appelle regexp(motif, x)).
    conn.create_scalar_function("regexp", 2, flags, |ctx| {
        let re: Arc<Regex> = ctx.get_or_create_aux(0, |v| -> Result<Regex, Box<dyn std::error::Error + Send + Sync>> {
            Ok(Regex::new(v.as_str()?)?)
        })?;
        Ok(match ctx.get_raw(1) {
            ValueRef::Text(t) => re.is_match(&String::from_utf8_lossy(t)),
            ValueRef::Null => false,
            other => re.is_match(&format!("{:?}", other)),
        })
    })?;
    Ok(())
}

/// Repère les noms de tables cités dans une requête (identifiants nus ou entre guillemets).
fn referenced(sql: &str, candidates: &[&str]) -> Vec<String> {
    let lower: Vec<String> = candidates.iter().map(|c| c.to_lowercase()).collect();
    let mut found = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let push = |w: &mut String, found: &mut Vec<String>| {
        let l = w.to_lowercase();
        if let Some(i) = lower.iter().position(|c| *c == l)
            && !found.contains(&candidates[i].to_string())
        {
            found.push(candidates[i].to_string());
        }
        w.clear();
    };
    for c in sql.chars() {
        match quote {
            Some(q) if c == q || (q == '[' && c == ']') => {
                push(&mut word, &mut found);
                quote = None;
            }
            Some('\'') => {}
            Some(_) => word.push(c),
            None if c == '"' || c == '`' || c == '[' || c == '\'' => {
                push(&mut word, &mut found);
                quote = Some(c);
            }
            None if c.is_alphanumeric() || c == '_' => word.push(c),
            None => push(&mut word, &mut found),
        }
    }
    push(&mut word, &mut found);
    found
}

impl SqlEngine {
    pub fn open(path: &Path) -> Res<Self> {
        let _ = std::fs::remove_file(path);
        let conn = Connection::open(path).map_err(sql_err)?;
        conn.execute_batch(
            "PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA temp_store=FILE; PRAGMA cache_size=-65536;",
        )
        .map_err(sql_err)?;
        register_functions(&conn).map_err(sql_err)?;
        Ok(SqlEngine { conn, loaded: HashMap::new() })
    }

    /// Supprime la table d'un fichier retiré de l'espace de travail.
    pub fn forget(&mut self, name: &str) {
        if self.loaded.remove(name).is_some() {
            let _ = self.conn.execute_batch(&format!("DROP TABLE IF EXISTS {}", quote_ident(name)));
        }
    }

    fn ensure(&mut self, t: &SqlTable) -> Res<()> {
        if self.loaded.get(t.name).map(String::as_str) == Some(t.id) {
            return Ok(());
        }
        let table = quote_ident(t.name);
        let cols: Vec<String> = t.headers.iter().map(|h| quote_ident(h)).collect();
        let tx = self.conn.transaction().map_err(sql_err)?;
        tx.execute_batch(&format!("DROP TABLE IF EXISTS {table}; CREATE TABLE {table} ({});", cols.join(", ")))
            .map_err(sql_err)?;
        {
            let placeholders = vec!["?"; t.headers.len()].join(", ");
            let mut ins = tx.prepare(&format!("INSERT INTO {table} VALUES ({placeholders})")).map_err(sql_err)?;
            let mut rdr = reader(t.path)?;
            let mut rec = csv::StringRecord::new();
            while rdr.read_record(&mut rec).map_err(io_err("Erreur de lecture"))? {
                ins.execute(rusqlite::params_from_iter(rec.iter())).map_err(sql_err)?;
            }
        }
        tx.commit().map_err(sql_err)?;
        self.loaded.insert(t.name.to_string(), t.id.to_string());
        Ok(())
    }

    /// Exécute une requête de lecture et écrit le résultat dans un fichier normalisé.
    pub fn query(&mut self, sql: &str, tables: &[SqlTable], out: PathBuf) -> Res<Written> {
        let sql = sql.trim().trim_end_matches(';').trim();
        if sql.is_empty() {
            return Err("Saisissez une requête SQL".into());
        }
        let names: Vec<&str> = tables.iter().map(|t| t.name).collect();
        let used = referenced(sql, &names);
        if used.is_empty() {
            return Err("La requête ne cite aucun fichier : utilisez les noms de tables indiqués dans la liste".into());
        }
        for name in &used {
            let t = tables.iter().find(|t| t.name == name).unwrap();
            self.ensure(t)?;
        }
        let mut stmt = self.conn.prepare(sql).map_err(sql_err)?;
        if !stmt.readonly() {
            return Err("Seules les requêtes de lecture (SELECT, WITH…) sont autorisées".into());
        }
        let headers: Vec<String> = stmt.column_names().map(String::from).collect();
        if headers.is_empty() {
            return Err("La requête ne renvoie aucune colonne".into());
        }
        let n = headers.len();
        let mut sink = Sink::create(out, unique_headers(headers))?;
        let mut rows = stmt.query([]).map_err(sql_err)?;
        let mut vals: Vec<String> = Vec::with_capacity(n);
        while let Some(row) = rows.next().map_err(sql_err)? {
            vals.clear();
            for i in 0..n {
                vals.push(match row.get_ref(i).map_err(sql_err)? {
                    ValueRef::Null => String::new(),
                    ValueRef::Integer(v) => v.to_string(),
                    ValueRef::Real(v) => crate::text::format_number(v, '.'),
                    ValueRef::Text(t) | ValueRef::Blob(t) => String::from_utf8_lossy(t).into_owned(),
                });
            }
            sink.write(&vals)?;
        }
        sink.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csvio::{rows, tests::tmpdir};

    fn src(dir: &Path, name: &str, headers: &[&str], data: &[&[&str]]) -> Written {
        let mut s = Sink::create(dir.join(name), headers.iter().map(|s| s.to_string()).collect()).unwrap();
        for r in data {
            s.write(r).unwrap();
        }
        s.finish().unwrap()
    }

    #[test]
    fn requetes() {
        let d = tmpdir("sql");
        let c = src(&d, "c.csv", &["id", "ville", "montant"], &[&["1", "Paris", "1 200,5"], &["2", "Lyon", "80"], &["3", "Paris", "10"]]);
        let o = src(&d, "o.csv", &["client id", "date"], &[&["1", "05/01/2024"], &["3", "2023-12-31"]]);
        let mut eng = SqlEngine::open(&d.join("db.sqlite")).unwrap();
        let tables = [
            SqlTable { name: "clients", id: "a", path: &c.path, headers: &c.headers },
            SqlTable { name: "commandes", id: "b", path: &o.path, headers: &o.headers },
        ];
        let w = eng
            .query(
                "SELECT ville, SUM(num(montant)) AS total, COUNT(*) n FROM clients GROUP BY ville ORDER BY total DESC;",
                &tables,
                d.join("r1.csv"),
            )
            .unwrap();
        assert_eq!(w.headers, vec!["ville", "total", "n"]);
        let r: Vec<_> = rows(&w.path).unwrap().map(Result::unwrap).collect();
        assert_eq!(r, vec![vec!["Paris", "1210.5", "2"], vec!["Lyon", "80", "1"]]);

        let w = eng
            .query(
                r#"SELECT c.id, date_iso(o."date") d FROM clients c JOIN "commandes" o ON o."client id" = c.id WHERE c.ville REGEXP '^Par' ORDER BY d"#,
                &tables,
                d.join("r2.csv"),
            )
            .unwrap();
        let r: Vec<_> = rows(&w.path).unwrap().map(Result::unwrap).collect();
        assert_eq!(r, vec![vec!["3", "2023-12-31"], vec!["1", "2024-01-05"]]);

        assert!(eng.query("DELETE FROM clients", &tables, d.join("x.csv")).unwrap_err().contains("lecture"));
        assert!(eng.query("SELECT 1", &tables, d.join("y.csv")).is_err());
        assert!(eng.query("SELECT * FROM clients WHERE", &tables, d.join("z.csv")).unwrap_err().starts_with("Erreur SQL"));
    }

    #[test]
    fn tables_citees() {
        assert_eq!(referenced("select * from Clients c join \"ventes 2024\" v", &["clients", "ventes 2024", "autre"]), vec!["clients", "ventes 2024"]);
        assert!(referenced("select 'clients' from x", &["clients"]).is_empty());
    }
}
