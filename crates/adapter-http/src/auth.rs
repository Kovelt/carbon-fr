//! Middleware de **bord** : authentification par clé API + application de quota
//! (ADR-0015).
//!
//! Préoccupation d'**infrastructure entrante**, jamais du domaine : le `core` ne
//! prend aucun principal, les cas d'usage sont inchangés. Le middleware résout un
//! **principal** (anonyme par IP, ou clé authentifiée via [`ApiKeyRepository`]),
//! applique un quota par fenêtre, et laisse passer la requête telle quelle.
//!
//! **Opt-in** : il n'est appliqué que si la composition root le câble. Par défaut
//! (self-hosting), l'API reste **anonyme et sans limite** — parité OSS (§6).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use carbonfr_core::ports::{ApiKeyRecord, ApiKeyRepository, ApiTier};
use sha2::{Digest, Sha256};

use crate::error::problem_response;

/// Limites de débit par niveau (requêtes par minute), ADR-0015 §5.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Limite des appelants **anonymes** (par IP).
    pub anonymous_per_min: u32,
    /// Limite des appelants à **clé gratuite**.
    pub free_per_min: u32,
    /// Faire confiance à `X-Forwarded-For` pour identifier l'IP anonyme. Faux par
    /// défaut (l'en-tête est spoofable hors d'un proxy de confiance) → sans proxy,
    /// l'anonyme tombe dans un seau unique plutôt qu'un quota par IP contournable.
    pub trust_proxy: bool,
    /// En-tête d'IP réelle **posé et écrasé par le proxy** (ex. `x-real-ip`),
    /// opt-in via `CARBONFR_REAL_IP_HEADER`. `None` (défaut) : dernier segment de
    /// `X-Forwarded-For`, sûr par construction avec tout proxy qui **ajoute**
    /// l'IP réelle à droite (Caddy, Traefik, nginx avec
    /// `$proxy_add_x_forwarded_for`). Ne configurer un en-tête dédié que si le
    /// proxy l'écrase systématiquement : l'ancienne priorité inconditionnelle à
    /// `X-Real-Ip` rendait le quota contournable derrière un proxy qui relaie
    /// l'en-tête client tel quel (audit 2026-08).
    pub real_ip_header: Option<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            anonymous_per_min: 60,
            free_per_min: 600,
            trust_proxy: false,
            real_ip_header: None,
        }
    }
}

/// Appelant résolu. L'**anonyme** n'est pas un tier : c'est l'absence de clé.
enum Principal {
    Anonymous { ip: String },
    Keyed { id: String, tier: ApiTier },
}

/// État du middleware : registre de clés + compteur de quota en mémoire
/// + caches de résolution (négatif et positif).
#[derive(Clone)]
pub struct AuthState {
    keys: Arc<dyn ApiKeyRepository>,
    limiter: Arc<Mutex<RateLimiter>>,
    negative: Arc<Mutex<NegativeKeyCache>>,
    positive: Arc<Mutex<PositiveKeyCache>>,
    config: AuthConfig,
}

impl AuthState {
    pub fn new(keys: Arc<dyn ApiKeyRepository>, config: AuthConfig) -> Self {
        Self {
            keys,
            limiter: Arc::new(Mutex::new(RateLimiter::default())),
            negative: Arc::new(Mutex::new(NegativeKeyCache::default())),
            positive: Arc::new(Mutex::new(PositiveKeyCache::default())),
            config,
        }
    }
}

