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

/// Source autoritaire d'une estimation LCOE. Aucune source n'est privilégiée par
/// défaut ; l'équilibre méthodologique prime. **Multi-sources par filière** depuis
/// la recherche du 2026-06-20 (la fourchette devient une **dispersion inter-sources**).
///
/// **Critère d'inclusion & fondement de réutilisation (recherche licences
/// 2026-06-20).** On ne réutilise que des *chiffres-faits* — **non protégés par le
/// droit d'auteur** (CPI L112-1, dichotomie idée/expression) — **ré-encodés** dans
/// la structure propre [`CostEstimate`], jamais les tableaux/figures/texte des
/// rapports, et en **petit nombre** par filière (≠ extraction substantielle d'une
/// base, CPI L341-1/L342-3). Fondement **par source** :
/// - **ADEME** = **Licence Ouverte / Etalab 2.0** (réutilisation commerciale
///   explicitement permise avec attribution) — confiance haute.
/// - **Cour des comptes** = pas de licence ouverte nommée sur `ccomptes.fr`, mais
///   conditions du site **sans clause non commerciale** + **CRPA art. L321-1** et s.
///   — confiance moyenne.
/// - **RTE** = mentions légales du **rapport restrictives** → réutilisation des
///   chiffres fondée **uniquement** sur « faits non protégés + extraction non
///   substantielle », **PAS** sur une Licence Ouverte du rapport — confiance
///   moyenne, **risque résiduel réel**.
/// - **IRENA** = licence maison **permissive** (« material may be freely used,
///   shared, copied, reproduced … provided that appropriate acknowledgement is
///   given », **sans clause NC**, pas du Creative Commons) → réutilisation y
///   compris commerciale — confiance haute. LCOE **mondiaux** (≠ France : Chine
///   tire les moyennes vers le bas — c'est la dispersion recherchée, pas une erreur).
/// - **CRE** = autorité administrative ; réutilisation des informations publiques
///   (**CRPA art. L321-1**), sans clause NC — confiance haute.
///
/// Sont **écartées** les sources dont la licence **interdit** le commercial — motif
/// *licence*, uniforme, indépendant du résultat (toutes vérifiées le 2026-06-20) :
/// **AIE/IEA** et **GIEC/IPCC AR6** (CC BY-NC), **Fraunhofer ISE** (clause NC
/// explicite), **NEA/OCDE** (licence restrictive), **Lazard** (propriétaire). La
/// souveraineté (France-first) est une **préférence de contexte**, jamais le motif
/// disqualifiant. ⚠️ Recherche best-effort, **pas un avis juridique** ; pour un
/// **palier payant** s'appuyant sur la donnée RTE, une confirmation écrite de RTE
/// est recommandée (ADR-0024 §risques + revue de neutralité).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CostSource {
    /// ADEME — *Coûts des EnR&R en France* (renouvelables, France).
    Ademe,
    /// Cour des comptes — coûts du nucléaire **existant** (coût courant économique).
    CourDesComptes,
    /// RTE — *Futurs énergétiques 2050* (nouveau nucléaire + prospectif).
    Rte,
    /// IRENA — *Renewable Power Generation Costs in 2024* (LCOE **mondiaux** EnR).
    Irena,
    /// CRE — coûts du **nucléaire existant** (coût complet du parc).
    Cre,
}

