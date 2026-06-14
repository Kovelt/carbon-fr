//! Cas d'usage : backfill de l'historique par export de masse (ADR-0003).

use time::Duration;

use crate::domain::{TimeRange, derive_acv_ademe};
use crate::ports::{Eco2mixArchive, IntensityRepository};

use super::ApplicationError;

/// Bilan d'un backfill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BackfillReport {
    /// Mesures lues depuis l'export.
    pub read: usize,
    /// Mesures effectivement écrites ou mises à jour (upsert conditionnel).
    pub written: usize,
    /// Nombre de tranches d'export traitées.
    pub windows: usize,
}

/// Rapatrie l'historique national en **tranches temporelles successives**,
/// chaque tranche faisant l'objet d'un export de masse (un téléchargement), puis
/// d'un upsert conditionnel au millésime (ADR-0006).
///
/// Le découpage borne la mémoire et la taille de chaque export. Périmètre :
/// national — l'intensité régionale n'existe pas à la source (addendum
/// ADR-0003), elle viendra avec le modèle régional.
pub struct BackfillHistory<A: Eco2mixArchive, R: IntensityRepository> {
    archive: A,
    repository: R,
    window: Duration,
}

impl<A: Eco2mixArchive, R: IntensityRepository> BackfillHistory<A, R> {
    /// `window` : largeur de chaque tranche d'export. Une valeur nulle ou
    /// négative produit un backfill vide (aucune tranche).
    pub fn new(archive: A, repository: R, window: Duration) -> Self {
        Self {
            archive,
            repository,
            window,
        }
    }

    /// Exécute le backfill sur `range` et retourne le bilan.
    pub async fn execute(&self, range: TimeRange) -> Result<BackfillReport, ApplicationError> {
        let mut report = BackfillReport::default();
        if self.window <= Duration::ZERO {
            return Ok(report);
        }

        let mut start = range.start();
        while start < range.end() {
            // Borne la tranche à la fin de l'intervalle, sans déborder.
            let end = start
                .checked_add(self.window)
                .map(|candidate| candidate.min(range.end()))
                .unwrap_or_else(|| range.end());

            let Some(slice) = TimeRange::new(start, end) else {
                break;
            };

            let batch = self.archive.export_national(slice).await?;
            report.read += batch.len();

            // Enrichit chaque mesure de sa dérivée cycle de vie (ADR-0008).
            let mut enriched = Vec::with_capacity(batch.len() * 2);
            for measurement in batch {
                if let Some(acv) = derive_acv_ademe(&measurement) {
                    enriched.push(acv);
                }
                enriched.push(measurement);
            }

            report.written += self.repository.upsert_many(&enriched).await?;
            report.windows += 1;

            start = end;
        }

        Ok(report)
    }
}