/// Compteur de quota **fenêtre fixe par minute**, en mémoire (suffisant pour une
/// instance unique, ADR-0007 ; réversible derrière un futur `UsageMeter`).
///
/// ⚠️ **Fenêtre fixe, pas glissante** (audit F20) : le compteur repart à zéro au
/// changement de minute (`entry.0 != minute`) au lieu de glisser en continu. À la
/// frontière (requêtes à 59,9 s puis à 60,1 s), un appelant peut donc passer
/// jusqu'à `2 × limite` en ~1-2 s. **Trade-off assumé, pas un bug** : ce quota est
/// une garde de dégradation anti-abus (protéger l'instance et le quota amont
/// RTE/ENTSO-E), pas un SLA de facturation strict — le comptage exact est le rôle
/// du futur `UsageMeter` persistant (ADR-0015, tier payant). Si un SLA strict
/// devenait nécessaire ici, remplacer par une fenêtre glissante (log
/// d'horodatages par `id`) ou un token-bucket, sans changer la signature de
/// `check` ni le comportement de `enforce()` en aval.
#[derive(Default)]
struct RateLimiter {
    windows: HashMap<String, (i64, u32)>,
    /// Dernière minute où la purge a tourné. Le `retain` O(n) ne s'exécute
    /// qu'**une fois par changement de minute** (audit 2026-08) : l'ancienne
    /// purge « si `len` > seuil » ne retirait rien pendant une inondation
    /// d'identifiants distincts (toutes les entrées étaient de la minute
    /// courante) et re-scannait toute la carte sous le mutex à CHAQUE requête.
    last_purge_minute: i64,
}

/// Plafond dur d'identifiants suivis par le limiteur. Au-delà, les identifiants
/// **inédits** partagent un seau de débordement unique (audit 2026-08) : la
/// carte reste bornée même sous une rotation d'adresses (ex. /64 IPv6), au prix
/// d'un quota collectif dégradé pour les nouveaux venus de la minute —
/// acceptable pour une garde anti-abus (pas un SLA, cf. doc de `RateLimiter`).
const RATE_LIMITER_MAX_TRACKED: usize = 10_000;

/// Identifiant du seau de débordement partagé. Préfixe distinct des seaux
/// réels (`ip:`/`key:`) : aucune collision possible.
const OVERFLOW_BUCKET_ID: &str = "overflow:shared";

impl RateLimiter {
    /// Incrémente le compteur de `id` pour `minute`. `Some(restant)` si sous la
    /// limite, `None` si dépassée. Fenêtre **fixe** (cf. doc de `RateLimiter`) :
    /// `minute` n'est pas un curseur glissant.
    fn check(&mut self, id: &str, limit: u32, minute: i64) -> Option<u32> {
        self.purge(minute);
        let id = self.effective_id(id);
        let entry = self.windows.entry(id.to_string()).or_insert((minute, 0));
        if entry.0 != minute {
            *entry = (minute, 0);
        }
        if entry.1 >= limit {
            return None;
        }
        entry.1 += 1;
        Some(limit - entry.1)
    }

    /// Le quota de `id` est-il **déjà** épuisé pour `minute` ? Lecture seule
    /// (ne consomme rien) : permet de couper court AVANT un travail coûteux
    /// (résolution de clé en base) sans pénaliser le chemin nominal.
    fn exhausted(&mut self, id: &str, limit: u32, minute: i64) -> bool {
        self.purge(minute);
        let id = self.effective_id(id);
        matches!(self.windows.get(id), Some((m, count)) if *m == minute && *count >= limit)
    }

    /// Retire les fenêtres des minutes passées, une seule fois par changement
    /// de minute (leur compteur repartirait de zéro de toute façon, cf. `check`).
    fn purge(&mut self, minute: i64) {
        if minute != self.last_purge_minute {
            self.windows.retain(|_, (m, _)| *m == minute);
            self.last_purge_minute = minute;
        }
    }

    /// Applique le plafond dur : carte pleine + identifiant inédit → seau de
    /// débordement partagé au lieu d'une insertion (borne la mémoire ET le
    /// coût de la purge).
    fn effective_id<'a>(&self, id: &'a str) -> &'a str {
        if self.windows.len() >= RATE_LIMITER_MAX_TRACKED && !self.windows.contains_key(id) {
            OVERFLOW_BUCKET_ID
        } else {
            id
        }
    }
}

/// TTL du cache négatif de résolution de clé (secondes). Court : une clé
/// fraîchement délivrée (`mint-key`) essayée trop tôt n'attend jamais plus que
/// ce délai avant d'être reconnue.
const NEGATIVE_CACHE_TTL_SECS: i64 = 60;

