//! Couche comparative **LCOE** (coût de production par filière) — ADR-0024.
//!
//! Cet ADR ne décrit pas une fonctionnalité, il décrit des **garde-fous**. La
//! neutralité ne se déclare pas, elle se construit dans la structure de donnée :
//!
//! - **Jamais un chiffre unique** : toujours une fourchette ([`LcoeRange`]
//!   min/médiane/max) restituant la **dispersion publiée par la source** (§1).
//!   Chaque filière est aujourd'hui mono-source (le multi-sources est un objectif
//!   de gouvernance, pas une propriété déjà tenue — cf. revue de neutralité).
//! - **Méthode et périmètre de première classe** : chaque estimation est clé par
//!   `source × technologie × périmètre × millésime` ([`CostReferenceKey`], §2).
//! - **Aucune soustraction / aucun « écart »** : ce module n'expose **aucune**
//!   opération mettant le LCOE et le prix de marché en différence (§3). C'est
//!   garanti structurellement : il n'existe pas de fonction qui combine les deux.
//! - **Statut « estimation » systématique** : toute valeur est une estimation
//!   sous hypothèses, jamais au même niveau qu'une mesure (§4).
//! - **Nucléaire scindé** existant amorti / nouveau, **jamais fusionnés** (§2).
//! - **Symétrie de périmètre** : le même périmètre est exposé pour toutes les
//!   filières, ou pour aucune (GATE, piège prioritaire) → ici [`Perimeter::Plateau`]
//!   uniforme.
//!
//! Domaine **pur** : table versionnée en constante de domaine, aucune IO (les
//! chiffres des rapports sont ré-encodés ici avec attribution, jamais reproduits
//! tels quels — ADR-0024 §risques).

/// Source autoritaire d'une estimation LCOE — triptyque public français
/// (ADR-0024 §5). Aucune source n'est privilégiée par défaut ; l'équilibre
/// méthodologique prime.
///
/// **Critère d'inclusion uniforme (revue 2026-06-20)** : on ré-encode des
/// *chiffres-faits* publiés tant que la licence **n'interdit pas** la réutilisation
/// commerciale. ADEME = Licence Ouverte (jeu réutilisable) ; Cour des comptes / RTE
/// = rapports d'institutions publiques, sans clause d'interdiction (chiffres
/// réutilisables comme faits). Sont **écartées** les sources dont la licence
/// **interdit explicitement** la réutilisation commerciale (AIE, CC-BY-NC) ou qui
/// sont entièrement propriétaires (Lazard) — motif *interdiction de licence*,
/// indépendant du résultat. La souveraineté (France-first) est une **préférence de
/// contexte**, jamais le motif disqualifiant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CostSource {
    /// ADEME — *Coûts des EnR&R en France* (renouvelables).
    Ademe,
    /// Cour des comptes — coûts du nucléaire **existant** (coût courant économique).
    CourDesComptes,
    /// RTE — *Futurs énergétiques 2050* (nouveau nucléaire + prospectif).
    Rte,
}

impl CostSource {
    pub fn slug(self) -> &'static str {
        match self {
            CostSource::Ademe => "ademe",
            CostSource::CourDesComptes => "cour-des-comptes",
            CostSource::Rte => "rte",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CostSource::Ademe => "ADEME",
            CostSource::CourDesComptes => "Cour des comptes",
            CostSource::Rte => "RTE",
        }
    }

    /// Attribution / référence de la source (réutilisation des **chiffres**, pas
    /// des tableaux ; licences à confirmer pour CdC/RTE — ADR-0024 §risques).
    pub fn attribution(self) -> &'static str {
        match self {
            CostSource::Ademe => {
                "ADEME, « Coûts des énergies renouvelables et de récupération en France » \
                 (Licence Ouverte / Etalab)"
            }
            CostSource::CourDesComptes => {
                "Cour des comptes, rapports sur les coûts de la filière nucléaire \
                 (document public ; licence formelle à confirmer)"
            }
            CostSource::Rte => {
                "RTE, « Futurs énergétiques 2050 » \
                 (données RTE largement en Licence Ouverte ; termes du rapport à confirmer)"
            }
        }
    }

    pub fn from_slug(slug: &str) -> Option<CostSource> {
        [
            CostSource::Ademe,
            CostSource::CourDesComptes,
            CostSource::Rte,
        ]
        .into_iter()
        .find(|s| s.slug() == slug)
    }
}

