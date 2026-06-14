//! Traduction entre les lignes SQL et les types du domaine.
//!
//! L'encodage de stockage (rang de millésime, colonnes de mix) est une
//! préoccupation d'adapter : il vit ici, jamais dans `core`.

use std::collections::HashMap;

use carbonfr_core::domain::{
    CarbonIntensity, GenerationMix, IntensityStats, Measurement, MeasurementKey, Methodology,
    Region, RollupBucket, Vintage,
};
use carbonfr_core::ports::RepositoryError;
use sqlx::Row;
use sqlx::postgres::PgRow;
use time::OffsetDateTime;

/// Construit une erreur de backend à partir d'un message.
pub(crate) fn backend(msg: impl Into<String>) -> RepositoryError {
    RepositoryError::Backend(msg.into())
}

/// Rang de qualité du millésime, tel que stocké (ADR-0006).
pub(crate) fn vintage_rank(vintage: Vintage) -> i16 {
    match vintage {
        Vintage::Tr => 0,
        Vintage::Consolidated => 1,
        Vintage::Definitive => 2,
    }
}

/// Réciproque de [`vintage_rank`]. Échoue sur une valeur hors domaine (donnée
/// corrompue en base).
fn vintage_from_rank(rank: i16) -> Result<Vintage, RepositoryError> {
    match rank {
        0 => Ok(Vintage::Tr),
        1 => Ok(Vintage::Consolidated),
        2 => Ok(Vintage::Definitive),
        other => Err(backend(format!(
            "rang de millésime inconnu en base : {other}"
        ))),
    }
}

/// Déduplique par clé `(region, at, methodology)` en conservant la **meilleure
/// qualité de millésime**. Indispensable avant un INSERT multi-lignes :
/// PostgreSQL refuse qu'un même `ON CONFLICT` affecte deux fois la même ligne.
///
/// L'ordre d'origine des survivants est préservé (déterminisme).
pub(crate) fn dedup_by_key(measurements: &[Measurement]) -> Vec<&Measurement> {
    let mut best: HashMap<MeasurementKey, usize> = HashMap::with_capacity(measurements.len());
    for (index, measurement) in measurements.iter().enumerate() {
        match best.get(&measurement.key()) {
            Some(&kept)
                if vintage_rank(measurements[kept].vintage)
                    >= vintage_rank(measurement.vintage) => {}
            _ => {
                best.insert(measurement.key(), index);
            }
        }
    }
    let mut kept: Vec<usize> = best.into_values().collect();
    kept.sort_unstable();
    kept.into_iter().map(|index| &measurements[index]).collect()
}

/// Valeur d'un champ de mix à lier, ou `None` si la mesure n'a pas de mix.
pub(crate) fn mix_field(
    mix: &Option<GenerationMix>,
    extract: impl Fn(&GenerationMix) -> f64,
) -> Option<f64> {
    mix.as_ref().map(extract)
}

/// Reconstruit le mix : `Some` seulement si **toutes** les colonnes sont
/// présentes (écriture atomique), `None` sinon.
fn read_mix(row: &PgRow) -> Result<Option<GenerationMix>, RepositoryError> {
    let get = |col: &str| -> Result<Option<f64>, RepositoryError> {
        row.try_get(col).map_err(backend_from)
    };
    let (nucleaire, gaz, charbon, fioul, hydraulique) = (
        get("mix_nucleaire")?,
        get("mix_gaz")?,
        get("mix_charbon")?,
        get("mix_fioul")?,
        get("mix_hydraulique")?,
    );
    let (eolien, solaire, bioenergies, pompage, echanges) = (
        get("mix_eolien")?,
        get("mix_solaire")?,
        get("mix_bioenergies")?,
        get("mix_pompage")?,
        get("mix_echanges")?,
    );
    // Thermique agrégé (régional) : colonne optionnelle, indépendante du détail
    // par filière. Reste `None` au national.
    let thermique = get("mix_thermique")?;

    Ok((|| {
        Some(GenerationMix {
            nucleaire: nucleaire?,
            gaz: gaz?,
            charbon: charbon?,
            fioul: fioul?,
            hydraulique: hydraulique?,
            eolien: eolien?,
            solaire: solaire?,
            bioenergies: bioenergies?,
            pompage: pompage?,
            echanges: echanges?,
            thermique,
        })
    })())
}

fn backend_from(e: sqlx::Error) -> RepositoryError {
    backend(format!("lecture de colonne : {e}"))
}