/// Taille maximale du cache négatif (empreintes distinctes mémorisées).
const NEGATIVE_CACHE_MAX: usize = 10_000;

/// Cache **négatif** de résolution de clé : empreinte inconnue → instant
/// d'expiration (audit 2026-08). Évite un aller-retour Postgres par requête
/// quand un même Bearer invalide est rejoué en boucle. **Borné** (pas de
/// croissance illimitée sous rotation d'empreintes) et à TTL court.
#[derive(Default)]
struct NegativeKeyCache {
    entries: HashMap<String, i64>,
}

impl NegativeKeyCache {
    /// L'empreinte est-elle connue comme invalide (entrée non expirée) ?
    fn contains(&self, hash: &str, now: i64) -> bool {
        matches!(self.entries.get(hash), Some(expiry) if *expiry > now)
    }

    /// Mémorise une empreinte inconnue. Cache plein → purge des entrées
    /// expirées ; s'il le reste, on n'insère pas (le quota par IP borne déjà
    /// l'abus — perdre le cache est sans danger, juste moins économe).
    fn insert(&mut self, hash: String, now: i64) {
        if self.entries.len() >= NEGATIVE_CACHE_MAX {
            self.entries.retain(|_, expiry| *expiry > now);
            if self.entries.len() >= NEGATIVE_CACHE_MAX {
                return;
            }
        }
        self.entries.insert(hash, now + NEGATIVE_CACHE_TTL_SECS);
    }
}

/// TTL du cache positif de résolution de clé (secondes). Borne la **latence de
/// propagation** d'une révocation ou d'un changement de tier à ≤ 60 s — sans
/// risque aujourd'hui : aucun chemin de révocation n'existe (`mint-key` ne fait
/// qu'upserter).
const POSITIVE_CACHE_TTL_SECS: i64 = 60;

/// Taille maximale du cache positif (empreintes distinctes mémorisées).
const POSITIVE_CACHE_MAX: usize = 10_000;

/// Cache **positif** de résolution de clé : empreinte valide → (enregistrement
/// résolu, instant d'expiration). Strictement symétrique du cache négatif
/// (audit 2026-08) : évite un aller-retour Postgres par requête quand une même
/// clé valide est rejouée. **Borné** et à TTL court ; invalidation par TTL seul.
#[derive(Default)]
struct PositiveKeyCache {
    entries: HashMap<String, (ApiKeyRecord, i64)>,
}

impl PositiveKeyCache {
    /// Enregistrement mémorisé pour l'empreinte (entrée non expirée), le cas
    /// échéant.
    fn get(&self, hash: &str, now: i64) -> Option<ApiKeyRecord> {
        match self.entries.get(hash) {
            Some((record, expiry)) if *expiry > now => Some(record.clone()),
            _ => None,
        }
    }

    /// Mémorise une empreinte résolue. Cache plein → purge des entrées
    /// expirées ; s'il le reste, on n'insère pas (même politique que le cache
    /// négatif — perdre le cache est sans danger, juste moins économe).
    fn insert(&mut self, hash: String, record: ApiKeyRecord, now: i64) {
        if self.entries.len() >= POSITIVE_CACHE_MAX {
            self.entries.retain(|_, (_, expiry)| *expiry > now);
            if self.entries.len() >= POSITIVE_CACHE_MAX {
                return;
            }
        }
        self.entries
            .insert(hash, (record, now + POSITIVE_CACHE_TTL_SECS));
    }
}

/// Empreinte SHA-256 hex d'une clé (jamais stockée/loguée en clair).
fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Jeton `Authorization: Bearer …`, le cas échéant.
pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?;
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// IP client. Sans `trust_proxy`, tout en-tête de transfert est ignoré
/// (spoofable) → `unknown` (seau anonyme unique). Derrière un proxy de confiance,
/// on lit l'IP réelle via [`forwarded_client_ip`].
fn client_ip(headers: &HeaderMap, config: &AuthConfig) -> String {
    if !config.trust_proxy {
        return "unknown".to_string();
    }
    forwarded_client_ip(headers, config.real_ip_header.as_deref())
        .unwrap_or_else(|| "unknown".to_string())
}

