//! Décomposition du prix de l'électricité, ancrée sur le **TRV** (ADR-0023).
//!
//! On n'affiche pas deux chiffres en regard (« coût de production » vs
//! « facture ») : on expose **toute la chaîne du prix payé**, décomposée par
//! composante, chacune sourcée. Le « prix réel de l'énergie » n'est pas asséné —
//! il **émerge** comme la composante énergie (prix spot day-ahead, factuel).
//!
//! Domaine **pur** : aucune IO. La donnée spot vient d'un port amont
//! (`SpotPriceSource`) ; les composantes réglementaires (TURPE, accise, TVA,
//! résidu TRV) sont des **constantes de domaine versionnées** par période de
//! validité — exactement comme [`EmissionFactors`](super::EmissionFactors)
//! (vérifiabilité, ADR-0023 §2, ADR-0024 §0).

use time::OffsetDateTime;

use crate::domain::{GenerationMix, Measurement, Region};

/// Prix spot day-ahead du marché de gros (zone de marché FR), en **€/MWh**, à un
/// horodatage donné (ADR-0023 §3). Source canonique : ENTSO-E.
///
/// La valeur **peut être négative** : les prix spot négatifs sont un phénomène de
/// marché réel (surproduction renouvelable). Aucune borne basse n'est imposée —
/// seule la finitude est vérifiée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpotPrice {
    pub at: OffsetDateTime,
    pub eur_per_mwh: f64,
}

impl SpotPrice {
    /// `None` si la valeur n'est pas finie (NaN/inf — donnée amont corrompue).
    pub fn new(at: OffsetDateTime, eur_per_mwh: f64) -> Option<Self> {
        if eur_per_mwh.is_finite() {
            Some(Self { at, eur_per_mwh })
        } else {
            None
        }
    }
}

/// Construction réglementée du TRV (empilement publié par la CRE), **versionnée**
/// par période de validité (millésime) — ADR-0023 §2.
///
/// Constante de domaine, **pas une dépendance IO** : les valeurs sont sourcées et
/// la donnée reste reproductible historiquement (clé = période de validité).
///
/// Valeurs **millésime 2026 sourcées** auprès de la CRE et du BOFiP (cf.
/// [`TrvReference::trv_2026`]). Les montants au MWh consommé visent un profil
/// résidentiel **BT ≤ 36 kVA, option Base, 6 kVA, ~2 400 kWh/an** (facture-type
/// CRE). Tout changement de valeur = **nouveau millésime** (jamais de mutation
/// silencieuse d'un millésime publié, ADR-0005/0019).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrvReference {
    /// Période de validité (millésime), ex. `"2026"`.
    pub vintage: &'static str,
    /// Acheminement (TURPE 7, fixé par la CRE) ramené au kWh consommé, en €/MWh.
    /// **Valeur dérivée** d'un barème €/an + c€/kWh (cf. [`TrvReference::trv_2026`]) :
    /// la conversion en €/MWh dépend du profil et de la ventilation horaire.
    pub turpe_eur_mwh: f64,
    /// Accise sur l'électricité (ex-TICFE/CSPE), en €/MWh — tarif normal ménages.
    pub accise_eur_mwh: f64,
    /// Composante commercialisation / **résidu** structurel de la construction
    /// réglementée du TRV, en €/MWh — pas un chiffre libre (ADR-0023 §2).
    pub commercialisation_eur_mwh: f64,
    /// Taux de TVA appliqué à la base de consommation. **20 % unique** sur toute
    /// la facture HT depuis la LF 2025 (le taux réduit 5,5 % sur l'abonnement a
    /// été supprimé) — BOFiP ACTU-2025-00057.
    pub tva_rate: f64,
}

