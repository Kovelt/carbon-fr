//! Test de parité (ADR-0031 décision 2) : une table interne
//! `operationId → méthode SDK` doit couvrir **exactement** les opérations
//! `/v1` du snapshot OpenAPI servi — ni trou (opération sans méthode), ni
//! entrée fantôme (méthode qui prétend couvrir une opération qui n'existe
//! plus).
//!
//! Limite assumée : seul l'`operationId` est vérifié ; le nom de méthode de la
//! table est indicatif. Que chaque méthode appelle bien la bonne route est
//! prouvé par `tests/rest.rs` (un test par opération, méthode/chemin/paramètres
//! vérifiés côté serveur de test).
//!
//! Lit `../adapter-http/tests/openapi.snapshot.json` via `CARGO_MANIFEST_DIR`
//! (chemin **dans le workspace**, pas publié avec `carbonfr-sdk` — cf.
//! `cargo package --list`) : si le fichier est absent (crate empaquetée /
//! téléchargée depuis crates.io, hors du workspace), le test se marque
//! ignoré proprement plutôt que d'échouer.

use std::collections::BTreeSet;

/// `(operationId, méthode SDK)` — une entrée par opération `/v1` couverte
/// par cette version. `intensity_stream` est couvert par
/// `CarbonFr::intensity_stream`, derrière la feature Cargo `stream`
/// (`src/stream.rs`) — listé ici indépendamment de la feature active pour ce
/// run de test, puisque la parité porte sur la *surface*, pas sur les
/// features compilées.
const COVERED: &[(&str, &str)] = &[
    ("cost_reference", "CarbonFr::cost_reference"),
    ("create_webhook", "CarbonFr::create_webhook"),
    ("delete_webhook", "CarbonFr::delete_webhook"),
    ("eligibility_rulesets", "CarbonFr::eligibility_rulesets"),
    ("exchanges", "CarbonFr::exchanges"),
    ("exchanges_date", "CarbonFr::exchanges_history"),
    ("factors", "CarbonFr::factors"),
    ("forecast", "CarbonFr::forecast"),
    ("greenest_window", "CarbonFr::greenest_window"),
    ("intensity_below", "CarbonFr::below"),
    ("intensity_date", "CarbonFr::intensity_date"),
    ("intensity_now", "CarbonFr::intensity_now"),
    ("intensity_stats", "CarbonFr::intensity_stats"),
    (
        "intensity_stream",
        "CarbonFr::intensity_stream (feature `stream`)",
    ),
    ("list_webhooks", "CarbonFr::list_webhooks"),
    ("methodologies", "CarbonFr::methodologies"),
    ("mix", "CarbonFr::mix"),
    ("price", "CarbonFr::price"),
    ("price_date", "CarbonFr::price_history"),
    ("record_visit", "CarbonFr::record_visit"),
    ("renewable", "CarbonFr::renewable"),
    ("schedule", "CarbonFr::schedule"),
    ("schedule_slots", "CarbonFr::schedule_slots"),
    ("visit_stats", "CarbonFr::visit_stats"),
    ("weather", "CarbonFr::weather"),
    ("weather_date", "CarbonFr::weather_history"),
];

/// Hors périmètre v1 par décision explicite (ADR-0031 décision 8) : sondes
/// d'infra, pas du contrat `/v1`.
const EXCLUDED: &[&str] = &["health", "health_ready"];

#[test]
fn every_v1_operation_is_covered_or_explicitly_excluded() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let snapshot_path = format!("{manifest_dir}/../adapter-http/tests/openapi.snapshot.json");

    let raw = match std::fs::read_to_string(&snapshot_path) {
        Ok(raw) => raw,
        Err(_) => {
            eprintln!(
                "snapshot OpenAPI introuvable ({snapshot_path}) — crate probablement empaquetée \
                 hors du workspace : test ignoré proprement plutôt qu'en échec."
            );
            return;
        }
    };
    let snapshot: serde_json::Value = serde_json::from_str(&raw).expect("JSON OpenAPI valide");
    let paths = snapshot["paths"]
        .as_object()
        .expect("`paths` doit être un objet");

    let mut operation_ids = BTreeSet::new();
    for methods in paths.values() {
        let methods = methods.as_object().expect("un chemin liste ses méthodes");
        for operation in methods.values() {
            if let Some(id) = operation.get("operationId").and_then(|v| v.as_str()) {
                operation_ids.insert(id.to_string());
            }
        }
    }
    assert!(
        operation_ids.len() >= 20,
        "snapshot OpenAPI suspicieusement petit ({} opérations) — le fichier lu est-il le bon ?",
        operation_ids.len()
    );

    let covered_ids: BTreeSet<&str> = COVERED.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        covered_ids.len(),
        COVERED.len(),
        "operationId en double dans la table COVERED de parity.rs"
    );

    // (1) Toute opération du snapshot est couverte, ou explicitement exclue.
    let uncovered: Vec<&str> = operation_ids
        .iter()
        .map(String::as_str)
        .filter(|id| !covered_ids.contains(id) && !EXCLUDED.contains(id))
        .collect();
    assert!(
        uncovered.is_empty(),
        "opérations /v1 du snapshot sans méthode SDK ni exclusion documentée dans parity.rs : {uncovered:?}"
    );

    // (2) La table ne référence aucune opération absente du snapshot (dérive inverse :
    //     une opération renommée/supprimée côté serveur doit faire échouer ce test, pas
    //     seulement laisser une entrée orpheline silencieuse).
    let stale: Vec<&str> = covered_ids
        .iter()
        .copied()
        .filter(|id| !operation_ids.contains(*id))
        .collect();
    assert!(
        stale.is_empty(),
        "la table COVERED de parity.rs référence des opérations absentes du snapshot OpenAPI : {stale:?}"
    );

    // (3) Les exclusions elles-mêmes doivent être des opérations réelles (sinon la liste
    //     `EXCLUDED` documente un nom qui n'existe pas / plus).
    let stale_exclusions: Vec<&str> = EXCLUDED
        .iter()
        .copied()
        .filter(|id| !operation_ids.contains(*id))
        .collect();
    assert!(
        stale_exclusions.is_empty(),
        "la liste EXCLUDED de parity.rs référence des opérations absentes du snapshot OpenAPI : {stale_exclusions:?}"
    );
}