/// Technologie de production. Le **nucléaire est scindé** existant amorti /
/// nouveau (construction) : deux grandeurs distinctes, jamais fusionnées
/// (ADR-0024 §2 + GATE Bloc 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CostTechnology {
    /// Nucléaire **existant amorti** (parc en exploitation).
    NucleaireExistant,
    /// Nucléaire **nouveau** (construction, type EPR2).
    NucleaireNouveau,
    SolairePv,
    EolienTerrestre,
    EolienMer,
    Hydraulique,
    Biomasse,
}

impl CostTechnology {
    pub fn slug(self) -> &'static str {
        match self {
            CostTechnology::NucleaireExistant => "nucleaire-existant",
            CostTechnology::NucleaireNouveau => "nucleaire-nouveau",
            CostTechnology::SolairePv => "solaire-pv",
            CostTechnology::EolienTerrestre => "eolien-terrestre",
            CostTechnology::EolienMer => "eolien-mer",
            CostTechnology::Hydraulique => "hydraulique",
            CostTechnology::Biomasse => "biomasse",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CostTechnology::NucleaireExistant => "Nucléaire existant (amorti)",
            CostTechnology::NucleaireNouveau => "Nucléaire nouveau (EPR2)",
            CostTechnology::SolairePv => "Solaire photovoltaïque",
            CostTechnology::EolienTerrestre => "Éolien terrestre",
            CostTechnology::EolienMer => "Éolien en mer (posé)",
            CostTechnology::Hydraulique => "Hydraulique",
            CostTechnology::Biomasse => "Biomasse",
        }
    }

    pub fn from_slug(slug: &str) -> Option<CostTechnology> {
        [
            CostTechnology::NucleaireExistant,
            CostTechnology::NucleaireNouveau,
            CostTechnology::SolairePv,
            CostTechnology::EolienTerrestre,
            CostTechnology::EolienMer,
            CostTechnology::Hydraulique,
            CostTechnology::Biomasse,
        ]
        .into_iter()
        .find(|t| t.slug() == slug)
    }
}

/// Périmètre de coût. v1 = **plateau** (coûts au niveau de la centrale,
/// hors coûts système / externalités / back-up) appliqué **uniformément** à
/// toutes les filières — c'est le garde-fou de **symétrie de périmètre** : on
/// n'inclut jamais une dimension de coût (externalités, intermittence…) pour une
/// filière et pas pour les autres (ADR-0024 GATE, piège prioritaire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Perimeter {
    Plateau,
}

impl Perimeter {
    pub fn slug(self) -> &'static str {
        match self {
            Perimeter::Plateau => "plateau",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Perimeter::Plateau => {
                "Plateau — coûts au niveau de la centrale ; exclut les coûts système \
                 (back-up, réseau, stockage) et le démantèlement / les déchets de long terme ; \
                 non directement comparable entre filières pilotables et variables"
            }
        }
    }

    pub fn from_slug(slug: &str) -> Option<Perimeter> {
        [Perimeter::Plateau].into_iter().find(|p| p.slug() == slug)
    }
}

/// Fourchette LCOE (€/MWh) : `min ≤ median ≤ max`. Restituer la dispersion **est**
/// l'information (ADR-0024 §1). Jamais un point unique.
///
/// Invariant garanti à la construction (resserrement sur la médiane, sur le
/// modèle de [`ForecastPoint::new`](super::ForecastPoint) — une borne incohérente
/// ne casse pas la table).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LcoeRange {
    pub min: f64,
    pub median: f64,
    pub max: f64,
}

