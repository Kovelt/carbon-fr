//! Traduction entre les lignes SQL et les types du domaine.
//!
//! L'encodage de stockage (rang de millésime, colonnes de mix) est une
//! préoccupation d'adapter : il vit ici, jamais dans `core`.

use carbonfr_core::domain::{
    CarbonIntensity, GenerationMix, Measurement, Methodology, Region, Vintage,
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
        };
        assert_eq!(mix_field(&Some(mix), |m| m.nucleaire), Some(100.0));
    }
}
