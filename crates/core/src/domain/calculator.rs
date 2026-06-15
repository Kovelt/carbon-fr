//! Stratégie de calcul d'une méthodologie (ADR-0010 §4).
//!
//! Une méthodologie est une **fonction pure** `(mix, contexte d'import,
//! facteurs) → intensité`, exprimée derrière le trait [`MethodologyCalculator`].
//! Aucune IO : les implémentations sont testables avec des données en mémoire,
//! et le choix de méthode est un *dispatch* (statique ou dynamique) côté
//! adapter, sans toucher au domaine.

use crate::domain::{
    CarbonIntensity, CrossBorderFlows, EmissionFactors, GenerationMix, Methodology,
    acv_ademe_consumption_intensity, acv_ademe_intensity,
};

/// Contexte de calcul fourni au moment de dériver l'intensité d'une méthode.
///
/// Regroupe tout ce dont une méthode *peut* avoir besoin ; chaque méthode pioche
/// ce qui la concerne (`rte-direct` n'utilise que `published`, la consommation
/// a besoin de `cross_border`).
#[derive(Debug, Clone, Copy)]
pub struct MethodologyContext<'a> {
    /// Mix de production au pas de la mesure.
    pub mix: &'a GenerationMix,
    /// Contexte d'import (flux signés + intensités voisines), si disponible.
    pub cross_border: Option<&'a CrossBorderFlows>,
    /// Table de facteurs d'émission cycle de vie.
    pub factors: &'a EmissionFactors,
    /// Facteur de pertes T&D (uplift consommation).
    pub td_loss: f64,
    /// Intensité **publiée par la source** (RTE `taux_co2`), si disponible —
    /// `rte-direct` est un report, pas un calcul.
    pub published: Option<CarbonIntensity>,
}

/// Stratégie de calcul d'une méthodologie carbone (ADR-0010 §4).
pub trait MethodologyCalculator {
    /// Identité versionnée de la méthode produite.
    fn methodology(&self) -> Methodology;

    /// Dérive l'intensité pour cette méthode à partir du contexte. `None` si la
    /// donnée nécessaire manque (ex. pas de contexte d'import pour la
    /// consommation, production nulle…).
    fn intensity(&self, ctx: &MethodologyContext<'_>) -> Option<CarbonIntensity>;
}

/// `rte-direct` : report de l'estimation publiée par RTE (ADR-0005). Non
/// dérivable du mix — c'est la valeur source telle quelle.
#[derive(Debug, Clone, Copy, Default)]
pub struct RteDirect;

impl MethodologyCalculator for RteDirect {
    fn methodology(&self) -> Methodology {
        Methodology::rte_direct()
    }

    fn intensity(&self, ctx: &MethodologyContext<'_>) -> Option<CarbonIntensity> {
        ctx.published
    }
}

/// `acv-ademe@1` : cycle de vie de la **production** française (ADR-0008).
#[derive(Debug, Clone, Copy, Default)]
pub struct AcvAdemeProduction;

impl MethodologyCalculator for AcvAdemeProduction {
    fn methodology(&self) -> Methodology {
        Methodology::acv_ademe()
    }

    fn intensity(&self, ctx: &MethodologyContext<'_>) -> Option<CarbonIntensity> {
        acv_ademe_intensity(ctx.mix, ctx.factors)
    }
}

/// `acv-ademe@2` : cycle de vie *consumption-based*, imports valorisés à
/// l'intensité du voisin + pertes T&D (ADR-0010). Requiert le contexte d'import.
#[derive(Debug, Clone, Copy, Default)]
pub struct AcvAdemeConsumption;

impl MethodologyCalculator for AcvAdemeConsumption {
    fn methodology(&self) -> Methodology {
        Methodology::acv_ademe_consumption()
    }

    fn intensity(&self, ctx: &MethodologyContext<'_>) -> Option<CarbonIntensity> {
        let flows = ctx.cross_border?;
        acv_ademe_consumption_intensity(ctx.mix, flows, ctx.factors, ctx.td_loss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CrossBorderFlow, Neighbor};

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

    fn ctx<'a>(
        mix: &'a GenerationMix,
        flows: Option<&'a CrossBorderFlows>,
        factors: &'a EmissionFactors,
        published: Option<CarbonIntensity>,
    ) -> MethodologyContext<'a> {
        MethodologyContext {
            mix,
            cross_border: flows,
            factors,
            td_loss: 0.0,
            published,
        }
    }

