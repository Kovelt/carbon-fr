//! Cadres et **rulesets versionnés** d'éligibilité (ADR-0025/0026).
//!
//! Aucune valeur réglementaire n'est codée en dur dans la logique : tout passe
//! par un [`EligibilityRuleset`] **versionné et servi** (à l'image des
//! méthodologies carbone, ADR-0005/0006). Une nouvelle version = une nouvelle
//! entrée au catalogue, jamais une mutation silencieuse d'une version publiée.

use time::{Date, Month, OffsetDateTime};

/// Comparateur fossile GHG de l'hydrogène (gCO₂eq/MJ) — RED II / actes délégués
/// UE. Base commune RFNBO (2023/1185) et bas-carbone (2025/2359). **[FAIT]**
pub const FOSSIL_COMPARATOR_G_PER_MJ: f64 = 94.0;

/// Réduction GHG minimale exigée d'un carburant bas-carbone / RFNBO. **[FAIT]**
pub const REQUIRED_GHG_REDUCTION: f64 = 0.70;

/// Pouvoir calorifique inférieur (PCI) de l'hydrogène, MJ/kg. **[FAIT]**
pub const H2_LHV_MJ_PER_KG: f64 = 120.0;

/// Budget GHG **produit** d'un H₂ bas-carbone, en gCO₂eq/kgH₂ :
/// `94 × (1 − 0,70) × 120 = 3384`. **[FAIT]** (seuil produit 28,2 gCO₂eq/MJ).
pub const LOW_CARBON_BUDGET_G_PER_KG: f64 =
    FOSSIL_COMPARATOR_G_PER_MJ * (1.0 - REQUIRED_GHG_REDUCTION) * H2_LHV_MJ_PER_KG;

/// Consommation électrique de référence d'un électrolyseur (kWh/kgH₂), médiane de
/// la fourchette industrielle 50–55. **[FAIT]** (fourchette). Paramétrable.
pub const DEFAULT_ELECTROLYZER_KWH_PER_KG: f64 = 53.0;

/// Plafond de sûreté du seuil d'intensité bas-carbone (gCO₂eq/kWh). Au-delà, le
/// seuil n'a plus de sens (aucune intensité électrique réelle ne l'approche) et
/// rendrait le pilier trivialement toujours vrai. Miroir de la borne `]0, 1000]`
/// appliquée au seuil direct côté HTTP : garantit qu'un seuil **dérivé** d'une
/// consommation absurde ne peut pas la contourner (audit F03).
pub const MAX_LOW_CARBON_INTENSITY_THRESHOLD: f64 = 1000.0;

/// Seuil **d'intensité électrique** bas-carbone dérivé, en gCO₂eq/kWh.
///
/// **[ESTIMATION — proxy carbon-fr, NON réglementaire]** : c'est une *condition
/// nécessaire* (« drapeau rouge »). Au-delà, l'électricité **à elle seule** crève
/// déjà le budget GHG produit (`LOW_CARBON_BUDGET_G_PER_KG`) → l'H₂ ne peut pas
/// être bas-carbone, indépendamment des autres postes (compression, eau,
/// auxiliaires). En deçà, l'H₂ *pourrait* qualifier (dépend de ces autres postes).
/// L'acte 2025/2359 ne fixe aucun seuil **électrique** ; il fixe le seuil produit.
pub fn low_carbon_intensity_threshold(kwh_per_kg: f64) -> f64 {
    (LOW_CARBON_BUDGET_G_PER_KG / kwh_per_kg).round()
}

/// Cadre d'éligibilité servi (binôme neutre, ADR-0025). `carbon-fr` expose
/// l'éligibilité au regard de chaque cadre ; il ne tranche pas la « couleur » de
/// l'hydrogène.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EligibilityFramework {
    /// Renouvelable (Règl. délégués (UE) 2023/1184 & 2023/1185).
    Rfnbo,
    /// Bas-carbone inclusif (nucléaire, gaz+CCS — acte délégué (UE) 2025/2359).
    LowCarbon,
}

impl EligibilityFramework {
    pub fn slug(self) -> &'static str {
        match self {
            EligibilityFramework::Rfnbo => "rfnbo",
            EligibilityFramework::LowCarbon => "low-carbon",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "rfnbo" => Some(EligibilityFramework::Rfnbo),
            "low-carbon" => Some(EligibilityFramework::LowCarbon),
            _ => None,
        }
    }
}

