//! Intensité carbone : la grandeur centrale du domaine.

/// Intensité carbone de l'électricité, en **gCO₂eq/kWh** (unité canonique).
///
/// Type *newtype* qui garantit l'invariant « valeur finie et positive » : une
/// intensité négative ou `NaN` n'a pas de sens physique. La construction passe
/// donc par [`CarbonIntensity::new`], qui renvoie `None` si l'invariant est
/// violé.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct CarbonIntensity(f64);

impl CarbonIntensity {
    /// Construit une intensité en gCO₂eq/kWh.
    ///
    /// Renvoie `None` si `g` n'est pas un réel fini positif ou nul.
    pub fn new(g: f64) -> Option<Self> {
        if g.is_finite() && g >= 0.0 {
            Some(Self(g))
        } else {
            None
        }
    }

    /// Valeur en gCO₂eq/kWh.
    pub fn value(self) -> f64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_negative_and_nan() {
        assert!(CarbonIntensity::new(-1.0).is_none());
        assert!(CarbonIntensity::new(f64::NAN).is_none());
        assert!(CarbonIntensity::new(f64::INFINITY).is_none());
    }

    #[test]
    fn accepts_zero_and_positive() {
        assert_eq!(CarbonIntensity::new(0.0).unwrap().value(), 0.0);
        assert_eq!(CarbonIntensity::new(42.0).unwrap().value(), 42.0);
    }
}