impl TrvReference {
    /// Empilement TRV **millésime 2026** (Tarif Bleu, résidentiel BT ≤ 36 kVA,
    /// option Base, profil de référence 6 kVA / ~2 400 kWh/an). Valeurs sourcées.
    ///
    /// **Sources primaires** (consultées 2026-06-20) :
    /// - **TURPE 7 HTA-BT** — CRE délib. n°2025-78 (13/03/2025), grille en vigueur
    ///   au 1er août 2025 (applicable janv.–juil. 2026 ; +3,04 % au 1/8/2026).
    ///   Conversion : part fixe = gestion 16,80 + comptage Linky 22,00 +
    ///   6 kVA × 10,11 = **99,46 €/an** → 41,4 €/MWh à 2 400 kWh ; + part variable
    ///   réseau (plages CU4) ≈ 36 €/MWh ⇒ **≈ 78 €/MWh** (plage défendable 53–116
    ///   selon la ventilation horaire — confiance « moyenne » sur la conversion).
    /// - **Accise** = **30,85 €/MWh** (ménages, tarif normal, au 1/2/2026 ; base
    ///   25,19 + majoration de péréquation ZNI 5,66, facturée à tous) — CRE délib.
    ///   TRVE 2026 n°2026-06 (14/01/2026) + BOFiP BOI-RES-EAT-000240. Évolution :
    ///   33,70 (1/2/2025) → 29,98 (1/8/2025) → 30,85 (2026).
    /// - **Commercialisation** = **18,11 €/MWh HT** (résidentiel) — CRE délib.
    ///   n°2026-06 (empilement TRVE 2026).
    /// - **TVA** = **20 %** unique — BOFiP ACTU-2025-00057.
    ///
    /// ⚠️ Caveats : `turpe_eur_mwh` est une **conversion** dépendant du profil
    /// (6 kVA / 2 400 kWh retenus) ; au 2e semestre 2026 le TURPE est revalorisé
    /// (+3,04 %) et l'accise peut être réindexée — à re-millésimer le cas échéant.
    pub const fn trv_2026() -> Self {
        Self {
            vintage: "2026",
            turpe_eur_mwh: 78.0,
            accise_eur_mwh: 30.85,
            commercialisation_eur_mwh: 18.11,
            tva_rate: 0.20,
        }
    }
}

/// Une composante de la chaîne du prix payé (ADR-0023 §1-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceComponentKind {
    /// « Prix réel de l'énergie » = composante énergie spot (ADR-0023 §3).
    Energie,
    /// Acheminement (TURPE).
    Acheminement,
    /// Accise sur l'électricité (ex-TICFE/CSPE).
    Accise,
    /// Commercialisation / résidu structurel du TRV.
    Commercialisation,
    /// TVA appliquée à la base de consommation.
    Tva,
}

impl PriceComponentKind {
    pub fn slug(self) -> &'static str {
        match self {
            PriceComponentKind::Energie => "energie",
            PriceComponentKind::Acheminement => "acheminement",
            PriceComponentKind::Accise => "accise",
            PriceComponentKind::Commercialisation => "commercialisation",
            PriceComponentKind::Tva => "tva",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PriceComponentKind::Energie => "Énergie (prix de marché spot)",
            PriceComponentKind::Acheminement => "Acheminement (TURPE)",
            PriceComponentKind::Accise => "Accise sur l'électricité",
            PriceComponentKind::Commercialisation => "Commercialisation / résidu",
            PriceComponentKind::Tva => "TVA",
        }
    }

    /// Source / fondement réglementaire de la composante (ADR-0023 §5 : chaque
    /// composante porte sa source).
    pub fn source(self) -> &'static str {
        match self {
            PriceComponentKind::Energie => "Prix spot day-ahead — ENTSO-E Transparency Platform",
            PriceComponentKind::Acheminement => "TURPE (Commission de régulation de l'énergie)",
            PriceComponentKind::Accise => {
                "Accise sur l'électricité (ex-TICFE/CSPE), loi de finances"
            }
            PriceComponentKind::Commercialisation => "Résidu de la construction du TRV (CRE)",
            PriceComponentKind::Tva => "TVA (Code général des impôts)",
        }
    }
}

/// Montant d'une composante, en €/MWh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceComponent {
    pub kind: PriceComponentKind,
    pub amount_eur_mwh: f64,
}

/// Filière de production (contexte explicatif du prix, ADR-0023 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filiere {
    Nucleaire,
    Gaz,
    Charbon,
    Fioul,
    Hydraulique,
    Eolien,
    Solaire,
    Bioenergies,
    /// Thermique fossile agrégé (mix régional).
    Thermique,
}

