//! Webhooks : modèle d'abonnement + **primitives de sécurité pures** (ADR-0016).
//!
//! Tout ce qui est **dangereux** vit ici, pur et testable sans réseau : le
//! déclenchement *edge-triggered*, la validation **anti-SSRF** de l'URL de
//! rappel, et la **signature HMAC-SHA256**. Le `core` ne fait jamais de requête
//! sortante — l'émission est le rôle d'un adapter `Notifier`.

use std::net::IpAddr;

use sha2::{Digest, Sha256};
use url::{Host, Url};

use crate::domain::Region;

/// Sens du franchissement de seuil d'un abonnement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdDirection {
    /// Notifier quand l'intensité **passe sous** le seuil.
    Below,
    /// Notifier quand l'intensité **passe au-dessus** du seuil.
    Above,
}

impl ThresholdDirection {
    pub fn code(self) -> &'static str {
        match self {
            ThresholdDirection::Below => "below",
            ThresholdDirection::Above => "above",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "below" => Some(ThresholdDirection::Below),
            "above" => Some(ThresholdDirection::Above),
            _ => None,
        }
    }

    /// La valeur `v` satisfait-elle la condition (sous/au-dessus du seuil) ?
    fn holds(self, v: f64, threshold: f64) -> bool {
        match self {
            ThresholdDirection::Below => v < threshold,
            ThresholdDirection::Above => v > threshold,
        }
    }
}

/// Abonnement webhook : une condition de seuil sur une région, possédée par une
/// clé API (ADR-0016 §1). Le `secret` sert à signer les livraisons ; il n'est
/// jamais ré-exposé après création.
#[derive(Debug, Clone, PartialEq)]
pub struct Subscription {
    pub id: String,
    /// Empreinte de la clé propriétaire (jamais la clé en clair).
    pub owner_key_hash: String,
    pub region: Region,
    pub threshold: f64,
    pub direction: ThresholdDirection,
    pub callback_url: String,
    pub secret: String,
}

/// Faut-il notifier ? **Edge-triggered** (ADR-0016 §2) : on ne déclenche qu'au
/// **franchissement** — la condition devient vraie alors qu'elle était fausse
/// au pas précédent. Sans état précédent connu (`previous = None`), on **ne
/// déclenche pas** (on attend une vraie transition — pas de spam au démarrage).
pub fn should_fire(
    direction: ThresholdDirection,
    threshold: f64,
    previous: Option<f64>,
    current: f64,
) -> bool {
    match previous {
        Some(prev) => !direction.holds(prev, threshold) && direction.holds(current, threshold),
        None => false,
    }
}

/// Raison de rejet d'une URL de rappel (ADR-0016 §3).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebhookUrlError {
    #[error("schéma non autorisé (HTTPS requis)")]
    NotHttps,
    #[error("URL malformée")]
    Malformed,
    #[error("hôte interdit (privé/loopback/local)")]
    ForbiddenHost,
}

/// Valide une URL de rappel **anti-SSRF** (ADR-0016 §3). Pure : vérifie le schéma
/// HTTPS, l'absence d'*userinfo*, et — pour un hôte **littéral IP** — qu'il n'est
/// pas privé/loopback/link-local/réservé. La **re-validation à la résolution
/// DNS** (TOCTOU) est faite par l'adapter de livraison au moment de l'appel.
///
/// L'analyse de l'hôte passe par [`url::Url`] — **le même analyseur (WHATWG) que
/// reqwest**. C'est essentiel : un `str::parse::<IpAddr>()` ne reconnaît que la
/// forme décimale pointée, alors que reqwest/`url` normalisent aussi les formes
/// décimale entière (`2130706433`), octale (`0177…`), hexadécimale (`0x7f…`) et
/// courte (`127.1`) en `Host::Ipv4`. Un tel hôte est ensuite contacté par reqwest
/// **sans passer par le resolver anti-SSRF** (hyper ne résout pas un littéral IP).
/// Valider avec un analyseur différent laissait donc un contournement total
/// (audit 2026-07, F01). On inspecte ici exactement l'hôte que le client joindra.
pub fn validate_webhook_url(url: &str) -> Result<(), WebhookUrlError> {
    let parsed = Url::parse(url).map_err(|_| WebhookUrlError::Malformed)?;
    if parsed.scheme() != "https" {
        return Err(WebhookUrlError::NotHttps);
    }
    // Pas d'userinfo (`user:pass@host`) : vecteur d'ambiguïté/contournement.
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(WebhookUrlError::ForbiddenHost);
    }
    match parsed.host() {
        None => Err(WebhookUrlError::Malformed),
        // Littéraux IP (toutes formes reconnues par WHATWG) → plages interdites.
        Some(Host::Ipv4(v4)) if !is_public_ip(IpAddr::V4(v4)) => {
            Err(WebhookUrlError::ForbiddenHost)
        }
        Some(Host::Ipv6(v6)) if !is_public_ip(IpAddr::V6(v6)) => {
            Err(WebhookUrlError::ForbiddenHost)
        }
        // Nom d'hôte → localhost refusé ici ; les autres noms sont revalidés à la
        // résolution DNS par l'adapter de livraison (anti-rebinding).
        Some(Host::Domain(domain)) => {
            let lower = domain.to_ascii_lowercase();
            if lower == "localhost" || lower.ends_with(".localhost") {
                Err(WebhookUrlError::ForbiddenHost)
            } else {
                Ok(())
            }
        }
        // IP publiques (v4/v6) : acceptées.
        Some(_) => Ok(()),
    }
}

