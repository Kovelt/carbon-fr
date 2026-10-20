//! Représentation **CSV** de l'historique d'intensité (`GET /v1/intensity/date`,
//! PROD-API-3, plan I9) : choix du format de la réponse et écriture du corps
//! **en flux**.
//!
//! # Choix du format
//!
//! Dans l'ordre : le paramètre `?format=` (`json` ou `csv` ; tout autre texte
//! → `400`), puis, sans lui, l'en-tête `Accept` ; sans l'un ni l'autre, JSON —
//! le défaut historique, inchangé. [`accepts_csv`] lit `Accept` selon la RFC 9110
//! (§12.5.1), en version volontairement réduite : `text/csv` doit être **cité
//! explicitement** avec un poids `q > 0` (un `*/*` ou un `text/*` seuls ne le
//! demandent pas : navigateurs, `curl` et clients HTTP génériques gardent le
//! JSON), et il l'emporte sur `application/json` (ou, à défaut, sur
//! `application/*` / `*/*`) quand son poids est supérieur — ou égal et listé
//! avant.
//!
//! # Format du CSV
//!
//! - `Content-Type: text/csv; charset=utf-8`, servi en pièce jointe
//!   (`Content-Disposition: attachment`, nom de fichier [`filename`]) ;
//! - fin de ligne `\n`, séparateur `,`, une ligne d'en-tête ([`HEADER`]) puis une
//!   ligne par mesure, dans l'ordre du cas d'usage (chronologique) ;
//! - `timestamp` : RFC 3339, le même texte que le JSON (`to_rfc3339`) ;
//!   `intensity` : nombre décimal à point en gCO₂eq/kWh, **sans exposant**
//!   (`Display` d'un `f64` n'en produit jamais), même valeur que le JSON ;
//!   `unit`, `region`, `methodology` : les champs de tête du JSON, répétés sur
//!   chaque ligne ; `vintage` : le millésime du point ;
//! - une plage sans mesure donne l'en-tête seul (le même `200` que le JSON à
//!   `data` vide).
//!
//! **Rien à échapper.** Aucune des valeurs écrites ne peut contenir `,`, `"` ni
//! retour à la ligne : horodatage RFC 3339, nombre fini, unité constante, slug
//! de région, identifiant de méthodologie validé contre une liste fermée et code
//! de millésime sont tous issus d'ensembles fermés ou de formats fixes. Le test
//! `csv_fields_never_need_escaping` garde cet invariant : toute valeur future
//! qui l'enfreindrait doit faire écrire un vrai échappement (RFC 4180) ici, pas
//! y échapper silencieusement.
//!
//! # Flux
//!
//! Le corps est produit paquet par paquet (`ROWS_PER_CHUNK` lignes), à la
//! demande de la connexion : le CSV n'existe jamais en entier en mémoire — seuls
//! les quelques paquets en attente d'écriture y sont — et un client lent
//! ralentit le rendu au lieu de le faire gonfler. La *série* elle-même est déjà
//! en mémoire — le port `IntensityRepository::range` renvoie un `Vec` — : seule
//! l'écriture est paresseuse. Le premier paquet est rendu **avant** de répondre,
//! pour qu'une erreur de formatage systématique donne encore un vrai `500`
//! Problem Details plutôt qu'un `200` interrompu ; une erreur plus tardive ne
//! peut, par nature, que couper la connexion (statut déjà envoyé) et est
//! journalisée.

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use carbonfr_core::domain::Measurement;
use time::{OffsetDateTime, UtcOffset};

use crate::dto::{INTENSITY_UNIT, to_rfc3339};
use crate::error::ApiError;

/// Ligne d'en-tête du CSV : les noms des champs du JSON.
const HEADER: &str = "timestamp,intensity,unit,region,methodology,vintage\n";

