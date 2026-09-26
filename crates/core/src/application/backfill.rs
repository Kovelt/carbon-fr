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

/// Rapatrie l'historique en **tranches temporelles successives**, chaque
/// tranche faisant l'objet d'un export de masse (un téléchargement), puis d'un
/// upsert conditionnel au millésime (ADR-0006).
///
/// Le découpage borne la mémoire et la taille de chaque export. Périmètre :
/// national ([`execute`](Self::execute)) et régional
/// ([`execute_regional`](Self::execute_regional)) — les deux exports de masse
/// ODRÉ existent, chacun sur son propre jeu de données (ADR-0003 addendum
/// 2026-09-26).
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

    /// Exécute le backfill **national** sur `range` et retourne le bilan.
    /// Chaque mesure exportée est enrichie de sa dérivée cycle de vie
    /// (`acv-ademe`, ADR-0008) avant l'upsert.
    pub async fn execute(&self, range: TimeRange) -> Result<BackfillReport, ApplicationError> {
        let mut report = BackfillReport::default();
        for slice in slice_range(range, self.window) {
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
        }
        Ok(report)
    }

    /// Exécute le backfill **régional** sur `range` et retourne le bilan (item
    /// PROD-1). Même découpage en tranches que [`execute`](Self::execute), mais
    /// **sans** dérivation cycle de vie à l'upsert : les mesures rendues par
    /// [`Eco2mixArchive::export_regional`] sont déjà `acv-ademe`, dérivées à
    /// l'adaptation (ADR-0003 addendum 2026-09-26).
    pub async fn execute_regional(
        &self,
        range: TimeRange,
    ) -> Result<BackfillReport, ApplicationError> {
        let mut report = BackfillReport::default();
        for slice in slice_range(range, self.window) {
            let batch = self.archive.export_regional(slice).await?;
            report.read += batch.len();
            report.written += self.repository.upsert_many(&batch).await?;
            report.windows += 1;
        }
        Ok(report)
    }
}

/// Découpe `range` en tranches successives de largeur `window`, sans trou ni
/// chevauchement (factorisé entre [`BackfillHistory::execute`] et
/// [`BackfillHistory::execute_regional`]). Une largeur nulle ou négative ne
/// produit aucune tranche.
fn slice_range(range: TimeRange, window: Duration) -> Vec<TimeRange> {
    let mut slices = Vec::new();
    if window <= Duration::ZERO {
        return slices;
    }

    let mut start = range.start();
    while start < range.end() {
        // Borne la tranche à la fin de l'intervalle, sans déborder.
        let end = start
            .checked_add(window)
            .map(|candidate| candidate.min(range.end()))
            .unwrap_or_else(|| range.end());

        let Some(slice) = TimeRange::new(start, end) else {
            break;
        };
        slices.push(slice);
        start = end;
    }
    slices
}
