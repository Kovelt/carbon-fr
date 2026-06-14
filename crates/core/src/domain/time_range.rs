//! Intervalle temporel pour les requêtes d'historique.

use time::OffsetDateTime;

/// Intervalle temporel **semi-ouvert** `[start, end)`.
///
/// Semi-ouvert pour que des intervalles adjacents (mois successifs, par ex.) se
/// juxtaposent sans recouvrement ni trou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    start: OffsetDateTime,
    end: OffsetDateTime,
}

impl TimeRange {
    /// Construit l'intervalle `[start, end)`.
    ///
    /// Renvoie `None` si `end` n'est pas strictement postérieur à `start`.
    pub fn new(start: OffsetDateTime, end: OffsetDateTime) -> Option<Self> {
        if end > start {
            Some(Self { start, end })
        } else {
            None
        }
    }

    pub fn start(&self) -> OffsetDateTime {
        self.start
    }

    pub fn end(&self) -> OffsetDateTime {
        self.end
    }

    /// Vrai si `at` appartient à `[start, end)`.
    pub fn contains(&self, at: OffsetDateTime) -> bool {
        at >= self.start && at < self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn rejects_empty_or_inverted() {
        let t = OffsetDateTime::UNIX_EPOCH;
        assert!(TimeRange::new(t, t).is_none());
        assert!(TimeRange::new(t + Duration::hours(1), t).is_none());
    }

    #[test]
    fn contains_is_half_open() {
        let t = OffsetDateTime::UNIX_EPOCH;
        let range = TimeRange::new(t, t + Duration::hours(1)).unwrap();
        assert!(range.contains(t));
        assert!(range.contains(t + Duration::minutes(30)));
        assert!(!range.contains(t + Duration::hours(1)));
    }
}
