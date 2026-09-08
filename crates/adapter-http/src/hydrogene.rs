//! Page carte « électrolyseurs × carbone live » (couche B-light, ADR-0025/0029).
//!
//! **Hors contrat `/v1`** (comme `/docs`, `/health`, `/metrics`) : une page
//! HTML **auto-contenue** (zéro CDN, zéro tuile externe — SVG maison) et ses
//! trois jeux de données embarqués au build (`include_str!`, même motif que les
//! fixtures ENTSO-E) :
//!
//! - `sites.json` — électrolyseurs FR/UE (European Hydrogen Observatory,
//!   © Clean Hydrogen JU, reproduction autorisée avec mention de la source),
//!   instantané **semestriel** filtré `Water electrolysis` — la procédure de
//!   rafraîchissement est documentée dans l'ADR-0029 ;
//! - `regions.geojson` — contours des régions (IGN Admin Express, Licence
//!   Ouverte 2.0), simplifiés/allégés ;
//! - `pays.geojson` — contexte pays (Natural Earth, domaine public).
//!
//! La donnée **live** (intensités régionales, fenêtres d'éligibilité, SSE) est
//! consommée par la page via l'API `/v1` en même origine — la page n'ajoute
//! aucune donnée carbone propre, elle ne fait que composer l'existant.
//!
//! Neutralité (ADR-0025/0029) : la page n'affiche **jamais** une éligibilité
//! par site (donnée au niveau site absente) — la couleur carbone est celle du
//! **réseau**, les marqueurs sont un état structurel du parc.

use axum::http::header;
use axum::response::{Html, IntoResponse};

const PAGE: &str = include_str!("../assets/hydrogene/index.html");
const SITES: &str = include_str!("../assets/hydrogene/sites.json");
const REGIONS: &str = include_str!("../assets/hydrogene/regions.geojson");
const PAYS: &str = include_str!("../assets/hydrogene/pays.geojson");

/// Ancêtres d'iframe autorisés pour la page carte : elle-même (`'self'`) et le
/// site vitrine, qui l'embarque en vignette. Posé par l'application (et non par
/// le reverse proxy) car le middleware `secure-headers` de la stack pose un
/// `X-Frame-Options: SAMEORIGIN` global : quand `frame-ancestors` est présent,
/// les navigateurs ignorent `X-Frame-Options` (CSP 2 §8.1) — la dérogation ne
/// vaut donc que pour cette route, `/docs` et le reste gardent SAMEORIGIN.
/// La page est anonyme et sans action utilisateur : élargir ses ancêtres ne
/// crée pas de surface de clickjacking.
const FRAME_ANCESTORS: &str = "frame-ancestors 'self' https://carbon-fr.kovelt.fr";

pub(crate) async fn page() -> impl IntoResponse {
    (
        [(header::CONTENT_SECURITY_POLICY, FRAME_ANCESTORS)],
        Html(PAGE),
    )
}

pub(crate) async fn sites() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        SITES,
    )
}

pub(crate) async fn regions() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/geo+json")], REGIONS)
}

pub(crate) async fn pays() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/geo+json")], PAYS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les trois jeux embarqués sont du JSON valide et portent leur provenance
    /// (l'attribution fait partie du contrat de réutilisation des sources).
    #[test]
    fn embedded_datasets_are_valid_json_with_provenance() {
        let sites: serde_json::Value = serde_json::from_str(SITES).expect("sites.json");
        assert!(
            sites["source"]
                .as_str()
                .unwrap()
                .contains("European Hydrogen Observatory")
        );
        assert!(
            sites["license"]
                .as_str()
                .unwrap()
                .contains("Clean Hydrogen JU")
        );
        assert!(sites["count"].as_u64().unwrap() > 100);
        assert_eq!(
            sites["count"].as_u64().unwrap() as usize,
            sites["sites"].as_array().unwrap().len()
        );
        // Chaque site est géolocalisé et filtré électrolyse (contrat de la carte).
        for s in sites["sites"].as_array().unwrap() {
            assert!(s["lat"].as_f64().is_some() && s["lon"].as_f64().is_some());
            assert!(matches!(
                s["stage"].as_str(),
                Some("Operation") | Some("Construction")
            ));
        }

        let regions: serde_json::Value = serde_json::from_str(REGIONS).expect("regions.geojson");
        assert_eq!(regions["features"].as_array().unwrap().len(), 13);

        let pays: serde_json::Value = serde_json::from_str(PAYS).expect("pays.geojson");
        assert!(pays["features"].as_array().unwrap().len() > 30);
    }

    /// La page est auto-contenue : aucune ressource externe (CDN, tuiles,
    /// polices) — tout est inline ou servi en même origine.
    #[test]
    fn page_is_self_contained() {
        for needle in [
            "src=\"http",
            "href=\"http://",
            "@import",
            "cdn.",
            "unpkg",
            "jsdelivr",
        ] {
            assert!(
                !PAGE.contains(needle),
                "ressource externe interdite dans la page : {needle}"
            );
        }
        // Les liens sortants (attribution) sont permis dans les <a href="https…">.
        assert!(PAGE.contains("naturalearthdata.com"));
        // La neutralité par-site est affirmée dans la page servie.
        assert!(PAGE.contains("n'évalue"));
    }
}