/// Mappe une ligne `measurement` complète vers le domaine.
pub(crate) fn row_to_measurement(row: &PgRow) -> Result<Measurement, RepositoryError> {
    let region_slug: String = row.try_get("region").map_err(backend_from)?;
    let region = Region::from_slug(&region_slug)
        .ok_or_else(|| backend(format!("région inconnue en base : {region_slug}")))?;

    let at: OffsetDateTime = row.try_get("at").map_err(backend_from)?;

    let methodology_id: String = row.try_get("methodology_id").map_err(backend_from)?;
    let methodology_version: i32 = row.try_get("methodology_version").map_err(backend_from)?;

    let intensity_value: f64 = row.try_get("intensity").map_err(backend_from)?;
    let intensity = CarbonIntensity::new(intensity_value).ok_or_else(|| {
        backend(format!(
            "intensité hors domaine en base : {intensity_value}"
        ))
    })?;

    let rank: i16 = row.try_get("vintage_rank").map_err(backend_from)?;
    let vintage = vintage_from_rank(rank)?;

    Ok(Measurement {
        at,
        region,
        intensity,
        methodology: Methodology::new(methodology_id, methodology_version.max(0) as u32),
        vintage,
        mix: read_mix(row)?,
    })
}

/// Intensité depuis une valeur de base, ou erreur si hors domaine.
fn intensity(value: f64) -> Result<CarbonIntensity, RepositoryError> {
    CarbonIntensity::new(value)
        .ok_or_else(|| backend(format!("intensité hors domaine en base : {value}")))
}

/// Construit les statistiques depuis des agrégats SQL.
pub(crate) fn intensity_stats(
    avg: f64,
    min: f64,
    max: f64,
    count: i64,
) -> Result<IntensityStats, RepositoryError> {
    Ok(IntensityStats {
        average: intensity(avg)?,
        min: intensity(min)?,
        max: intensity(max)?,
        count: count.max(0) as u64,
    })
}

/// Mappe une ligne de rollup (`bucket`, agrégats) vers un [`RollupBucket`].
pub(crate) fn rollup_row(row: &PgRow) -> Result<RollupBucket, RepositoryError> {
    let start: OffsetDateTime = row.try_get("bucket").map_err(backend_from)?;
    let avg: f64 = row.try_get("avg_intensity").map_err(backend_from)?;
    let min: f64 = row.try_get("min_intensity").map_err(backend_from)?;
    let max: f64 = row.try_get("max_intensity").map_err(backend_from)?;
    let count: i64 = row.try_get("n").map_err(backend_from)?;
    Ok(RollupBucket {
        start,
        stats: intensity_stats(avg, min, max, count)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vintage_rank_roundtrips() {
        for v in [Vintage::Tr, Vintage::Consolidated, Vintage::Definitive] {
            assert_eq!(vintage_from_rank(vintage_rank(v)).unwrap(), v);
        }
    }

    #[test]
    fn ranks_preserve_quality_ordering() {
        assert!(vintage_rank(Vintage::Definitive) > vintage_rank(Vintage::Consolidated));
        assert!(vintage_rank(Vintage::Consolidated) > vintage_rank(Vintage::Tr));
    }

    #[test]
    fn unknown_rank_is_rejected() {
        assert!(vintage_from_rank(7).is_err());
    }

    #[test]
    fn dedup_keeps_best_vintage_per_key() {
        use time::OffsetDateTime;

        let at = OffsetDateTime::UNIX_EPOCH;
        let make = |g: f64, vintage: Vintage| Measurement {
            at,
            region: Region::National,
            intensity: CarbonIntensity::new(g).unwrap(),
            methodology: Methodology::rte_direct(),
            vintage,
            mix: None,
        };
        // Même clé (region, at, methodology) → un seul survivant, le meilleur.
        let input = [make(50.0, Vintage::Tr), make(40.0, Vintage::Consolidated)];
        let kept = dedup_by_key(&input);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].vintage, Vintage::Consolidated);

        // Clés distinctes (méthodologies différentes) → tout est conservé.
        let other = Measurement {
            methodology: Methodology::new("acv-ademe", 1),
            ..make(10.0, Vintage::Tr)
        };
        let input = [make(50.0, Vintage::Tr), other];
        assert_eq!(dedup_by_key(&input).len(), 2);
    }

    #[test]
    fn mix_field_is_none_without_mix() {
        assert_eq!(mix_field(&None, |m| m.nucleaire), None);
        let mix = GenerationMix {
            nucleaire: 100.0,
            gaz: 0.0,
            charbon: 0.0,
            fioul: 0.0,
            hydraulique: 0.0,
            eolien: 0.0,
            solaire: 0.0,
            bioenergies: 0.0,
            pompage: 0.0,
            echanges: 0.0,
            thermique: None,
        };
        assert_eq!(mix_field(&Some(mix), |m| m.nucleaire), Some(100.0));
    }
}