/// Nombre de lignes de mesure rendues par paquet de flux. Une ligne fait de ~60 à
/// ~100 octets : un paquet pèse donc de 15 à 26 Kio — assez pour amortir le coût
/// de chaque paquet (un en-tête de chunk HTTP/1.1, un tour de boucle de
/// connexion), assez petit pour que la mémoire ajoutée reste négligeable. Une
/// année au pas de 15 minutes (≈ 35 000 lignes) tient en ~140 paquets.
const ROWS_PER_CHUNK: usize = 256;

/// Format de représentation d'une réponse d'historique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponseFormat {
    Json,
    Csv,
}

/// Résout le format de la réponse : `?format=` d'abord, puis `Accept`, puis
/// JSON. `400` (Problem Details `bad_request`) si `format` n'est ni `json` ni
/// `csv` — valeurs sensibles à la casse, comme `region` ou `interval`.
pub(crate) fn negotiate_format(
    format: Option<&str>,
    headers: &HeaderMap,
) -> Result<ResponseFormat, ApiError> {
    match format {
        Some("json") => Ok(ResponseFormat::Json),
        Some("csv") => Ok(ResponseFormat::Csv),
        Some(_) => Err(ApiError::bad_request(
            "`format` doit valoir `json` ou `csv`",
        )),
        None if accepts_csv(headers) => Ok(ResponseFormat::Csv),
        None => Ok(ResponseFormat::Json),
    }
}

/// `true` si l'en-tête `Accept` demande le CSV plutôt que le JSON (règle
/// détaillée dans la documentation du module).
///
/// Les plages de médias de toutes les lignes `Accept` sont lues comme une seule
/// liste, dans l'ordre. Pour le JSON, c'est la plage **la plus spécifique** qui
/// compte (`application/json`, puis `application/*`, puis `*/*`).
fn accepts_csv(headers: &HeaderMap) -> bool {
    // (poids en millièmes, rang d'apparition) de `text/csv`, puis des trois
    // plages qui couvrent le JSON, de la plus spécifique à la plus générale.
    let mut csv: Option<(u16, usize)> = None;
    let mut json: [Option<(u16, usize)>; 3] = [None; 3];

    let mut rank = 0;
    for value in headers.get_all(header::ACCEPT) {
        // Une valeur non ASCII n'est pas une plage lisible : ignorée.
        let Ok(value) = value.to_str() else { continue };
        for range in value.split(',') {
            let mut parts = range.split(';');
            let media = parts.next().unwrap_or_default().trim();
            let entry = Some((weight_millis(parts), rank));
            rank += 1;
            if media.eq_ignore_ascii_case("text/csv") {
                csv = entry;
            } else if media.eq_ignore_ascii_case("application/json") {
                json[0] = entry;
            } else if media.eq_ignore_ascii_case("application/*") {
                json[1] = entry;
            } else if media == "*/*" {
                json[2] = entry;
            }
        }
    }

    let Some((csv_weight, csv_rank)) = csv.filter(|&(weight, _)| weight > 0) else {
        return false;
    };
    match json.into_iter().flatten().next() {
        // Aucune plage ne couvre le JSON : le CSV est la seule option demandée.
        None => true,
        Some((json_weight, json_rank)) => {
            csv_weight > json_weight || (csv_weight == json_weight && csv_rank < json_rank)
        }
    }
}

/// Poids `q` d'une plage de médias, en millièmes (RFC 9110 §12.4.2) : `q=0.8`
/// vaut 800, l'absence de `q` vaut 1000. Un `q` illisible ou non fini est
/// ignoré (poids par défaut) ; hors de `[0, 1]`, il est ramené dans l'intervalle.
fn weight_millis<'a>(params: impl Iterator<Item = &'a str>) -> u16 {
    params
        .filter_map(|param| param.split_once('='))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("q"))
        .and_then(|(_, value)| value.trim().parse::<f32>().ok())
        .filter(|q| q.is_finite())
        .map_or(1000, |q| (q.clamp(0.0, 1.0) * 1000.0).round() as u16)
}