/// IP réelle telle que vue par le **reverse proxy de confiance**, toujours
/// **validée comme adresse IP** (audit 2026-08 : une valeur forgée non
/// parseable tombe dans le seau `unknown` — plus jamais de chaîne arbitraire
/// comme clé de quota ou entrée du compteur de visiteurs).
///
/// Par défaut : **dernier** segment de `X-Forwarded-For` — le proxy **ajoute**
/// l'IP réelle à droite, les segments de gauche sont fournis par le client
/// (spoofables) — d'où le dernier, pas le premier. Sûr par construction avec
/// tout proxy qui appende (Caddy, Traefik, nginx via
/// `$proxy_add_x_forwarded_for`).
///
/// `real_ip_header` (opt-in, `CARBONFR_REAL_IP_HEADER`) : lit cet en-tête dédié
/// **à la place**, sans repli sur `X-Forwarded-For` (l'opérateur a déclaré que
/// le proxy le pose ; une valeur absente ou invalide est une anomalie →
/// `unknown`). L'ancienne priorité inconditionnelle à `X-Real-Ip` rendait le
/// quota contournable derrière un proxy qui relaie l'en-tête client tel quel
/// (ex. Caddy sans `header_up X-Real-IP`).
pub(crate) fn forwarded_client_ip(
    headers: &HeaderMap,
    real_ip_header: Option<&str>,
) -> Option<String> {
    if let Some(name) = real_ip_header {
        return headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_ip);
    }
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|value| {
            value
                .split(',')
                .map(str::trim)
                .rev()
                .find(|s| !s.is_empty())
        })
        .and_then(parse_ip)
}

/// Parse une adresse IP (v4/v6) ; forme **normalisée** en sortie (deux graphies
/// IPv6 équivalentes → une seule clé de seau).
fn parse_ip(value: &str) -> Option<String> {
    value
        .trim()
        .parse::<std::net::IpAddr>()
        .ok()
        .map(|ip| ip.to_string())
}

fn limit_for(tier: ApiTier, config: &AuthConfig) -> u32 {
    match tier {
        ApiTier::Free => config.free_per_min,
    }
}

fn current_minute() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp() / 60
}

fn error_response(
    status: StatusCode,
    code: &'static str,
    title: &'static str,
    message: &str,
) -> Response {
    problem_response(status, code, title, message.to_string())
}