impl LcoeRange {
    pub fn new(min: f64, median: f64, max: f64) -> Self {
        let min = if min <= median { min } else { median };
        let max = if max >= median { max } else { median };
        Self { min, median, max }
    }
}

/// Hypothèses clés d'une estimation (ADR-0024 §2), quand disponibles. La présence
/// de ces dimensions est **uniforme** (même structure pour toutes les filières) ;
/// `None` signale une hypothèse non publiée, jamais une dimension retirée.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CostAssumptions {
    /// Taux d'actualisation (WACC), ex. `0.04`.
    pub discount_rate: Option<f64>,
    /// Durée de vie retenue (années).
    pub lifetime_years: Option<u32>,
    /// Facteur de charge, dans `[0, 1]`.
    pub load_factor: Option<f64>,
}

/// Nature de la grandeur (ADR-0024 GATE Bloc 1, revue 2026-06-20). Un **coût
/// comptable** d'un parc déjà amorti n'est **pas commensurable** à un **LCOE
/// prospectif** d'un moyen neuf : on les distingue explicitement pour ne pas
/// créer de fausse comparabilité sous un libellé uniforme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CostBasis {
    /// Coût comptable d'un parc **existant amorti** (ex. coût courant économique).
    AccountingAmortized,
    /// **LCOE prospectif** d'un moyen neuf (coût moyen actualisé ex-ante).
    ProspectiveLcoe,
}

impl CostBasis {
    pub fn slug(self) -> &'static str {
        match self {
            CostBasis::AccountingAmortized => "accounting-amortized",
            CostBasis::ProspectiveLcoe => "prospective-lcoe",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CostBasis::AccountingAmortized => "Coût comptable (parc existant amorti)",
            CostBasis::ProspectiveLcoe => "LCOE prospectif (moyen neuf)",
        }
    }
}

/// Clé d'une estimation : `source × technologie × périmètre × millésime`
/// (ADR-0024 §2). Le millésime = **année du rapport** d'où provient le chiffre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CostReferenceKey {
    pub source: CostSource,
    pub technology: CostTechnology,
    pub perimeter: Perimeter,
    pub vintage: u32,
}

/// Une estimation LCOE — **toujours** une estimation sous hypothèses, jamais une
/// mesure (ADR-0024 §4). Porte sa clé (provenance + périmètre + millésime), sa
/// **nature de grandeur** ([`CostBasis`]), sa fourchette et ses hypothèses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostEstimate {
    pub key: CostReferenceKey,
    pub basis: CostBasis,
    pub range: LcoeRange,
    pub assumptions: CostAssumptions,
}

/// Catalogue versionné des estimations LCOE. Lecture seule ; aucune opération de
/// comparaison au prix de marché n'est exposée (garantie de non-verdict, §3).
#[derive(Debug, Clone, PartialEq)]
pub struct CostReferenceCatalog {
    entries: Vec<CostEstimate>,
}

impl CostReferenceCatalog {
    pub fn entries(&self) -> &[CostEstimate] {
        &self.entries
    }

    /// Sous-ensemble filtré (filtres `None` = pas de contrainte). Ne réordonne
    /// pas et n'agrège pas : aucun classement par défaut suggérant un gagnant
    /// (ADR-0024 GATE Bloc 2).
    pub fn filtered(
        &self,
        source: Option<CostSource>,
        technology: Option<CostTechnology>,
        perimeter: Option<Perimeter>,
        vintage: Option<u32>,
    ) -> Vec<CostEstimate> {
        self.entries
            .iter()
            .copied()
            .filter(|e| source.is_none_or(|s| e.key.source == s))
            .filter(|e| technology.is_none_or(|t| e.key.technology == t))
            .filter(|e| perimeter.is_none_or(|p| e.key.perimeter == p))
            .filter(|e| vintage.is_none_or(|v| e.key.vintage == v))
            .collect()
    }
}