/// Hôte (sans port) d'une URL `https://…`, pour la re-résolution DNS côté
/// adapter de livraison. `None` si l'URL n'est pas exploitable.
pub fn webhook_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return None;
    }
    let host = if let Some(end) = authority.strip_prefix('[') {
        end.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    (!host.is_empty()).then(|| host.to_string())
}

/// `true` si l'IP est **publiquement routable** (ni privée, ni loopback, ni
/// link-local, ni réservée). Conservateur : tout ce qui n'est pas clairement
/// public est refusé.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                // 0.0.0.0/8 (« ce réseau » ; 0.x souvent routé vers localhost).
                || o[0] == 0
                // 100.64.0.0/10 (CGNAT) ; 169.254 déjà couvert par link_local.
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                // 192.0.0.0/24 IETF, 198.18.0.0/15 benchmarking.
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (18..=19).contains(&o[1]))
                // 240.0.0.0/4 réservé (classe E ; 255.255.255.255 déjà broadcast).
                || o[0] >= 240)
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // fc00::/7 unique-local.
                || (s[0] & 0xfe00) == 0xfc00
                // fe80::/10 link-local.
                || (s[0] & 0xffc0) == 0xfe80
                // ::ffff:0:0/96 IPv4-mapped : refusé (peut masquer une IP privée).
                || v6.to_ipv4_mapped().is_some()
                // 2002::/16 (6to4) et 2001::/32 (Teredo) : encapsulent une IPv4,
                // potentiellement privée → refusés.
                || s[0] == 0x2002
                || (s[0] == 0x2001 && s[1] == 0x0000)
                // 64:ff9b::/96 (NAT64 well-known) : traduit vers une IPv4 arbitraire.
                || (s[0] == 0x0064 && s[1] == 0xff9b && s[2] == 0 && s[3] == 0))
        }
    }
}

/// Échappe une chaîne pour insertion dans une valeur JSON (guillemets, backslash,
/// contrôles). Les champs du payload sont contrôlés (slug, hex, RFC 3339), mais on
/// échappe par principe pour que le **contrat** reste sûr si un champ libre entrait.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Corps JSON d'une notification de webhook (ADR-0016). **Contrat pur et
/// testable** : c'est la forme du payload livré, centralisée ici plutôt que
/// construite à la main dans la composition root. `timestamp` est fourni déjà
/// formaté (RFC 3339) par l'appelant.
pub fn render_webhook_payload(
    subscription: &Subscription,
    timestamp: &str,
    intensity: f64,
) -> String {
    format!(
        "{{\"subscription_id\":\"{}\",\"region\":\"{}\",\"timestamp\":\"{}\",\
         \"intensity\":{},\"threshold\":{},\"direction\":\"{}\",\"unit\":\"gCO2eq/kWh\"}}",
        json_escape(&subscription.id),
        subscription.region.slug(),
        json_escape(timestamp),
        intensity,
        subscription.threshold,
        subscription.direction.code(),
    )
}