impl Filiere {
    pub fn slug(self) -> &'static str {
        match self {
            Filiere::Nucleaire => "nucleaire",
            Filiere::Gaz => "gaz",
            Filiere::Charbon => "charbon",
            Filiere::Fioul => "fioul",
            Filiere::Hydraulique => "hydraulique",
            Filiere::Eolien => "eolien",
            Filiere::Solaire => "solaire",
            Filiere::Bioenergies => "bioenergies",
            Filiere::Thermique => "thermique",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Filiere::Nucleaire => "Nucléaire",
            Filiere::Gaz => "Gaz",
            Filiere::Charbon => "Charbon",
            Filiere::Fioul => "Fioul",
            Filiere::Hydraulique => "Hydraulique",
            Filiere::Eolien => "Éolien",
            Filiere::Solaire => "Solaire",
            Filiere::Bioenergies => "Bioénergies",
            Filiere::Thermique => "Thermique fossile",
        }
    }

    /// Rang de **coût marginal court terme** croissant (ordre de mérite
    /// approximatif). Les renouvelables et le nucléaire ont un coût marginal
    /// faible ; les fossiles élevé. Sert uniquement à **estimer** la filière
    /// marginale — c'est une heuristique, pas une donnée d'appel par centrale.
    fn merit_order(self) -> u8 {
        match self {
            Filiere::Solaire => 0,
            Filiere::Eolien => 1,
            Filiere::Hydraulique => 2,
            Filiere::Nucleaire => 3,
            Filiere::Bioenergies => 4,
            Filiere::Charbon => 6,
            // `Thermique` (fossile agrégé régional) et `Gaz` partagent le rang 7,
            // sans ambiguïté : `mix_shares` est mutuellement exclusif — il pousse
            // soit `Thermique` (mix régional), soit `Gaz`/`Charbon`/`Fioul`
            // (national détaillé), jamais les deux. L'égalité de rang est donc
            // sans effet sur le choix de la filière marginale.
            Filiere::Thermique => 7,
            Filiere::Gaz => 7,
            Filiere::Fioul => 8,
        }
    }
}

/// Part d'une filière dans la production domestique (contexte du prix).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixShare {
    pub filiere: Filiere,
    /// Part dans la production domestique, dans `[0, 1]`.
    pub share: f64,
    pub output_mw: f64,
}

/// **Estimation** de la technologie marginale qui fixe le prix spot (ADR-0023
/// §4). Jamais une mesure : la vraie filière marginale exige la donnée d'appel
/// par centrale, indisponible ici. Dérivée par ordre de mérite ([`Filiere::merit_order`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarginalTechnology {
    pub filiere: Filiere,
    /// Toujours `true` : valeur estimée, jamais mesurée.
    pub estimated: bool,
}

/// Contexte explicatif d'un prix (sans verdict, ADR-0023 §4).
#[derive(Debug, Clone, PartialEq)]
pub struct PriceContext {
    pub shares: Vec<MixShare>,
    /// `None` si la production domestique est nulle/indéterminée.
    pub marginal: Option<MarginalTechnology>,
}

/// Décomposition complète du prix payé à un horodatage (ADR-0023 §1).
#[derive(Debug, Clone, PartialEq)]
pub struct PriceBreakdown {
    pub at: OffsetDateTime,
    pub region: Region,
    /// Millésime de la construction réglementaire appliquée.
    pub vintage: &'static str,
    pub components: Vec<PriceComponent>,
    pub context: PriceContext,
}

impl PriceBreakdown {
    /// Total TTC (somme des composantes), en €/MWh.
    pub fn total_eur_mwh(&self) -> f64 {
        self.components.iter().map(|c| c.amount_eur_mwh).sum()
    }
}

/// Parts de production par filière, à partir d'un mix (productions négatives
/// bornées à 0 ; pompage et échanges exclus — pas des productions primaires).
fn mix_shares(mix: &GenerationMix) -> Vec<MixShare> {
    let mut entries = vec![
        (Filiere::Nucleaire, mix.nucleaire),
        (Filiere::Hydraulique, mix.hydraulique),
        (Filiere::Eolien, mix.eolien),
        (Filiere::Solaire, mix.solaire),
        (Filiere::Bioenergies, mix.bioenergies),
    ];
    match mix.thermique {
        Some(thermique) => entries.push((Filiere::Thermique, thermique)),
        None => {
            entries.push((Filiere::Gaz, mix.gaz));
            entries.push((Filiere::Charbon, mix.charbon));
            entries.push((Filiere::Fioul, mix.fioul));
        }
    }

    let total: f64 = entries.iter().map(|(_, mw)| mw.max(0.0)).sum();
    entries
        .into_iter()
        .filter_map(|(filiere, mw)| {
            let output_mw = mw.max(0.0);
            if output_mw <= 0.0 {
                return None;
            }
            let share = if total > 0.0 { output_mw / total } else { 0.0 };
            Some(MixShare {
                filiere,
                share,
                output_mw,
            })
        })
        .collect()
}