/// Note explicative **neutre obligatoire** (ADR-0024 §3). Neutralise la lecture
/// naïve « scandale » par l'explication du mécanisme, sans désigner de camp.
///
/// La formulation a été **révisée après la revue de neutralité du 2026-06-20**
/// (`docs/adr/0024-revue-neutralite.md`) : on ne revendique plus une « dispersion
/// entre experts » (chaque filière est mono-source : la fourchette est interne à
/// la source), on explicite ce que le périmètre « plateau » exclut, et on signale
/// l'hétérogénéité des millésimes.
pub const COST_REFERENCE_DISCLAIMER: &str = "Le LCOE (coût moyen actualisé de l'énergie) \
estime un coût moyen de production sur la durée de vie d'un moyen de production, sous hypothèses \
(taux d'actualisation, durée de vie, facteur de charge, périmètre). Le prix de marché (voir \
/v1/price) est un prix marginal de compensation horaire, fixé par la dernière unité appelée. Ces \
deux grandeurs sont de nature différente et ne sont pas censées être égales dans un marché à \
tarification marginale. carbon-fr ne calcule ni n'affiche aucun écart entre elles et ne formule \
aucun jugement. PORTÉE ET LIMITES : chaque valeur est une ESTIMATION sous hypothèses, issue d'UNE \
source citée ; la fourchette (min/médiane/max) restitue la dispersion publiée par cette source, \
non un désaccord inter-sources. Le périmètre « plateau » couvre les coûts au niveau de la centrale \
et exclut, de part et d'autre, les coûts système (back-up de l'intermittence, réseau, stockage) ET \
le démantèlement et la gestion de long terme des déchets : il n'est donc PAS directement comparable \
entre filières pilotables et variables. Les millésimes sont hétérogènes (nucléaire 2021, \
renouvelables 2024), à interpréter avec prudence. Une hypothèse à `null` signifie « non publiée par \
la source », jamais une dimension retirée.";

