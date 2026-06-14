//! Statistiques de consultation de l'API (compteur de visiteurs sobre).
//!
//! Aucune donnée personnelle ici : le domaine ne manipule qu'une **clé visiteur
//! déjà anonymisée** (hachée par l'adapter HTTP) et des agrégats.

use time::Date;

/// Statistiques de consultation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VisitStats {
    /// Visiteurs uniques (clés distinctes).
    pub unique: u64,
    /// Visiteur-jours cumulés (clé × jour distincts).
    pub total: u64,
    /// Premier jour comptabilisé, `None` si aucun.
    pub since: Option<Date>,
}