/// Granularité de la corrélation temporelle RFNBO (mensuelle jusqu'à la bascule,
/// horaire ensuite). Pilier **RFNBO** ; sans objet pour `low-carbon`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalGranularity {
    Monthly,
    Hourly,
}

impl TemporalGranularity {
    pub fn slug(self) -> &'static str {
        match self {
            TemporalGranularity::Monthly => "monthly",
            TemporalGranularity::Hourly => "hourly",
        }
    }
}

/// Statut d'un ruleset au catalogue. `Served` = résolvable et appliqué ;
/// `Planned` = présent au catalogue (transparence) mais **jamais résolu** tant
/// que le droit n'est pas en vigueur (ADR-0006/0020).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesetStatus {
    Served,
    Planned,
}

impl RulesetStatus {
    pub fn slug(self) -> &'static str {
        match self {
            RulesetStatus::Served => "served",
            RulesetStatus::Planned => "planned",
        }
    }
}

/// Jeu de paramètres **versionné** d'un cadre d'éligibilité.
///
/// Tous les champs `Copy` : un ruleset se passe par valeur sans allocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EligibilityRuleset {
    /// Version stable (ex. `rfnbo:2023-1184`) — fait partie de l'identité servie.
    pub version: &'static str,
    pub framework: EligibilityFramework,
    pub status: RulesetStatus,
    pub adr: &'static str,
    /// Granularité par défaut **avant** la bascule horaire (pilier RFNBO).
    pub granularity: TemporalGranularity,
    /// Date de bascule vers la corrélation horaire (pilier RFNBO).
    pub hourly_switchover: Date,
    /// Seuil de l'exception « Article 4 » (part renouvelable de la zone). **[FAIT]**
    /// (légalement une moyenne **annuelle** ; utilisé ici comme seuil du signal
    /// `RenewableShare` **instantané** — proxy par créneau, cf. `evaluate`).
    pub article4_renewable_threshold: f64,
    /// Seuil de l'exception « surplus » (prix day-ahead, €/MWh). `None` désactive
    /// le pilier prix (cas `low-carbon`). **[FAIT]**
    pub surplus_price_eur_mwh: Option<f64>,
    /// Seuil d'intensité électrique bas-carbone (gCO₂eq/kWh), dérivé. `None` hors
    /// `low-carbon`. **[ESTIMATION]** (cf. [`low_carbon_intensity_threshold`]).
    pub low_carbon_intensity_threshold_g_per_kwh: Option<f64>,
    /// `true` : le seuil bas-carbone est un proxy non réglementaire (étiquetage
    /// obligatoire, ADR-0024/0025).
    pub low_carbon_intensity_is_indicative: bool,
    /// Consommation électrolyseur retenue pour dériver le seuil (kWh/kgH₂).
    pub electrolyzer_kwh_per_kg: f64,
    /// `true` si des overrides utilisateur ont été appliqués.
    pub overridden: bool,
    /// Citation textuelle de la base réglementaire (vérifiabilité).
    pub legal_basis: &'static str,
    /// Description neutre servie au catalogue.
    pub description: &'static str,
}

impl EligibilityRuleset {
    /// `rfnbo:2023-1184` — droit en vigueur (Règl. délégués (UE) 2023/1184 & 1185).
    pub fn rfnbo_2023_1184() -> Self {
        Self {
            version: "rfnbo:2023-1184",
            framework: EligibilityFramework::Rfnbo,
            status: RulesetStatus::Served,
            adr: "ADR-0026",
            granularity: TemporalGranularity::Monthly,
            hourly_switchover: date(2030, Month::January, 1),
            article4_renewable_threshold: 0.90,
            surplus_price_eur_mwh: Some(20.0),
            low_carbon_intensity_threshold_g_per_kwh: None,
            low_carbon_intensity_is_indicative: false,
            electrolyzer_kwh_per_kg: DEFAULT_ELECTROLYZER_KWH_PER_KG,
            overridden: false,
            legal_basis: "Règl. délégués (UE) 2023/1184 (corrélation temporelle/géographique, \
                          exceptions Art. 4 ≥90 % renouvelables et surplus prix ≤20 €/MWh ou \
                          <0,36×EUA) & 2023/1185 (réduction GHG ≥70 % vs 94 gCO₂eq/MJ). [FAIT]",
            description: "Vue renouvelable. Signaux RÉSEAU : part renouvelable instantanée du mix \
                          national (proxy ; l'Article 4 légal est une moyenne ANNUELLE de zone, \
                          ≈ jamais ≥90 % en FR) et surplus prix day-ahead ≤20 €/MWh. La branche EUA \
                          (<0,36×prix EUA) et l'additionnalité (PPA, niveau site) sont HORS périmètre. \
                          Corrélation géographique évaluée à la zone de dépôt nationale (FR = 1 zone).",
        }
    }