/// Estime la filière marginale : la filière en production la **plus coûteuse** au
/// sens de l'ordre de mérite, au-dessus d'un seuil de bruit (> 0,5 % de la
/// production ou 50 MW). `None` si la production est nulle.
fn estimate_marginal(shares: &[MixShare]) -> Option<MarginalTechnology> {
    let total: f64 = shares.iter().map(|s| s.output_mw).sum();
    if total <= 0.0 {
        return None;
    }
    let floor = (total * 0.005).max(50.0);
    shares
        .iter()
        .filter(|s| s.output_mw >= floor)
        .max_by_key(|s| s.filiere.merit_order())
        // Repli : à très basse production, aucune filière ne dépasse le plancher
        // absolu (50 MW) ; on prend alors la plus coûteuse en production. Contrat
        // tenu : `None` seulement si la production est nulle (garde ci-dessus).
        .or_else(|| shares.iter().max_by_key(|s| s.filiere.merit_order()))
        .map(|s| MarginalTechnology {
            filiere: s.filiere,
            estimated: true,
        })
}

/// Construit la décomposition complète du prix (ADR-0023 §1-4), à partir du prix
/// spot (énergie) et de la construction réglementaire versionnée. La TVA est
/// appliquée à la base de consommation `énergie + acheminement + accise +
/// commercialisation`.
pub fn price_breakdown(
    at: OffsetDateTime,
    region: Region,
    spot: &SpotPrice,
    mix: &GenerationMix,
    reference: &TrvReference,
) -> PriceBreakdown {
    let energie = spot.eur_per_mwh;
    let acheminement = reference.turpe_eur_mwh;
    let accise = reference.accise_eur_mwh;
    let commercialisation = reference.commercialisation_eur_mwh;
    let base_ht = energie + acheminement + accise + commercialisation;
    // Assiette de TVA **plancher à 0** : un prix spot très négatif (surproduction
    // renouvelable, observé < -100 €/MWh) doit rester visible dans la composante
    // énergie, mais ne peut pas produire une TVA négative (revue 2026-06-20). Le
    // total reste la somme honnête des composantes (énergie négative incluse).
    let tva = base_ht.max(0.0) * reference.tva_rate;

    let components = vec![
        PriceComponent {
            kind: PriceComponentKind::Energie,
            amount_eur_mwh: energie,
        },
        PriceComponent {
            kind: PriceComponentKind::Acheminement,
            amount_eur_mwh: acheminement,
        },
        PriceComponent {
            kind: PriceComponentKind::Accise,
            amount_eur_mwh: accise,
        },
        PriceComponent {
            kind: PriceComponentKind::Commercialisation,
            amount_eur_mwh: commercialisation,
        },
        PriceComponent {
            kind: PriceComponentKind::Tva,
            amount_eur_mwh: tva,
        },
    ];

    let shares = mix_shares(mix);
    let marginal = estimate_marginal(&shares);

    PriceBreakdown {
        at,
        region,
        vintage: reference.vintage,
        components,
        context: PriceContext { shares, marginal },
    }
}