/// HMAC-SHA256 (RFC 2104) en hexadécimal — tout-Rust sur `sha2`, sans dépendance
/// supplémentaire (ADR-0016 §4). Signe le corps d'une livraison avec le secret
/// de l'abonnement.
pub fn hmac_sha256_hex(secret: &[u8], message: &[u8]) -> String {
    const BLOCK: usize = 64;
    // Clé normalisée à la taille de bloc (hachée si trop longue, complétée de 0).
    let mut key = if secret.len() > BLOCK {
        Sha256::digest(secret).to_vec()
    } else {
        secret.to_vec()
    };
    key.resize(BLOCK, 0);

    let mut inner = Sha256::new();
    inner.update(key.iter().map(|b| b ^ 0x36).collect::<Vec<u8>>());
    inner.update(message);
    let inner = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(key.iter().map(|b| b ^ 0x5c).collect::<Vec<u8>>());
    outer.update(inner);
    let outer = outer.finalize();

    outer.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_trigger_fires_only_on_crossing() {
        // Below 50 : franchit quand on passe de ≥50 à <50.
        assert!(should_fire(
            ThresholdDirection::Below,
            50.0,
            Some(60.0),
            40.0
        ));
        // Reste sous le seuil : pas de re-déclenchement.
        assert!(!should_fire(
            ThresholdDirection::Below,
            50.0,
            Some(40.0),
            30.0
        ));
        // Au-dessus : pas de déclenchement.
        assert!(!should_fire(
            ThresholdDirection::Below,
            50.0,
            Some(60.0),
            55.0
        ));
        // Sans état précédent : on attend une transition.
        assert!(!should_fire(ThresholdDirection::Below, 50.0, None, 40.0));
        // Above 80 : franchit quand on passe de ≤80 à >80.
        assert!(should_fire(
            ThresholdDirection::Above,
            80.0,
            Some(70.0),
            90.0
        ));
        assert!(!should_fire(
            ThresholdDirection::Above,
            80.0,
            Some(90.0),
            95.0
        ));
    }

    #[test]
    fn ssrf_rejects_dangerous_urls() {
        // HTTPS requis.
        assert_eq!(
            validate_webhook_url("http://example.com/hook"),
            Err(WebhookUrlError::NotHttps)
        );
        // localhost / loopback / privé / link-local / userinfo.
        for bad in [
            "https://localhost/hook",
            "https://127.0.0.1/hook",
            "https://10.0.0.5/hook",
            "https://192.168.1.10/hook",
            "https://172.16.3.4/hook",
            "https://169.254.169.254/latest/meta-data",
            "https://[::1]/hook",
            "https://[fe80::1]/hook",
            "https://user:pass@example.com/hook",
            "https://100.64.0.1/hook",
            // Plages complétées (P1 audit) :
            "https://0.0.0.0/hook",
            "https://0.1.2.3/hook",   // 0.0.0.0/8
            "https://240.0.0.1/hook", // 240/4 réservé
            "https://255.255.255.255/hook",
            "https://[2002:a00:1::1]/hook", // 6to4 encapsulant 10.0.0.1
            "https://[64:ff9b::a00:1]/hook", // NAT64 vers 10.0.0.1
            "https://[::ffff:127.0.0.1]/hook", // IPv4-mapped loopback
            // Encodages IP alternatifs de 127.0.0.1, normalisés en IP par reqwest
            // mais que `str::parse::<IpAddr>()` ne reconnaissait pas (F01 audit) :
            "https://2130706433/hook",   // décimal entier
            "https://0x7f.0.0.1/hook",   // hexadécimal
            "https://017700000001/hook", // octal
            "https://127.1/hook",        // forme courte
            // Décimal entier de 169.254.169.254 (métadonnées cloud, VPS Hetzner).
            "https://2852039166/latest/meta-data",
            // 192.0.0.0/24 IETF : tout le /24 doit être refusé, pas seulement .0 (F23).
            "https://192.0.0.9/hook",
        ] {
            assert_eq!(
                validate_webhook_url(bad),
                Err(WebhookUrlError::ForbiddenHost),
                "devrait rejeter {bad}"
            );
        }
        // URL publique légitime : acceptée.
        assert!(validate_webhook_url("https://hooks.example.com/carbon").is_ok());
        assert!(validate_webhook_url("https://8.8.8.8/hook").is_ok());
    }

    #[test]
    fn payload_is_well_formed_and_escaped() {
        let sub = Subscription {
            id: "ab\"cd".to_string(), // caractère à échapper
            owner_key_hash: "h".to_string(),
            region: Region::National,
            threshold: 50.0,
            direction: ThresholdDirection::Below,
            callback_url: "https://x".to_string(),
            secret: "s".to_string(),
        };
        let body = render_webhook_payload(&sub, "2024-01-01T00:00:00Z", 42.5);
        assert!(body.contains("\\\"cd")); // guillemet échappé
        assert!(body.contains("\"region\":\"national\""));
        assert!(body.contains("\"intensity\":42.5"));
        assert!(body.contains("\"direction\":\"below\""));
        assert!(body.contains("\"unit\":\"gCO2eq/kWh\""));
    }

    #[test]
    fn hmac_matches_known_vector() {
        // RFC 4231 cas de test 1 : clé = 20×0x0b, message = "Hi There".
        let key = [0x0b_u8; 20];
        let got = hmac_sha256_hex(&key, b"Hi There");
        assert_eq!(
            got,
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn hmac_with_long_key() {
        // RFC 4231 cas 6 : clé de 131 octets 0xaa (hachée car > bloc).
        let key = [0xaa_u8; 131];
        let got = hmac_sha256_hex(
            &key,
            b"Test Using Larger Than Block-Size Key - Hash Key First",
        );
        assert_eq!(
            got,
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }
}