/// Réponse CSV complète de l'historique : en-têtes (`Content-Type`,
/// `Content-Disposition`, `Vary: Accept`) et corps en flux.
///
/// Les `Cache-Control` / CORS sont posés plus haut par les couches du routeur,
/// comme pour le JSON.
pub(crate) fn csv_response(
    region: &str,
    methodology: &str,
    from: OffsetDateTime,
    to: OffsetDateTime,
    measurements: Vec<Measurement>,
) -> Result<Response, ApiError> {
    let body = csv_body(region, methodology, measurements)?;
    // Nom de fichier ASCII sans caractère à protéger (slug, chiffres, `T`, `Z`) :
    // les guillemets suffisent, pas besoin du `filename*` de la RFC 6266.
    let disposition = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        filename(region, from, to)
    ))
    .map_err(|_| ApiError::internal())?;
    Ok((
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/csv; charset=utf-8"),
            ),
            (header::CONTENT_DISPOSITION, disposition),
            // Le format dépend de `Accept` quand `?format=` est absent : un cache
            // partagé (la réponse porte `Cache-Control: public`) ne doit pas
            // servir le CSV à qui demande le JSON, ni l'inverse.
            (header::VARY, HeaderValue::from_static("Accept")),
        ],
        body,
    )
        .into_response())
}

/// Nom du fichier proposé au téléchargement :
/// `carbon-fr_intensity_<région>_<from>_<to>.csv`, bornes en UTC au format
/// ISO 8601 « basique » (`20260101T000000Z`) — ni `:` ni `+`, sûrs dans un nom
/// de fichier sur tous les systèmes.
fn filename(region: &str, from: OffsetDateTime, to: OffsetDateTime) -> String {
    format!(
        "carbon-fr_intensity_{region}_{}_{}.csv",
        compact_utc(from),
        compact_utc(to)
    )
}

/// `2026-01-01T02:00:00+02:00` → `20260101T000000Z` (secondes entières).
fn compact_utc(at: OffsetDateTime) -> String {
    let at = at.to_offset(UtcOffset::UTC);
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second()
    )
}

/// Corps CSV en flux : l'en-tête, puis les mesures par paquets de
/// `ROWS_PER_CHUNK` lignes, rendus à la demande.
///
/// Échoue (`Format`) si le **premier** paquet n'est pas formatable ; une erreur
/// dans un paquet suivant interrompt le flux (cf. documentation du module).
fn csv_body(
    region: &str,
    methodology: &str,
    measurements: Vec<Measurement>,
) -> Result<Body, time::error::Format> {
    let region = region.to_owned();
    let methodology = methodology.to_owned();
    let mut rest = measurements.into_iter();
    let mut failed = false;
    let mut chunks = std::iter::from_fn(move || {
        // Après une erreur, le flux est terminé : pas de reprise au paquet suivant.
        if failed {
            return None;
        }
        let chunk = render_rows(&region, &methodology, rest.by_ref().take(ROWS_PER_CHUNK))
            .inspect_err(|error| {
                tracing::error!(%error, "export CSV : horodatage non formatable");
            })
            .transpose();
        failed = matches!(chunk, Some(Err(_)));
        chunk
    });

    // Premier paquet rendu tout de suite : une erreur systématique de formatage
    // reste un `500` propre (le statut n'est pas encore parti).
    let first = chunks.next().transpose()?;

    let stream = futures_util::stream::iter(
        std::iter::once(Ok::<_, time::error::Format>(Bytes::from_static(
            HEADER.as_bytes(),
        )))
        .chain(first.map(Ok))
        .chain(chunks),
    );
    Ok(Body::from_stream(stream))
}