/// Middleware d'authentification + quota. À appliquer via
/// `axum::middleware::from_fn_with_state` au routeur protégé.
///
/// **Scopé au contrat `/v1`** (ADR-0007 : tout endpoint public vit sous `/v1`) :
/// les routes hors contrat — `/health`, `/health/ready`, `/metrics`, `/docs` —
/// ne sont jamais soumises au quota ni à l'authentification, même quand la
/// composition (`bin/server`) les fusionne dans le même `Router` que celui qui
/// reçoit cette couche (audit F07). Sondes de liveness et scrape Prometheus
/// doivent rester joignables quel que soit l'état du quota, sinon un pic de
/// trafic sur l'API provoque une panne auto-infligée (429 sur `/health` →
/// redémarrage par l'orchestrateur ; 429 sur `/metrics` → perte d'observabilité).
pub async fn enforce(State(state): State<AuthState>, request: Request, next: Next) -> Response {
    if !request.uri().path().starts_with("/v1") {
        return next.run(request).await;
    }
    // Préflight CORS : jamais décompté du quota ni soumis à l'auth — un
    // préflight n'exécute aucun handler et ne porte jamais `Authorization`
    // (spec Fetch). Défense en profondeur (audit 2026-08) : la `CorsLayer`,
    // couche la plus externe (cf. `router()`), répond normalement au préflight
    // avant même d'atteindre ce middleware.
    if request.method() == axum::http::Method::OPTIONS {
        return next.run(request).await;
    }
    let headers = request.headers();
    let ip = client_ip(headers, &state.config);
    let anon_id = format!("ip:{ip}");
    let minute = current_minute();

    // 1. Résolution du principal. Les **échecs** (clé inconnue, base
    //    injoignable) sont décomptés du seau anonyme de l'IP (audit 2026-08) :
    //    sans cela, le chemin non authentifié le plus coûteux (SHA-256 + SELECT
    //    Postgres) était le seul jamais throttlé — bypass de quota par Bearer
    //    aléatoire. Un cache négatif court évite en plus l'aller-retour base
    //    quand la même empreinte invalide est rejouée (et son symétrique
    //    positif, quand c'est une clé valide qui l'est).
    let principal = match bearer_token(headers) {
        Some(token) => {
            let hash = hash_key(&token);
            let now = time::OffsetDateTime::now_utc().unix_timestamp();
            let known_invalid = state
                .negative
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .contains(&hash, now);
            if known_invalid {
                return auth_failure_response(&state, &anon_id, minute, unauthorized_response);
            }
            // Seau anonyme déjà épuisé → 429 SANS toucher la base : coupe le
            // coût Postgres d'une inondation de Bearer aléatoires (lecture
            // seule — une clé valide depuis une IP « propre » ne consomme rien
            // ici). Effet de bord assumé : seau anonyme de l'IP épuisé = clé
            // valide de la même IP throttlée jusqu'à la minute suivante (même
            // trade-off que le seau partagé `unknown` sans proxy).
            let anon_exhausted = state
                .limiter
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .exhausted(&anon_id, state.config.anonymous_per_min, minute);
            if anon_exhausted {
                return rate_limited_response(state.config.anonymous_per_min);
            }
            // Cache positif : une clé valide rejouée ne coûte pas un SELECT par
            // requête. Hit → principal sans toucher la base (le quota par clé
            // `key:{hash}` s'applique pareil en aval) ; miss → résolution comme
            // avant, mémorisée à TTL court.
            let cached = state
                .positive
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .get(&hash, now);
            if let Some(record) = cached {
                Principal::Keyed {
                    id: hash,
                    tier: record.tier,
                }
            } else {
                match state.keys.resolve(&hash).await {
                    Ok(Some(record)) => {
                        let tier = record.tier;
                        state
                            .positive
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner())
                            .insert(hash.clone(), record, now);
                        Principal::Keyed { id: hash, tier }
                    }
                    Ok(None) => {
                        state
                            .negative
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner())
                            .insert(hash, now);
                        return auth_failure_response(
                            &state,
                            &anon_id,
                            minute,
                            unauthorized_response,
                        );
                    }
                    // Erreur transitoire (base) : pas de cache négatif — la clé est
                    // peut-être valide ; mais l'échec est décompté (ne pas offrir un
                    // chemin illimité vers une base déjà en difficulté).
                    Err(_) => {
                        return auth_failure_response(&state, &anon_id, minute, || {
                            error_response(
                                StatusCode::SERVICE_UNAVAILABLE,
                                "unavailable",
                                "Service indisponible",
                                "vérification de la clé impossible",
                            )
                        });
                    }
                }
            }
        }
        None => Principal::Anonymous { ip },
    };

    // 2. Application du quota (fenêtre minute).
    let (limit, id) = match &principal {
        Principal::Anonymous { ip } => (state.config.anonymous_per_min, format!("ip:{ip}")),
        Principal::Keyed { id, tier } => (limit_for(*tier, &state.config), format!("key:{id}")),
    };
    // Section critique triviale (incrément d'un compteur) : un verrou empoisonné
    // ne peut venir que d'un panic improbable ; on récupère le garde plutôt que
    // de propager le panic (convention : pas d'`expect` hors tests/bootstrap).
    let remaining = state
        .limiter
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .check(&id, limit, minute);

    match remaining {
        Some(remaining) => {
            let mut response = next.run(request).await;
            let h = response.headers_mut();
            h.insert("ratelimit-limit", limit.into());
            h.insert("ratelimit-remaining", remaining.into());
            response
        }
        None => rate_limited_response(limit),
    }
}

