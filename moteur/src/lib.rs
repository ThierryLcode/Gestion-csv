//! Moteur de Gestion CSV : lecture/écriture en flux, opérations sur un ou
//! plusieurs fichiers, profil, SQL, recettes et espace de travail.
//!
//! Il est partagé par l'interface Web (`gestion-csv`) et l'application de
//! bureau (`gestion-csv-bureau`). Le point d'entrée habituel est
//! [`store::AppState`], dont les méthodes de [`service`] exécutent une opération
//! et ajoutent son résultat à l'espace de travail.

pub mod csvio;
pub mod multi;
pub mod profile;
pub mod service;
pub mod sort;
pub mod sql;
pub mod steps;
pub mod store;
pub mod text;

pub use csvio::Res;
pub use store::{AppState, Entry, FileInfo, Recipe};