/// Catalogue LCOE **best-effort** (€/MWh, périmètre plateau uniforme).
///
/// ⚠️ Valeurs et millésimes **à confirmer/sourcer** dans les rapports cités
/// (re-vérification et re-millésimage = charge de gouvernance continue, ADR-0024
/// §conséquences). Chaque filière est **mono-source** : la fourchette est la
/// dispersion *publiée par la source*, pas un désaccord inter-sources (le
/// multi-sources par filière reste un objectif de gouvernance). Le nucléaire est
/// scindé existant (coût comptable amorti) / nouveau (LCOE prospectif) ; toutes
/// les filières partagent le périmètre plateau (cf. [`COST_REFERENCE_DISCLAIMER`]).
pub fn cost_reference_catalog() -> CostReferenceCatalog {
    let entries = vec![
        // — Nucléaire existant amorti (Cour des comptes) —
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::CourDesComptes,
                technology: CostTechnology::NucleaireExistant,
                perimeter: Perimeter::Plateau,
                vintage: 2021,
            },
            basis: CostBasis::AccountingAmortized,
            range: LcoeRange::new(49.0, 60.0, 75.0),
            assumptions: CostAssumptions {
                discount_rate: None,
                lifetime_years: Some(50),
                load_factor: Some(0.75),
            },
        },
        // — Nucléaire nouveau / EPR2 (RTE, Futurs énergétiques 2050) —
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Rte,
                technology: CostTechnology::NucleaireNouveau,
                perimeter: Perimeter::Plateau,
                vintage: 2021,
            },
            basis: CostBasis::ProspectiveLcoe,
            range: LcoeRange::new(100.0, 120.0, 150.0),
            assumptions: CostAssumptions {
                discount_rate: Some(0.04),
                lifetime_years: Some(60),
                load_factor: Some(0.65),
            },
        },
        // — Renouvelables (ADEME) —
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Ademe,
                technology: CostTechnology::SolairePv,
                perimeter: Perimeter::Plateau,
                vintage: 2024,
            },
            basis: CostBasis::ProspectiveLcoe,
            range: LcoeRange::new(45.0, 60.0, 90.0),
            assumptions: CostAssumptions {
                discount_rate: None,
                lifetime_years: Some(25),
                load_factor: Some(0.14),
            },
        },
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Ademe,
                technology: CostTechnology::EolienTerrestre,
                perimeter: Perimeter::Plateau,
                vintage: 2024,
            },
            basis: CostBasis::ProspectiveLcoe,
            range: LcoeRange::new(50.0, 65.0, 90.0),
            assumptions: CostAssumptions {
                discount_rate: None,
                lifetime_years: Some(25),
                load_factor: Some(0.25),
            },
        },
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Ademe,
                technology: CostTechnology::EolienMer,
                perimeter: Perimeter::Plateau,
                vintage: 2024,
            },
            basis: CostBasis::ProspectiveLcoe,
            range: LcoeRange::new(90.0, 110.0, 140.0),
            assumptions: CostAssumptions {
                discount_rate: None,
                lifetime_years: Some(25),
                load_factor: Some(0.40),
            },
        },
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Ademe,
                technology: CostTechnology::Hydraulique,
                perimeter: Perimeter::Plateau,
                vintage: 2024,
            },
            basis: CostBasis::ProspectiveLcoe,
            range: LcoeRange::new(15.0, 50.0, 100.0),
            assumptions: CostAssumptions {
                discount_rate: None,
                lifetime_years: Some(60),
                load_factor: Some(0.40),
            },
        },
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Ademe,
                technology: CostTechnology::Biomasse,
                perimeter: Perimeter::Plateau,
                vintage: 2024,
            },
            basis: CostBasis::ProspectiveLcoe,
            range: LcoeRange::new(90.0, 130.0, 200.0),
            assumptions: CostAssumptions {
                discount_rate: None,
                lifetime_years: Some(20),
                load_factor: Some(0.60),
            },
        },
    ];
    CostReferenceCatalog { entries }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_snaps_inconsistent_bounds_to_median() {
        let r = LcoeRange::new(80.0, 60.0, 40.0);
        assert_eq!(r.min, 60.0);
        assert_eq!(r.max, 60.0);
        assert!(r.min <= r.median && r.median <= r.max);
    }

    #[test]
    fn catalog_keeps_nuclear_split_existing_and_new() {
        let catalog = cost_reference_catalog();
        let techs: Vec<_> = catalog.entries().iter().map(|e| e.key.technology).collect();
        assert!(techs.contains(&CostTechnology::NucleaireExistant));
        assert!(techs.contains(&CostTechnology::NucleaireNouveau));
    }

    #[test]
    fn existing_nuclear_is_accounting_basis_not_prospective_lcoe() {
        // GATE 2026-06-20 : pas de fausse commensurabilité amorti vs prospectif.
        let catalog = cost_reference_catalog();
        for e in catalog.entries() {
            let expected = if e.key.technology == CostTechnology::NucleaireExistant {
                CostBasis::AccountingAmortized
            } else {
                CostBasis::ProspectiveLcoe
            };
            assert_eq!(e.basis, expected, "base incorrecte pour {:?}", e.key);
        }
    }

    #[test]
    fn every_entry_has_a_dispersion_and_uniform_perimeter() {
        // GATE Bloc 1 : dispersion par filière + symétrie de périmètre.
        let catalog = cost_reference_catalog();
        for e in catalog.entries() {
            assert!(e.range.min < e.range.max, "{:?} sans dispersion", e.key);
            assert_eq!(
                e.key.perimeter,
                Perimeter::Plateau,
                "périmètre non uniforme"
            );
        }
    }

    #[test]
    fn filter_does_not_reorder_or_aggregate() {
        let catalog = cost_reference_catalog();
        let ademe = catalog.filtered(Some(CostSource::Ademe), None, None, None);
        assert!(!ademe.is_empty());
        assert!(ademe.iter().all(|e| e.key.source == CostSource::Ademe));
    }
}