/// 401 « clé API inconnue » (Problem Details ; `WWW-Authenticate` posé par
/// `problem_response`).
fn unauthorized_response() -> Response {
    error_response(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "Non autorisé",
        "clé API inconnue",
    )
}

/// Décompte un **échec d'authentification** dans le seau anonyme de l'IP : si
/// le quota est épuisé, le 429 prime sur la réponse d'erreur (une inondation de
/// clés invalides est throttlée comme n'importe quel anonyme, audit 2026-08).
fn auth_failure_response(
    state: &AuthState,
    anon_id: &str,
    minute: i64,
    failure: impl FnOnce() -> Response,
) -> Response {
    let debit = state
        .limiter
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .check(anon_id, state.config.anonymous_per_min, minute);
    if debit.is_none() {
        return rate_limited_response(state.config.anonymous_per_min);
    }
    failure()
}

/// 429 « quota dépassé » avec en-têtes `RateLimit-*` et `Retry-After`.
fn rate_limited_response(limit: u32) -> Response {
    let mut response = error_response(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        "Quota dépassé",
        "quota dépassé",
    );
    let h = response.headers_mut();
    h.insert("ratelimit-limit", limit.into());
    h.insert("ratelimit-remaining", 0.into());
    // Réessai au début de la minute suivante.
    let retry = 60 - (time::OffsetDateTime::now_utc().unix_timestamp() % 60);
    if let Ok(v) = retry.to_string().parse() {
        h.insert(header::RETRY_AFTER, v);
    }
    response
}

/// Empreinte d'une clé en clair, pour la délivrance (`mint-key`).
pub fn key_fingerprint(key: &str) -> String {
    hash_key(key)
}