impl CostSource {
    pub fn slug(self) -> &'static str {
        match self {
            CostSource::Ademe => "ademe",
            CostSource::CourDesComptes => "cour-des-comptes",
            CostSource::Rte => "rte",
            CostSource::Irena => "irena",
            CostSource::Cre => "cre",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CostSource::Ademe => "ADEME",
            CostSource::CourDesComptes => "Cour des comptes",
            CostSource::Rte => "RTE",
            CostSource::Irena => "IRENA",
            CostSource::Cre => "CRE",
        }
    }

    /// Attribution / référence de la source. **Seuls des chiffres-faits sont
    /// ré-encodés** (jamais les tableaux/figures/texte) ; le fondement de
    /// réutilisation diffère par source (cf. doc de [`CostSource`]).
    pub fn attribution(self) -> &'static str {
        match self {
            CostSource::Ademe => {
                "ADEME, « Coûts des énergies renouvelables et de récupération en France » \
                 (Licence Ouverte / Etalab 2.0) — chiffres-faits ré-encodés par carbon-fr"
            }
            CostSource::CourDesComptes => {
                "Cour des comptes, rapports sur les coûts de la filière nucléaire \
                 (www.ccomptes.fr) — chiffres-faits ré-encodés ; réutilisation au titre du \
                 CRPA art. L321-1, source citée, sens non altéré"
            }
            CostSource::Rte => {
                "RTE, « Futurs énergétiques 2050 » (Bilan prévisionnel, rte-france.com) — \
                 chiffres-faits ré-encodés ; valeur issue du rapport (mentions légales \
                 restrictives), non d'un jeu sous Licence Ouverte ; source citée, sens non altéré"
            }
            CostSource::Irena => {
                "IRENA (2025), « Renewable power generation costs in 2024 », Abu Dhabi — \
                 LCOE MONDIAUX (moyennes pondérées + valeurs pays), USD 2024 convertis à \
                 0,9243 EUR/USD (moyenne annuelle 2024) ; licence IRENA permissive (sans clause \
                 NC), réutilisation y compris commerciale avec attribution ; chiffres-faits ré-encodés"
            }
            CostSource::Cre => {
                "CRE, rapport sur les coûts du nucléaire existant (www.cre.fr) — coût complet \
                 du parc, chiffres-faits ré-encodés ; réutilisation des informations publiques \
                 (CRPA art. L321-1), source citée, sens non altéré"
            }
        }
    }

    /// Périmètre géographique des chiffres de la source — `"france"` ou `"monde"`
    /// (GATE re-jeu n°3, 2026-06-20) : IRENA publie des LCOE **mondiaux**, souvent
    /// plus bas que les sources France. Exposer la géographie rend l'asymétrie
    /// **lisible par machine**, pour ne pas lire un plancher mondial comme un coût
    /// français.
    pub fn geography(self) -> &'static str {
        match self {
            CostSource::Ademe | CostSource::CourDesComptes | CostSource::Rte | CostSource::Cre => {
                "france"
            }
            CostSource::Irena => "monde",
        }
    }

    pub fn from_slug(slug: &str) -> Option<CostSource> {
        [
            CostSource::Ademe,
            CostSource::CourDesComptes,
            CostSource::Rte,
            CostSource::Irena,
            CostSource::Cre,
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
/// (`docs/adr/0024-revue-neutralite.md`) puis après l'**ajout de 2e sources** par
/// filière : on explicite que la couche est désormais **multi-sources** (la
/// dispersion mêle écart intra-source et inter-sources), ce que le périmètre
/// « plateau » exclut, et l'hétérogénéité des millésimes et des géographies.
pub const COST_REFERENCE_DISCLAIMER: &str = "Le LCOE (coût moyen actualisé de l'énergie) \
estime un coût moyen de production sur la durée de vie d'un moyen de production, sous hypothèses \
(taux d'actualisation, durée de vie, facteur de charge, périmètre). Le prix de marché (voir \
/v1/price) est un prix marginal de compensation horaire, fixé par la dernière unité appelée. Ces \
deux grandeurs sont de nature différente et ne sont pas censées être égales dans un marché à \
tarification marginale. carbon-fr ne calcule ni n'affiche aucun écart entre elles et ne formule \
aucun jugement. PORTÉE ET LIMITES : chaque valeur est une ESTIMATION sous hypothèses, issue d'une \
source citée. La plupart des filières ont DEUX sources (ex. ADEME + IRENA, Cour des comptes + CRE) : \
la fourchette affichée mêle alors la dispersion interne à une source ET l'écart entre sources. \
Précautions : les chiffres IRENA sont des LCOE MONDIAUX (souvent plus bas que les sources France — \
c'est la dispersion réelle, pas une erreur) ; certaines 2e sources ne fournissent qu'un point \
(min = max), pas une fourchette propre ; le nucléaire nouveau reste à une seule source (aucune 2e \
source primaire licence-compatible disponible). Le périmètre « plateau » couvre les coûts au niveau \
de la centrale et exclut, de part et d'autre, les coûts système (back-up de l'intermittence, réseau, \
stockage) ET le démantèlement et la gestion de long terme des déchets : il n'est donc PAS directement \
comparable entre filières pilotables et variables. Les millésimes sont hétérogènes (2021 à 2024), à \
interpréter avec prudence. Une hypothèse à `null` signifie « non publiée par la source », jamais une \
dimension retirée.";

/// Catalogue LCOE **best-effort** (€/MWh, périmètre plateau uniforme).
///
/// ⚠️ Valeurs et millésimes **à confirmer/sourcer** dans les rapports cités
/// (re-vérification et re-millésimage = charge de gouvernance continue, ADR-0024
/// §conséquences). **Multi-sources** depuis 2026-06-20 : la plupart des filières
/// ont 2 sources (ADEME + IRENA pour les renouvelables ; Cour des comptes + CRE
/// pour le nucléaire existant) → la fourchette mêle dispersion intra-source et
/// inter-sources. Le **nucléaire nouveau reste mono-source** (RTE) faute de 2e
/// source primaire licence-compatible (IPCC/NEA/IEA écartés pour clause NC). Le
/// nucléaire est scindé existant (coût comptable amorti) / nouveau (LCOE
/// prospectif) ; toutes les filières partagent le périmètre plateau (cf.
/// [`COST_REFERENCE_DISCLAIMER`]).
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
        // ─── 2e sources par filière (dispersion INTER-sources, recherche 2026-06-20) ───
        // CRE = 2e source pour le nucléaire existant (coût complet du parc, vraie
        // fourchette). IRENA = 2e source pour les 5 renouvelables (LCOE MONDIAUX,
        // USD 2024 → EUR à 0,9243 ; valeurs plus basses que l'ADEME France = la
        // dispersion recherchée, pas une erreur). Hypothèses non ré-encodées
        // (résumés sans valeurs exploitables) → `default()` (tout `None`).
        // Le nucléaire NOUVEAU reste mono-source (RTE) : aucune 2e source primaire
        // licence-compatible trouvée (IPCC/NEA/IEA écartés pour clause NC ; chiffre
        // EPR2 Cour des comptes non confirmé en table primaire) — asymétrie de
        // *disponibilité de licence*, pas de résultat (documentée, ADR-0024).
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Cre,
                technology: CostTechnology::NucleaireExistant,
                perimeter: Perimeter::Plateau,
                vintage: 2023,
            },
            basis: CostBasis::AccountingAmortized,
            range: LcoeRange::new(57.3, 60.7, 63.4),
            assumptions: CostAssumptions::default(),
        },
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Irena,
                technology: CostTechnology::SolairePv,
                perimeter: Perimeter::Plateau,
                vintage: 2024,
            },
            basis: CostBasis::ProspectiveLcoe,
            range: LcoeRange::new(30.5, 39.7, 40.0),
            assumptions: CostAssumptions::default(),
        },
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Irena,
                technology: CostTechnology::EolienTerrestre,
                perimeter: Perimeter::Plateau,
                vintage: 2024,
            },
            basis: CostBasis::ProspectiveLcoe,
            range: LcoeRange::new(26.8, 31.4, 48.1),
            assumptions: CostAssumptions::default(),
        },
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Irena,
                technology: CostTechnology::EolienMer,
                perimeter: Perimeter::Plateau,
                vintage: 2024,
            },
            basis: CostBasis::ProspectiveLcoe,
            range: LcoeRange::new(51.8, 73.0, 74.0),
            assumptions: CostAssumptions::default(),
        },
        // Hydraulique & biomasse : IRENA ne publie qu'une moyenne mondiale (point
        // unique, min = max) dans le résumé — la 2e source ajoute un point, pas une
        // fourchette propre (la dispersion par filière reste portée par l'ADEME).
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Irena,
                technology: CostTechnology::Hydraulique,
                perimeter: Perimeter::Plateau,
                vintage: 2024,
            },
            basis: CostBasis::ProspectiveLcoe,
            range: LcoeRange::new(52.7, 52.7, 52.7),
            assumptions: CostAssumptions::default(),
        },
        CostEstimate {
            key: CostReferenceKey {
                source: CostSource::Irena,
                technology: CostTechnology::Biomasse,
                perimeter: Perimeter::Plateau,
                vintage: 2024,
            },
            basis: CostBasis::ProspectiveLcoe,
            range: LcoeRange::new(80.4, 80.4, 80.4),
            assumptions: CostAssumptions::default(),
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

    const ALL_TECHS: [CostTechnology; 7] = [
        CostTechnology::NucleaireExistant,
        CostTechnology::NucleaireNouveau,
        CostTechnology::SolairePv,
        CostTechnology::EolienTerrestre,
        CostTechnology::EolienMer,
        CostTechnology::Hydraulique,
        CostTechnology::Biomasse,
    ];

    #[test]
    fn ranges_consistent_uniform_perimeter_and_each_filiere_dispersed() {
        // GATE Bloc 1 : périmètre symétrique + au moins une source à vraie
        // fourchette par filière. On tolère `min == max` au niveau entrée (certaines
        // 2e sources, ex. IRENA hydro/biomasse, ne publient qu'un point).
        let catalog = cost_reference_catalog();
        for e in catalog.entries() {
            assert!(
                e.range.min <= e.range.max,
                "{:?} : borne incohérente",
                e.key
            );
            assert_eq!(
                e.key.perimeter,
                Perimeter::Plateau,
                "périmètre non uniforme"
            );
        }
        for tech in ALL_TECHS {
            assert!(
                catalog
                    .entries()
                    .iter()
                    .any(|e| e.key.technology == tech && e.range.min < e.range.max),
                "aucune source dispersée pour {tech:?}"
            );
        }
    }

    #[test]
    fn renewables_and_existing_nuclear_are_multi_source() {
        // Dispersion INTER-sources : ≥ 2 sources distinctes par filière, sauf le
        // nucléaire nouveau (mono-source assumé, faute de 2e source primaire
        // licence-compatible — IPCC/NEA/IEA écartés pour clause NC).
        let catalog = cost_reference_catalog();
        let distinct_sources = |tech: CostTechnology| -> usize {
            catalog
                .entries()
                .iter()
                .filter(|e| e.key.technology == tech)
                .map(|e| e.key.source)
                .collect::<std::collections::HashSet<_>>()
                .len()
        };
        for tech in ALL_TECHS {
            let n = distinct_sources(tech);
            if tech == CostTechnology::NucleaireNouveau {
                assert_eq!(n, 1, "nucléaire nouveau attendu mono-source (assumé)");
            } else {
                assert!(n >= 2, "{tech:?} devrait avoir ≥ 2 sources, a {n}");
            }
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