    /// `low-carbon:2025-2359` — acte délégué (UE) 2025/2359 (adopté 8/7/2025).
    pub fn low_carbon_2025_2359() -> Self {
        let kwh = DEFAULT_ELECTROLYZER_KWH_PER_KG;
        Self {
            version: "low-carbon:2025-2359",
            framework: EligibilityFramework::LowCarbon,
            status: RulesetStatus::Served,
            adr: "ADR-0026",
            granularity: TemporalGranularity::Hourly,
            hourly_switchover: date(2025, Month::December, 11),
            article4_renewable_threshold: 0.0,
            surplus_price_eur_mwh: None,
            low_carbon_intensity_threshold_g_per_kwh: Some(low_carbon_intensity_threshold(kwh)),
            low_carbon_intensity_is_indicative: true,
            electrolyzer_kwh_per_kg: kwh,
            overridden: false,
            legal_basis: "Acte délégué (UE) 2025/2359 (adopté 8/7/2025) : seuil PRODUIT 28,2 gCO₂eq/MJ \
                          (=94×0,30) ≈3384 gCO₂eq/kgH₂. Seuil ÉLECTRIQUE = ESTIMATION carbon-fr : \
                          3384 ÷ 53 kWh/kg ≈ 64 gCO₂eq/kWh (condition nécessaire, tout le budget \
                          attribué à l'élec). Reconnaissance du nucléaire en cours (consultation \
                          d'ici 30/06/2026, évaluation d'ici 07/2028). [FAIT seuil produit / ESTIMATION seuil élec]",
            description: "Vue bas-carbone inclusive (nucléaire, gaz+CCS). Signal RÉSEAU : intensité \
                          carbone ≤ seuil dérivé (~64 gCO₂eq/kWh, INDICATIF, condition nécessaire — \
                          au-delà, H₂ bas-carbone impossible par l'électricité seule). Pas de pilier \
                          prix ni renouvelable. gCO₂eq/kgH₂ et certification HORS périmètre.",
        }
    }

    /// `rfnbo:2026-revision` — révision attendue (report de la bascule horaire,
    /// propositions 2031-2033, NON adopté). **`Planned`** : jamais résolu (D6).
    pub fn rfnbo_2026_revision() -> Self {
        Self {
            version: "rfnbo:2026-revision",
            status: RulesetStatus::Planned,
            adr: "ADR-0026",
            // Conserve la date EN VIGUEUR : aucune date de report n'est figée comme
            // un fait tant que le droit n'est pas adopté.
            hourly_switchover: date(2030, Month::January, 1),
            legal_basis: "Propositions de révision RFNBO (révision attendue ~juin 2026) : report de \
                          la bascule horaire (propositions 2031-2033 selon les sources) et phase-in \
                          de l'additionnalité. DROIT NON ADOPTÉ — non servi. [PROPOSITIONS]",
            description: "Réservé. Révision RFNBO attendue (report de l'échéance horaire, NON \
                          adoptée). Présent au catalogue pour transparence ; jamais appliqué.",
            ..Self::rfnbo_2023_1184()
        }
    }

    /// Granularité **effective** à un instant : horaire dès la bascule.
    pub fn granularity_at(&self, at: OffsetDateTime) -> TemporalGranularity {
        if at.date() >= self.hourly_switchover {
            TemporalGranularity::Hourly
        } else {
            self.granularity
        }
    }