/// Jeton aléatoire hexadécimal de `bytes` octets (CSPRNG **userspace** `rand`,
/// non bloquant — plus d'I/O fichier synchrone `/dev/urandom` sur le runtime
/// Tokio, audit F29) — id et secret d'abonnement webhook. Signature conservée en
/// `Option<String>` par cohérence avec les appelants ; renvoie toujours `Some`
/// en pratique (le CSPRNG OS ne panique qu'en cas d'échec catastrophique
/// d'initialisation de l'entropie système, hors de portée d'un repli gracieux).
pub(crate) fn random_hex(bytes: usize) -> Option<String> {
    use rand::Rng;
    let mut buf = vec![0u8; bytes];
    rand::rng().fill(buf.as_mut_slice());
    Some(buf.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_blocks_after_limit_then_resets_next_minute() {
        let mut rl = RateLimiter::default();
        assert_eq!(rl.check("a", 2, 100), Some(1));
        assert_eq!(rl.check("a", 2, 100), Some(0));
        assert_eq!(rl.check("a", 2, 100), None); // dépassé
        // Minute suivante : remis à zéro.
        assert_eq!(rl.check("a", 2, 101), Some(1));
        // Autre principal : compteur indépendant.
        assert_eq!(rl.check("b", 2, 101), Some(1));
    }

    #[test]
    fn bearer_token_parsing() {
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, "Bearer abc123".parse().unwrap());
        assert_eq!(bearer_token(&h), Some("abc123".to_string()));
        let mut empty = HeaderMap::new();
        empty.insert(header::AUTHORIZATION, "Basic xyz".parse().unwrap());
        assert_eq!(bearer_token(&empty), None);
    }

    #[test]
    fn rate_limiter_purges_once_per_minute_change() {
        // Les fenêtres des minutes passées sont retirées au changement de
        // minute (et une seule fois — audit 2026-08 : la purge ne tourne plus
        // à chaque `check` sous inondation).
        let mut rl = RateLimiter::default();
        for i in 0..50 {
            rl.check(&format!("ip:{i}"), 10, 100);
        }
        assert_eq!(rl.windows.len(), 50);
        rl.check("ip:new", 10, 101);
        // Seule l'entrée de la minute courante subsiste.
        assert_eq!(rl.windows.len(), 1);
        assert_eq!(rl.last_purge_minute, 101);
    }

    #[test]
    fn rate_limiter_caps_tracked_identifiers_with_overflow_bucket() {
        // Carte pleine : les identifiants inédits partagent le seau de
        // débordement (quota collectif) au lieu de croître sans borne.
        let mut rl = RateLimiter::default();
        for i in 0..RATE_LIMITER_MAX_TRACKED {
            rl.check(&format!("ip:{i}"), 10, 100);
        }
        assert_eq!(rl.check("ip:flood-1", 2, 100), Some(1));
        assert_eq!(rl.check("ip:flood-2", 2, 100), Some(0));
        // 3e identifiant inédit : le seau partagé est épuisé → throttlé.
        assert_eq!(rl.check("ip:flood-3", 2, 100), None);
        // La carte n'a grossi que du seau de débordement.
        assert_eq!(rl.windows.len(), RATE_LIMITER_MAX_TRACKED + 1);
        // Un identifiant déjà suivi garde son propre seau.
        assert_eq!(rl.check("ip:7", 10, 100), Some(8));
    }

    #[test]
    fn rate_limiter_exhausted_is_read_only() {
        let mut rl = RateLimiter::default();
        // Identifiant jamais vu : non épuisé, et le sondage ne consomme rien.
        assert!(!rl.exhausted("a", 2, 100));
        assert!(!rl.windows.contains_key("a"));
        assert_eq!(rl.check("a", 2, 100), Some(1));
        assert!(!rl.exhausted("a", 2, 100));
        assert_eq!(rl.check("a", 2, 100), Some(0));
        assert!(rl.exhausted("a", 2, 100));
        // Minute suivante : la fenêtre est repartie de zéro.
        assert!(!rl.exhausted("a", 2, 101));
    }

    #[test]
    fn negative_cache_remembers_then_expires() {
        let mut cache = NegativeKeyCache::default();
        cache.insert("hash".to_string(), 1_000);
        assert!(cache.contains("hash", 1_000));
        assert!(cache.contains("hash", 1_000 + NEGATIVE_CACHE_TTL_SECS - 1));
        // TTL écoulé : l'empreinte doit être re-résolue en base.
        assert!(!cache.contains("hash", 1_000 + NEGATIVE_CACHE_TTL_SECS));
        assert!(!cache.contains("autre", 1_000));
    }

    #[test]
    fn negative_cache_is_bounded() {
        let mut cache = NegativeKeyCache::default();
        for i in 0..NEGATIVE_CACHE_MAX {
            cache.insert(format!("h{i}"), 1_000);
        }
        // Plein, rien d'expiré → l'insertion est refusée (pas de croissance).
        cache.insert("trop".to_string(), 1_000);
        assert_eq!(cache.entries.len(), NEGATIVE_CACHE_MAX);
        assert!(!cache.contains("trop", 1_000));
        // Après expiration générale, l'insertion repasse (purge au passage).
        cache.insert("frais".to_string(), 1_000 + NEGATIVE_CACHE_TTL_SECS);
        assert_eq!(cache.entries.len(), 1);
        assert!(cache.contains("frais", 1_000 + NEGATIVE_CACHE_TTL_SECS));
    }

    #[test]
    fn positive_cache_remembers_then_expires() {
        let record = ApiKeyRecord {
            tier: ApiTier::Free,
            label: "test".to_string(),
        };
        let mut cache = PositiveKeyCache::default();
        cache.insert("hash".to_string(), record.clone(), 1_000);
        assert_eq!(cache.get("hash", 1_000), Some(record.clone()));
        assert_eq!(
            cache.get("hash", 1_000 + POSITIVE_CACHE_TTL_SECS - 1),
            Some(record)
        );
        // TTL écoulé : l'empreinte doit être re-résolue en base (latence de
        // propagation d'une révocation/changement de tier ≤ TTL).
        assert_eq!(cache.get("hash", 1_000 + POSITIVE_CACHE_TTL_SECS), None);
        assert_eq!(cache.get("autre", 1_000), None);
    }

    #[test]
    fn positive_cache_is_bounded() {
        let record = ApiKeyRecord {
            tier: ApiTier::Free,
            label: "test".to_string(),
        };
        let mut cache = PositiveKeyCache::default();
        for i in 0..POSITIVE_CACHE_MAX {
            cache.insert(format!("h{i}"), record.clone(), 1_000);
        }
        // Plein, rien d'expiré → l'insertion est refusée (pas de croissance).
        cache.insert("trop".to_string(), record.clone(), 1_000);
        assert_eq!(cache.entries.len(), POSITIVE_CACHE_MAX);
        assert_eq!(cache.get("trop", 1_000), None);
        // Après expiration générale, l'insertion repasse (purge au passage).
        cache.insert(
            "frais".to_string(),
            record.clone(),
            1_000 + POSITIVE_CACHE_TTL_SECS,
        );
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(
            cache.get("frais", 1_000 + POSITIVE_CACHE_TTL_SECS),
            Some(record)
        );
    }

    #[test]
    fn forwarded_ip_ignores_x_real_ip_by_default() {
        // Audit 2026-08 : `X-Real-Ip` n'est plus lu par défaut (spoofable si le
        // proxy ne l'écrase pas, ex. Caddy sans `header_up`). Défaut = dernier
        // segment de XFF, appendé par le proxy.
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "1.1.1.1, 2.2.2.2".parse().unwrap());
        h.insert("x-real-ip", "9.9.9.9".parse().unwrap());
        assert_eq!(forwarded_client_ip(&h, None), Some("2.2.2.2".to_string()));
    }

    #[test]
    fn forwarded_ip_uses_configured_real_ip_header_only_when_opted_in() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "1.1.1.1, 2.2.2.2".parse().unwrap());
        h.insert("x-real-ip", "9.9.9.9".parse().unwrap());
        // Opt-in explicite : l'en-tête dédié prime.
        assert_eq!(
            forwarded_client_ip(&h, Some("x-real-ip")),
            Some("9.9.9.9".to_string())
        );
        // En-tête configuré mais absent : pas de repli spoofable → unknown.
        let mut xff_only = HeaderMap::new();
        xff_only.insert("x-forwarded-for", "3.3.3.3".parse().unwrap());
        assert_eq!(forwarded_client_ip(&xff_only, Some("x-real-ip")), None);
    }

    #[test]
    fn forwarded_ip_takes_last_xff_segment_not_spoofable_first() {
        // Le client peut pré-remplir XFF ; le proxy de confiance AJOUTE l'IP réelle
        // à droite → on prend le dernier segment (1.1.1.1 = spoofé, ignoré).
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "1.1.1.1, 203.0.113.7".parse().unwrap());
        assert_eq!(
            forwarded_client_ip(&h, None),
            Some("203.0.113.7".to_string())
        );
    }

    #[test]
    fn forwarded_ip_rejects_forged_non_ip_values() {
        // Audit 2026-08 : une valeur qui n'est pas une adresse IP ne devient
        // jamais clé de quota ni entrée du compteur de visiteurs → unknown.
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "1.2.3.4, not-an-ip".parse().unwrap());
        assert_eq!(forwarded_client_ip(&h, None), None);
        let mut real = HeaderMap::new();
        real.insert("x-real-ip", "a1".parse().unwrap());
        assert_eq!(forwarded_client_ip(&real, Some("x-real-ip")), None);
        // Une IPv6 valide passe, sous forme normalisée.
        let mut v6 = HeaderMap::new();
        v6.insert("x-forwarded-for", "2001:DB8:0:0::1".parse().unwrap());
        assert_eq!(
            forwarded_client_ip(&v6, None),
            Some("2001:db8::1".to_string())
        );
    }

    #[test]
    fn forwarded_ip_none_without_headers() {
        assert_eq!(forwarded_client_ip(&HeaderMap::new(), None), None);
    }
}