/// Dérive la série de décompositions de prix en joignant chaque mesure (portant
/// un mix) au **prix spot le plus proche** (≤ son horodatage). `measurements` et
/// `spots` doivent être triés par horodatage croissant (jointure par fusion en
/// O(n+m)). Les mesures sans mix, ou sans prix spot antérieur disponible, sont
/// **omises** : la décomposition n'est définie que là où l'énergie spot existe.
pub fn price_series(
    measurements: &[Measurement],
    spots: &[SpotPrice],
    reference: &TrvReference,
) -> Vec<PriceBreakdown> {
    let mut out = Vec::new();
    let mut j = 0usize;
    let mut current: Option<&SpotPrice> = None;
    for m in measurements {
        let Some(mix) = m.mix.as_ref() else {
            continue;
        };
        while j < spots.len() && spots[j].at <= m.at {
            current = Some(&spots[j]);
            j += 1;
        }
        let Some(spot) = current else {
            continue;
        };
        out.push(price_breakdown(m.at, m.region, spot, mix, reference));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn national_mix() -> GenerationMix {
        GenerationMix {
            nucleaire: 38815.0,
            gaz: 666.0,
            charbon: 0.0,
            fioul: 34.0,
            hydraulique: 8893.0,
            eolien: 2555.0,
            solaire: 1050.0,
            bioenergies: 1006.0,
            pompage: -76.0,
            echanges: -11574.0,
            thermique: None,
        }
    }

    #[test]
    fn spot_rejects_non_finite_but_allows_negative() {
        let at = OffsetDateTime::UNIX_EPOCH;
        assert!(SpotPrice::new(at, f64::NAN).is_none());
        assert!(SpotPrice::new(at, -25.0).is_some(), "prix négatif valide");
    }

    #[test]
    fn breakdown_energy_component_is_the_spot_price() {
        let at = OffsetDateTime::UNIX_EPOCH;
        let spot = SpotPrice::new(at, 72.0).unwrap();
        let b = price_breakdown(
            at,
            Region::National,
            &spot,
            &national_mix(),
            &TrvReference::trv_2026(),
        );
        let energie = b
            .components
            .iter()
            .find(|c| c.kind == PriceComponentKind::Energie)
            .unwrap();
        assert_eq!(energie.amount_eur_mwh, 72.0);
    }

    #[test]
    fn tva_is_applied_on_the_consumption_base() {
        let at = OffsetDateTime::UNIX_EPOCH;
        let r = TrvReference::trv_2026();
        let spot = SpotPrice::new(at, 80.0).unwrap();
        let b = price_breakdown(at, Region::National, &spot, &national_mix(), &r);
        let base = 80.0 + r.turpe_eur_mwh + r.accise_eur_mwh + r.commercialisation_eur_mwh;
        let tva = b
            .components
            .iter()
            .find(|c| c.kind == PriceComponentKind::Tva)
            .unwrap();
        assert!((tva.amount_eur_mwh - base * r.tva_rate).abs() < 1e-9);
        assert!((b.total_eur_mwh() - (base + base * r.tva_rate)).abs() < 1e-9);
    }

    #[test]
    fn deeply_negative_spot_keeps_energy_visible_but_tva_nonnegative() {
        // Prix spot < -(turpe+accise+commercialisation) : la composante énergie
        // reste négative (factuelle), mais la TVA est plancher à 0 (revue 2026-06-20).
        let at = OffsetDateTime::UNIX_EPOCH;
        let spot = SpotPrice::new(at, -200.0).unwrap();
        let b = price_breakdown(
            at,
            Region::National,
            &spot,
            &national_mix(),
            &TrvReference::trv_2026(),
        );
        let energie = b
            .components
            .iter()
            .find(|c| c.kind == PriceComponentKind::Energie)
            .unwrap();
        let tva = b
            .components
            .iter()
            .find(|c| c.kind == PriceComponentKind::Tva)
            .unwrap();
        assert_eq!(energie.amount_eur_mwh, -200.0, "énergie négative visible");
        assert!(tva.amount_eur_mwh >= 0.0, "TVA jamais négative");
    }

    #[test]
    fn marginal_estimate_picks_most_expensive_running_filiere() {
        // Mix national : gaz (666 MW) en production → marginal estimé = gaz
        // (fioul à 34 MW sous le seuil de bruit).
        let shares = mix_shares(&national_mix());
        let marginal = estimate_marginal(&shares).unwrap();
        assert_eq!(marginal.filiere, Filiere::Gaz);
        assert!(marginal.estimated);
    }

    #[test]
    fn shares_exclude_pompage_and_sum_to_one() {
        let shares = mix_shares(&national_mix());
        let total: f64 = shares.iter().map(|s| s.share).sum();
        assert!((total - 1.0).abs() < 1e-9, "somme des parts = {total}");
        assert!(shares.iter().all(|s| s.filiere != Filiere::Charbon));
    }

    #[test]
    fn series_joins_nearest_prior_spot_and_omits_uncovered() {
        use crate::domain::{CarbonIntensity, Methodology, Vintage};
        use time::Duration;

        let t0 = OffsetDateTime::UNIX_EPOCH;
        let step = Duration::minutes(15);
        let measure = |i: i32| Measurement {
            at: t0 + step * i,
            region: Region::National,
            intensity: CarbonIntensity::new(40.0).unwrap(),
            methodology: Methodology::rte_direct(),
            vintage: Vintage::Consolidated,
            mix: Some(national_mix()),
        };
        let measurements = [measure(0), measure(1), measure(2)];

        // Un seul prix à t0+1 → couvre t1 et t2, PAS t0 (aucun prix antérieur).
        let spots = [SpotPrice::new(t0 + step, 55.0).unwrap()];

        let series = price_series(&measurements, &spots, &TrvReference::trv_2026());
        assert_eq!(series.len(), 2, "t0 omis (sans prix spot antérieur)");
        assert_eq!(series[0].at, t0 + step);
        let energie = series[0]
            .components
            .iter()
            .find(|c| c.kind == PriceComponentKind::Energie)
            .unwrap();
        assert_eq!(energie.amount_eur_mwh, 55.0);
    }
}