    /// Applique des overrides bornés (conserve `version`, pose `overridden`).
    ///
    /// Ordre des paramètres **figé** : `surplus_price`, puis `intensity_threshold`,
    /// puis `kwh_per_kg`. Un seuil d'intensité explicite l'emporte ; sinon, un
    /// `kwh_per_kg` recale le seuil dérivé (seulement si le cadre porte un seuil).
    pub fn with_overrides(
        mut self,
        surplus_price_eur_mwh: Option<f64>,
        low_carbon_intensity_threshold_g_per_kwh: Option<f64>,
        electrolyzer_kwh_per_kg: Option<f64>,
    ) -> Self {
        let mut changed = false;
        if let Some(p) = surplus_price_eur_mwh {
            self.surplus_price_eur_mwh = Some(p);
            changed = true;
        }
        if let Some(kwh) = electrolyzer_kwh_per_kg {
            self.electrolyzer_kwh_per_kg = kwh;
            if self.low_carbon_intensity_threshold_g_per_kwh.is_some()
                && low_carbon_intensity_threshold_g_per_kwh.is_none()
            {
                // Défense en profondeur : plafonner le seuil dérivé au même
                // maximum que le seuil direct (validé `]0, 1000]` côté HTTP), pour
                // qu'un `kwh` absurde ne produise pas un seuil hors contrat même si
                // ce crate pur est appelé sans la validation HTTP (audit F03).
                self.low_carbon_intensity_threshold_g_per_kwh = Some(
                    low_carbon_intensity_threshold(kwh).min(MAX_LOW_CARBON_INTENSITY_THRESHOLD),
                );
            }
            changed = true;
        }
        if let Some(t) = low_carbon_intensity_threshold_g_per_kwh {
            self.low_carbon_intensity_threshold_g_per_kwh = Some(t);
            changed = true;
        }
        self.overridden = changed;
        self
    }
}

/// Catalogue complet des rulesets (2 servis + 1 planifié).
pub fn ruleset_catalog() -> Vec<EligibilityRuleset> {
    vec![
        EligibilityRuleset::rfnbo_2023_1184(),
        EligibilityRuleset::low_carbon_2025_2359(),
        EligibilityRuleset::rfnbo_2026_revision(),
    ]
}

/// Résout un ruleset **servi** par cadre + version optionnelle.
///
/// `version` accepte la forme complète (`rfnbo:2023-1184`) ou le suffixe
/// (`2023-1184`). `None` → l'unique ruleset servi du cadre. Renvoie `None` si
/// inconnu **ou** `Planned` (jamais appliqué, D6).
pub fn resolve_ruleset(
    framework: EligibilityFramework,
    version: Option<&str>,
) -> Option<EligibilityRuleset> {
    ruleset_catalog().into_iter().find(|r| {
        r.framework == framework
            && r.status == RulesetStatus::Served
            && match version {
                None => true,
                Some(v) => r.version == v || r.version.split(':').nth(1) == Some(v),
            }
    })
}

