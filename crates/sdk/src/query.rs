//! Petits utilitaires de construction de query string, partagés par les 25
//! méthodes REST de `crate::methods` — évite de répéter le motif `if let
//! Some(v) = opt { pairs.append_pair(...) }` à chaque paramètre optionnel.

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::CarbonFrError;

/// Formatte un horodatage en RFC 3339 pour la query string. N'échoue en
/// pratique que pour un `OffsetDateTime` hors de la plage représentable par
/// `time` (jamais pour des dates raisonnables) — `Result` tout de même,
/// jamais de panique/`unwrap` (`CONTRIBUTING.md`).
pub(crate) fn format_rfc3339(value: OffsetDateTime) -> Result<String, CarbonFrError> {
    value
        .format(&Rfc3339)
        .map_err(|e| CarbonFrError::Config(format!("horodatage non formattable en RFC 3339 : {e}")))
}

/// Petites méthodes `opt_*` sur un `url::form_urlencoded::Serializer` (le
/// type de `Url::query_pairs_mut()`) : n'ajoute la paire que si la valeur est
/// `Some`, sinon no-op — le paramètre est alors simplement absent de la
/// requête (comportement par défaut du serveur, `resolve_region`/
/// `resolve_methodology`/… côté `crates/adapter-http/src/handlers.rs`).
pub(crate) trait QueryPairsExt {
    fn opt_str(&mut self, key: &str, value: Option<&str>) -> &mut Self;
    fn opt_display<T: std::fmt::Display>(&mut self, key: &str, value: Option<T>) -> &mut Self;
    fn opt_rfc3339(
        &mut self,
        key: &str,
        value: Option<OffsetDateTime>,
    ) -> Result<&mut Self, CarbonFrError>;
}

impl<T: url::form_urlencoded::Target> QueryPairsExt for url::form_urlencoded::Serializer<'_, T> {
    fn opt_str(&mut self, key: &str, value: Option<&str>) -> &mut Self {
        if let Some(v) = value {
            self.append_pair(key, v);
        }
        self
    }

    fn opt_display<T2: std::fmt::Display>(&mut self, key: &str, value: Option<T2>) -> &mut Self {
        if let Some(v) = value {
            self.append_pair(key, &v.to_string());
        }
        self
    }

    fn opt_rfc3339(
        &mut self,
        key: &str,
        value: Option<OffsetDateTime>,
    ) -> Result<&mut Self, CarbonFrError> {
        if let Some(v) = value {
            self.append_pair(key, &format_rfc3339(v)?);
        }
        Ok(self)
    }
}