    #[test]
    fn rte_direct_reports_published_value() {
        let mix = national_mix();
        let factors = EmissionFactors::acv_ademe_v1();
        let published = CarbonIntensity::new(21.0);
        let got = RteDirect.intensity(&ctx(&mix, None, &factors, published));
        assert_eq!(got, published);
        // Sans valeur publiée → indéfini (rte-direct ne se calcule pas).
        assert!(
            RteDirect
                .intensity(&ctx(&mix, None, &factors, None))
                .is_none()
        );
    }

    #[test]
    fn production_matches_free_function() {
        let mix = national_mix();
        let factors = EmissionFactors::acv_ademe_v1();
        let via_trait = AcvAdemeProduction
            .intensity(&ctx(&mix, None, &factors, None))
            .unwrap();
        let direct = acv_ademe_intensity(&mix, &factors).unwrap();
        assert_eq!(via_trait.value(), direct.value());
    }

    #[test]
    fn consumption_needs_cross_border_context() {
        let mix = national_mix();
        let factors = EmissionFactors::acv_ademe_v1();
        // Sans contexte d'import → indéfini.
        assert!(
            AcvAdemeConsumption
                .intensity(&ctx(&mix, None, &factors, None))
                .is_none()
        );
    }

    #[test]
    fn carbon_import_raises_intensity_above_production() {
        let mix = national_mix();
        let factors = EmissionFactors::acv_ademe_v1();
        let production = acv_ademe_intensity(&mix, &factors).unwrap();

        // Un import carboné (Allemagne, mix charbon/gaz) doit relever l'intensité
        // consommée au-dessus de la seule production très bas-carbone.
        let flows = CrossBorderFlows::new(vec![CrossBorderFlow {
            neighbor: Neighbor::Germany,
            flow_mw: 4000.0,
            neighbor_intensity: CarbonIntensity::new(400.0).unwrap(),
        }]);
        let consumption = AcvAdemeConsumption
            .intensity(&ctx(&mix, Some(&flows), &factors, None))
            .unwrap();
        assert!(
            consumption.value() > production.value(),
            "conso {} devrait dépasser prod {}",
            consumption.value(),
            production.value()
        );
    }

    #[test]
    fn net_export_keeps_intensity_near_production() {
        let mix = national_mix();
        let factors = EmissionFactors::acv_ademe_v1();
        let production = acv_ademe_intensity(&mix, &factors).unwrap();

        // Export pur : la conso reflète la production domestique (hors pertes).
        let flows = CrossBorderFlows::new(vec![CrossBorderFlow {
            neighbor: Neighbor::GreatBritain,
            flow_mw: -3000.0,
            neighbor_intensity: CarbonIntensity::new(250.0).unwrap(),
        }]);
        let consumption = AcvAdemeConsumption
            .intensity(&ctx(&mix, Some(&flows), &factors, None))
            .unwrap();
        assert!(
            (consumption.value() - production.value()).abs() < 0.01,
            "conso {} devrait égaler prod {} (export à l'intensité de prod)",
            consumption.value(),
            production.value()
        );
    }

    #[test]
    fn td_losses_uplift_intensity() {
        let mix = national_mix();
        let factors = EmissionFactors::acv_ademe_v1();
        let flows = CrossBorderFlows::new(vec![CrossBorderFlow {
            neighbor: Neighbor::Germany,
            flow_mw: 2000.0,
            neighbor_intensity: CarbonIntensity::new(400.0).unwrap(),
        }]);
        let no_loss = MethodologyContext {
            td_loss: 0.0,
            ..ctx(&mix, Some(&flows), &factors, None)
        };
        let with_loss = MethodologyContext {
            td_loss: 0.072,
            ..ctx(&mix, Some(&flows), &factors, None)
        };
        let base = AcvAdemeConsumption.intensity(&no_loss).unwrap().value();
        let uplifted = AcvAdemeConsumption.intensity(&with_loss).unwrap().value();
        assert!((uplifted - base * 1.072).abs() < 1e-6);
    }
}
