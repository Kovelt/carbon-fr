//! Cas d'usage : ingérer une **fenêtre glissante** récente d'une région.
//!
//! Complète [`IngestLatest`](super::IngestLatest) sans le remplacer : le
//! poller n'ingérant que le **dernier** point par cycle, tout retard de
//! publication d'ODRÉ (jeu temps réel publié avec un différé, cf. addendum
//! ADR-0003) se traduisait par un point définitivement perdu — jamais
//! rattrapé, faute de second passage sur ce créneau. `IngestRecent` relit les
//! `N` dernières heures à chaque cycle via le port existant
//! [`Eco2mixSource::range`] : les points déjà stockés sont simplement
//! ré-upsertés sans effet (même millésime), et les points apparus en retard
//! depuis le cycle précédent sont comblés — **sans appel ODRÉ supplémentaire**
//! (toujours un seul appel par zone et par cycle, cf. ADR-0003 addendum
//! 2026-09-25).

use time::{Duration, OffsetDateTime};

use crate::domain::{Measurement, Methodology, Region, TimeRange, derive_acv_ademe};
use crate::ports::{Eco2mixSource, IntensityRepository};

use super::ApplicationError;

/// Largeur par défaut de la fenêtre glissante (3 h) : couvre largement le
/// différé de publication observé du jeu temps réel d'ODRÉ, pour un coût
/// mémoire/upsert négligeable (~12 points au pas quart d'heure).
pub const DEFAULT_WINDOW: Duration = Duration::hours(3);

/// Bilan d'une ingestion de fenêtre glissante.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IngestReport {
    /// Mesures effectivement écrites ou mises à jour (upsert conditionnel au
    /// millésime, ADR-0006). Compte la mesure source **et**, le cas échéant,
    /// sa dérivée `acv-ademe`.
    pub written: usize,
    /// Mesure la plus récente de la fenêtre (par horodatage), telle que reçue
    /// de la source — **avant** dérivation. `None` si la fenêtre est vide
    /// (retard de publication supérieur à la largeur de fenêtre : pas une
    /// erreur, cf. [`execute`](IngestRecent::execute)). Fourni aux appelants
    /// qui n'ont besoin que du dernier point (le poller de `carbonfr-server`
    /// relit, lui, la dernière mesure en base pour le SSE et les métriques).
    pub latest: Option<Measurement>,
}

/// Ingère les mesures d'une région sur une fenêtre `[now - window, now)` et les
/// persiste via l'upsert conditionnel au millésime (ADR-0006). C'est
/// l'opération que le poller exécute périodiquement (ADR-0003 addendum
/// 2026-09-25), à la place d'[`IngestLatest`](super::IngestLatest) mais via le
/// **même** port
/// [`Eco2mixSource::range`] déjà utilisé pour le rattrapage de courts trous —
/// aucune nouvelle capacité côté source, aucun appel ODRÉ de plus.
pub struct IngestRecent<S: Eco2mixSource, R: IntensityRepository> {
    source: S,
    repository: R,
    window: Duration,
}

impl<S: Eco2mixSource, R: IntensityRepository> IngestRecent<S, R> {
    /// `window` : largeur de la fenêtre glissante interrogée à chaque appel.
    /// Une largeur nulle ou négative produit une fenêtre systématiquement vide
    /// (aucune mesure lue), sans erreur — comme [`BackfillHistory`](
    /// super::BackfillHistory) pour une tranche nulle.
    pub fn new(source: S, repository: R, window: Duration) -> Self {
        Self {
            source,
            repository,
            window,
        }
    }

    /// Construit avec la largeur par défaut ([`DEFAULT_WINDOW`], 3 h).
    pub fn with_default_window(source: S, repository: R) -> Self {
        Self::new(source, repository, DEFAULT_WINDOW)
    }

    /// Exécute l'ingestion pour `region`, la fenêtre étant calculée à partir de
    /// `now` — **jamais lu depuis l'horloge système par ce cas d'usage**, pour
    /// rester pur et testable (l'appelant, seul, connaît « maintenant »).
    ///
    /// Une fenêtre sans aucune mesure (retard de publication d'ODRÉ supérieur à
    /// la largeur de fenêtre, ou source momentanément vide) **n'est pas une
    /// erreur** : elle rend un bilan à zéro. Seule une erreur de la source ou
    /// du repository remonte en [`ApplicationError`].
    pub async fn execute(
        &self,
        region: Region,
        now: OffsetDateTime,
    ) -> Result<IngestReport, ApplicationError> {
        let Some(range) = TimeRange::new(now - self.window, now) else {
            return Ok(IngestReport::default());
        };

        let measurements = self.source.range(region, range).await?;
        if measurements.is_empty() {
            return Ok(IngestReport::default());
        }

        let latest = measurements.iter().max_by_key(|m| m.at).cloned();

        // Dérive `acv-ademe` pour chaque mesure qui n'en est pas déjà une (même
        // règle qu'IngestLatest) : la variante régionale, elle, est déjà servie
        // en acv-ademe par la source et n'est pas re-dérivée.
        let mut batch = Vec::with_capacity(measurements.len() * 2);
        for measurement in measurements {
            if measurement.methodology != Methodology::acv_ademe()
                && let Some(acv) = derive_acv_ademe(&measurement)
            {
                batch.push(acv);
            }
            batch.push(measurement);
        }

        let written = self.repository.upsert_many(&batch).await?;
        Ok(IngestReport { written, latest })
    }
}