/// Fabrique de date littérale (seul `expect` toléré : constante de code).
fn date(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("date littérale valide")
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Time;

    /// Construit un `OffsetDateTime` UTC sans la feature `time/macros` (D9).
    fn at(year: i32, month: Month, day: u8, hour: u8) -> OffsetDateTime {
        OffsetDateTime::new_utc(
            Date::from_calendar_date(year, month, day).expect("date de test"),
            Time::from_hms(hour, 0, 0).expect("heure de test"),
        )
    }

    #[test]
    fn low_carbon_threshold_is_derived_round_64_at_53_kwh() {
        assert_eq!(low_carbon_intensity_threshold(53.0), 64.0);
        // Budget produit = 28,2 g/MJ × 120 MJ/kg = 3384 g/kg.
        assert!((LOW_CARBON_BUDGET_G_PER_KG - 3384.0).abs() < 1e-6);
    }

    #[test]
    fn threshold_scales_with_consumption() {
        assert_eq!(low_carbon_intensity_threshold(50.0), 68.0); // 3384/50 = 67,68
        assert_eq!(low_carbon_intensity_threshold(55.0), 62.0); // 3384/55 = 61,5
    }

    #[test]
    fn framework_slugs_roundtrip() {
        for f in [EligibilityFramework::Rfnbo, EligibilityFramework::LowCarbon] {
            assert_eq!(EligibilityFramework::from_slug(f.slug()), Some(f));
        }
        assert_eq!(EligibilityFramework::from_slug("vert"), None);
    }

    #[test]
    fn granularity_switches_to_hourly_after_switchover() {
        let r = EligibilityRuleset::rfnbo_2023_1184();
        assert_eq!(
            r.granularity_at(at(2029, Month::December, 31, 23)),
            TemporalGranularity::Monthly
        );
        assert_eq!(
            r.granularity_at(at(2030, Month::January, 1, 0)),
            TemporalGranularity::Hourly
        );
    }

    #[test]
    fn catalog_serves_only_in_force_law() {
        let served: Vec<_> = ruleset_catalog()
            .into_iter()
            .filter(|r| r.status == RulesetStatus::Served)
            .map(|r| r.version)
            .collect();
        assert_eq!(served, ["rfnbo:2023-1184", "low-carbon:2025-2359"]);
    }

    #[test]
    fn resolve_ruleset_rejects_planned_and_unknown() {
        assert!(resolve_ruleset(EligibilityFramework::Rfnbo, None).is_some());
        // La version planifiée existe au catalogue mais n'est jamais résolue.
        assert!(resolve_ruleset(EligibilityFramework::Rfnbo, Some("2026-revision")).is_none());
        assert!(resolve_ruleset(EligibilityFramework::Rfnbo, Some("9999")).is_none());
    }

    #[test]
    fn resolve_ruleset_accepts_full_or_suffix_version() {
        let by_full = resolve_ruleset(EligibilityFramework::Rfnbo, Some("rfnbo:2023-1184"));
        let by_suffix = resolve_ruleset(EligibilityFramework::Rfnbo, Some("2023-1184"));
        assert_eq!(by_full, by_suffix);
        assert!(by_full.is_some());
    }

    #[test]
    fn default_low_carbon_threshold_is_64() {
        let r = EligibilityRuleset::low_carbon_2025_2359();
        assert_eq!(r.low_carbon_intensity_threshold_g_per_kwh, Some(64.0));
        assert!(r.low_carbon_intensity_is_indicative);
    }

    #[test]
    fn with_overrides_recomputes_threshold_from_kwh() {
        let r = EligibilityRuleset::low_carbon_2025_2359().with_overrides(None, None, Some(50.0));
        assert_eq!(r.low_carbon_intensity_threshold_g_per_kwh, Some(68.0));
        assert!(r.overridden);
        assert_eq!(r.electrolyzer_kwh_per_kg, 50.0);
    }

    #[test]
    fn absurd_kwh_derives_capped_threshold_not_gigantic() {
        // kwh absurde (erreur d'unité) : le seuil dérivé serait round(3384/0.53)
        // ≈ 6385 sans garde ; il est plafonné au maximum de sûreté (audit F03).
        let r = EligibilityRuleset::low_carbon_2025_2359().with_overrides(None, None, Some(0.53));
        assert_eq!(
            r.low_carbon_intensity_threshold_g_per_kwh,
            Some(MAX_LOW_CARBON_INTENSITY_THRESHOLD)
        );
    }

    #[test]
    fn explicit_threshold_override_wins_over_kwh() {
        let r =
            EligibilityRuleset::low_carbon_2025_2359().with_overrides(None, Some(40.0), Some(50.0));
        assert_eq!(r.low_carbon_intensity_threshold_g_per_kwh, Some(40.0));
    }

    #[test]
    fn surplus_override_keeps_version_sets_overridden() {
        let r = EligibilityRuleset::rfnbo_2023_1184().with_overrides(Some(5.0), None, None);
        assert_eq!(r.version, "rfnbo:2023-1184");
        assert_eq!(r.surplus_price_eur_mwh, Some(5.0));
        assert!(r.overridden);
    }

    #[test]
    fn kwh_override_on_rfnbo_does_not_invent_a_threshold() {
        // rfnbo ne porte pas de seuil d'intensité → un override kwh ne doit pas en créer un.
        let r = EligibilityRuleset::rfnbo_2023_1184().with_overrides(None, None, Some(50.0));
        assert_eq!(r.low_carbon_intensity_threshold_g_per_kwh, None);
    }

    #[test]
    fn no_overrides_leaves_overridden_false() {
        let r = EligibilityRuleset::rfnbo_2023_1184().with_overrides(None, None, None);
        assert!(!r.overridden);
    }
}