/// Écrit les lignes de `rows` dans un paquet. `None` si `rows` est vide (fin du
/// flux).
fn render_rows(
    region: &str,
    methodology: &str,
    rows: impl Iterator<Item = Measurement>,
) -> Result<Option<Bytes>, time::error::Format> {
    use std::fmt::Write as _;

    let mut out = String::new();
    for m in rows {
        let at = to_rfc3339(m.at)?;
        // Écrire dans un `String` ne peut pas échouer.
        let _ = writeln!(
            out,
            "{at},{},{INTENSITY_UNIT},{region},{methodology},{}",
            m.intensity.value(),
            m.vintage.code()
        );
    }
    Ok((!out.is_empty()).then(|| Bytes::from(out)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use carbonfr_core::domain::{CarbonIntensity, Methodology, Region, Vintage};
    use futures_util::StreamExt;
    use time::{Date, Month, Time};

    fn accept(lines: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for line in lines {
            headers.append(header::ACCEPT, HeaderValue::from_str(line).unwrap());
        }
        headers
    }

    fn measurement(at: OffsetDateTime, intensity: f64) -> Measurement {
        Measurement {
            at,
            region: Region::National,
            intensity: CarbonIntensity::new(intensity).unwrap(),
            methodology: Methodology::rte_direct(),
            vintage: Vintage::Definitive,
            mix: None,
        }
    }

    /// Lit un corps de bout en bout, paquet par paquet.
    async fn chunks_of(body: Body) -> Vec<Result<Bytes, axum::Error>> {
        body.into_data_stream().collect().await
    }

    /// `text/csv` doit être cité explicitement, avec un poids non nul, et ne
    /// détrône le JSON que s'il pèse plus — ou autant et passe avant.
    #[test]
    fn accept_header_negotiation() {
        // (en-tête Accept, CSV attendu)
        let cases: &[(&[&str], bool)] = &[
            // Pas d'en-tête, jokers, JSON explicite : le JSON historique.
            (&[], false),
            (&["*/*"], false),
            (&["text/*"], false),
            (&["application/json"], false),
            (
                &["text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"],
                false,
            ),
            (&["application/json, text/plain, */*"], false),
            // CSV explicite.
            (&["text/csv"], true),
            (&["TEXT/CSV"], true),
            (&["  text/csv  "], true),
            (&["text/csv; charset=utf-8"], true),
            (&["text/csv;q=0.9;charset=utf-8"], true),
            // Poids : refus explicite, poids inférieur au JSON, supérieur au JSON.
            (&["text/csv;q=0"], false),
            (&["text/csv;q=0, application/json"], false),
            (&["text/csv;q=0.5, application/json"], false),
            (&["text/csv, application/json;q=0.5"], true),
            (&["text/csv;q=0.8, */*;q=0.5"], true),
            (&["application/json;q=0, text/csv"], true),
            // Égalité de poids : l'ordre de la liste départage ; face à un joker,
            // la citation explicite du CSV compte.
            (&["text/csv, application/json"], true),
            (&["application/json, text/csv"], false),
            (&["text/csv, */*"], true),
            (&["*/*, text/csv"], false),
            (&["text/csv, application/*"], true),
            // La plage JSON la plus spécifique l'emporte sur les jokers.
            (&["application/json;q=0.1, */*, text/csv;q=0.5"], true),
            // Plusieurs lignes `Accept` = une seule liste.
            (&["application/json", "text/csv"], false),
            (&["text/csv", "application/json"], true),
            // Poids illisible ou hors bornes : valeur par défaut / ramené.
            (&["text/csv;q=abc, application/json;q=0.5"], true),
            (&["text/csv;q=NaN, application/json;q=0.5"], true),
            (&["text/csv;q=7, application/json;q=0.5"], true),
            (&["text/csv;q=-3"], false),
        ];
        for (lines, expected) in cases {
            assert_eq!(accepts_csv(&accept(lines)), *expected, "Accept = {lines:?}");
        }
        // Une valeur non ASCII est ignorée, sans paniquer.
        let mut headers = HeaderMap::new();
        headers.append(
            header::ACCEPT,
            HeaderValue::from_bytes(b"text/csv\xff").unwrap(),
        );
        assert!(!accepts_csv(&headers));
    }

    /// `?format=` prime sur `Accept` ; seules les valeurs `json` et `csv` sont
    /// admises (sensibles à la casse).
    #[test]
    fn query_parameter_beats_accept_header() {
        let csv = accept(&["text/csv"]);
        let json = accept(&["application/json"]);
        let none = HeaderMap::new();

        // `ApiError` n'est pas `Debug` : on compare des `Option`.
        let negotiate = |format, headers: &HeaderMap| negotiate_format(format, headers).ok();

        assert_eq!(negotiate(None, &none), Some(ResponseFormat::Json));
        assert_eq!(negotiate(None, &csv), Some(ResponseFormat::Csv));
        assert_eq!(negotiate(None, &json), Some(ResponseFormat::Json));
        assert_eq!(negotiate(Some("csv"), &none), Some(ResponseFormat::Csv));
        assert_eq!(negotiate(Some("csv"), &json), Some(ResponseFormat::Csv));
        assert_eq!(negotiate(Some("json"), &csv), Some(ResponseFormat::Json));

        for invalid in ["xml", "", "CSV", "Json", "csv ", "text/csv"] {
            assert_eq!(
                negotiate(Some(invalid), &csv),
                None,
                "format = {invalid:?} doit être refusé, même avec `Accept: text/csv`"
            );
        }
    }

    #[test]
    fn filename_uses_compact_utc_bounds() {
        let from = OffsetDateTime::parse(
            "2026-01-01T00:00:00+01:00",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();
        let to = OffsetDateTime::parse(
            "2026-01-08T12:30:45.5Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();
        // 00:00 à +01:00 = 23:00 la veille en UTC ; secondes fractionnaires ignorées.
        assert_eq!(
            filename("bretagne", from, to),
            "carbon-fr_intensity_bretagne_20251231T230000Z_20260108T123045Z.csv"
        );
        assert!(!filename("national", from, to).contains([':', '+', ' ']));
    }

    /// Les lignes ont la forme exacte du contrat, valeurs comprises.
    #[tokio::test]
    async fn rows_follow_the_documented_layout() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let body = csv_body(
            "national",
            "rte-direct",
            vec![
                measurement(t0, 20.0),
                measurement(t0 + time::Duration::minutes(15), 38.84702345),
            ],
        )
        .unwrap();
        let text: Vec<u8> = chunks_of(body)
            .await
            .into_iter()
            .flat_map(|chunk| chunk.unwrap())
            .collect();
        assert_eq!(
            String::from_utf8(text).unwrap(),
            "timestamp,intensity,unit,region,methodology,vintage\n\
             1970-01-01T00:00:00Z,20,gCO2eq/kWh,national,rte-direct,definitive\n\
             1970-01-01T00:15:00Z,38.84702345,gCO2eq/kWh,national,rte-direct,definitive\n"
        );
    }

    /// Plage vide : l'en-tête seul, en un seul paquet.
    #[tokio::test]
    async fn empty_series_is_header_only() {
        let chunks = chunks_of(csv_body("national", "rte-direct", Vec::new()).unwrap()).await;
        assert_eq!(chunks.len(), 1);
        assert_eq!(&chunks[0].as_ref().unwrap()[..], HEADER.as_bytes(),);
    }

    /// Le corps est vraiment découpé : pas de bloc unique, aucun paquet plus
    /// gros que prévu, et aucune ligne perdue, dupliquée ou coupée aux
    /// frontières de paquets (effectifs autour des multiples de la taille).
    #[tokio::test]
    async fn body_is_streamed_in_bounded_chunks_without_losing_rows() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        for n in [
            1,
            ROWS_PER_CHUNK - 1,
            ROWS_PER_CHUNK,
            ROWS_PER_CHUNK + 1,
            3 * ROWS_PER_CHUNK,
            3 * ROWS_PER_CHUNK + 7,
        ] {
            let series: Vec<Measurement> = (0..n)
                .map(|i| measurement(t0 + time::Duration::minutes(15 * i as i64), i as f64))
                .collect();
            let chunks = chunks_of(csv_body("national", "rte-direct", series).unwrap()).await;

            // En-tête + ceil(n / ROWS_PER_CHUNK) paquets de lignes.
            assert_eq!(chunks.len(), 1 + n.div_ceil(ROWS_PER_CHUNK), "n = {n}");
            let mut lines = 0;
            for chunk in &chunks {
                let chunk = chunk.as_ref().unwrap();
                assert!(
                    chunk.len() < 32 * 1024,
                    "paquet trop gros : {} o",
                    chunk.len()
                );
                assert!(
                    chunk.ends_with(b"\n"),
                    "un paquet doit finir sur une ligne entière"
                );
                lines += chunk.iter().filter(|&&b| b == b'\n').count();
            }
            assert_eq!(lines, 1 + n, "n = {n}");

            // Les valeurs se suivent sans trou ni doublon.
            let text: Vec<u8> = chunks.into_iter().flat_map(|c| c.unwrap()).collect();
            let text = String::from_utf8(text).unwrap();
            for (i, line) in text.lines().skip(1).enumerate() {
                let value = line.split(',').nth(1).unwrap();
                assert_eq!(value, i.to_string(), "n = {n}, ligne {i}");
            }
        }
    }

    /// Horodatage non formatable (année négative, hors RFC 3339) : au premier
    /// paquet, erreur franche avant toute réponse ; plus tard, le flux s'arrête
    /// sur l'erreur et ne reprend pas.
    #[tokio::test]
    async fn unformattable_timestamp_fails_cleanly() {
        let bad = OffsetDateTime::new_utc(
            Date::from_calendar_date(-1, Month::January, 1).unwrap(),
            Time::MIDNIGHT,
        );
        let ok = OffsetDateTime::UNIX_EPOCH;

        // Dans le premier paquet : refusé avant de construire le corps.
        assert!(csv_body("national", "rte-direct", vec![measurement(bad, 1.0)]).is_err());

        // Dans le deuxième paquet : l'en-tête et le premier paquet passent, puis
        // une erreur, puis plus rien (3 paquets de lignes prévus, 1 seul rendu).
        let mut series: Vec<Measurement> = (0..3 * ROWS_PER_CHUNK)
            .map(|_| measurement(ok, 1.0))
            .collect();
        series[ROWS_PER_CHUNK + 3] = measurement(bad, 1.0);
        let chunks = chunks_of(csv_body("national", "rte-direct", series).unwrap()).await;
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].is_ok() && chunks[1].is_ok());
        assert!(chunks[2].is_err());
    }

    /// Invariant du module : aucune valeur écrite n'a besoin d'échappement
    /// (RFC 4180). Les ensembles fermés sont énumérés ; les formats fixes
    /// (horodatage, nombre) sont couverts par leur propre nature.
    #[test]
    fn csv_fields_never_need_escaping() {
        let needs_escaping = |value: &str| value.contains([',', '"', '\n', '\r']);

        for region in std::iter::once(Region::National).chain(Region::METROPOLITAN) {
            assert!(!needs_escaping(region.slug()), "{}", region.slug());
        }
        for vintage in [Vintage::Tr, Vintage::Consolidated, Vintage::Definitive] {
            assert!(!needs_escaping(vintage.code()), "{}", vintage.code());
        }
        for methodology in crate::handlers::KNOWN_METHODOLOGIES {
            assert!(!needs_escaping(methodology), "{methodology}");
        }
        assert!(!needs_escaping(INTENSITY_UNIT));
        // L'en-tête : 6 colonnes, une seule ligne, pas de guillemets.
        assert_eq!(HEADER.trim_end().split(',').count(), 6);
        assert_eq!(HEADER.matches('\n').count(), 1);
        assert!(HEADER.ends_with('\n') && !HEADER.contains(['"', '\r']));
        // Le nombre écrit est toujours une notation décimale simple.
        for value in [0.0, 12.0, 38.84702345, 1e-9, 123_456_789.0, 1e21] {
            let text = value.to_string();
            assert!(
                text.chars().all(|c| c.is_ascii_digit() || c == '.'),
                "{text}"
            );
        }
    }
}
