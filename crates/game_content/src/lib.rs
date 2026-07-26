//! Stable authored vocabulary and coherent per-campaign content rules.
//!
//! This crate describes what may exist and generates the hidden truth for one
//! campaign. History decides what was actually made, learned, lost, and found;
//! gameplay materializes those facts without inventing a second ruleset.

use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FormulaId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MaterialKind {
    Iron,
    Silver,
    Oak,
    CleanLinen,
    Ashroot,
    RiverPearl,
    Moonmoss,
    SilverSalt,
    Emberseed,
    Sulfur,
    CopperSalt,
}

impl MaterialKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Iron => "iron",
            Self::Silver => "silver",
            Self::Oak => "oak",
            Self::CleanLinen => "clean linen",
            Self::Ashroot => "ashroot",
            Self::RiverPearl => "river pearl",
            Self::Moonmoss => "moonmoss",
            Self::SilverSalt => "silver salts",
            Self::Emberseed => "emberseed",
            Self::Sulfur => "sulfur",
            Self::CopperSalt => "copper salts",
        }
    }

    pub const fn source(self) -> MaterialSource {
        match self {
            Self::Iron | Self::Silver | Self::Sulfur | Self::CopperSalt | Self::SilverSalt => {
                MaterialSource::Mineral
            }
            Self::Oak => MaterialSource::Timber,
            Self::CleanLinen => MaterialSource::Crafted,
            Self::Ashroot | Self::Moonmoss | Self::Emberseed => MaterialSource::Plant,
            Self::RiverPearl => MaterialSource::River,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaterialSource {
    Mineral,
    Timber,
    Crafted,
    Plant,
    River,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemForm {
    Longsword,
    HuntingBow,
    Grimoire,
    RitualVessel,
    Reagent,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MagicPrinciple {
    Heat,
    Life,
    Memory,
    Light,
    Distance,
    Decay,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MagicEffect {
    Heal,
    Kindle,
}

impl MagicEffect {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Heal => "healing",
            Self::Kindle => "kindling flame",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormulaCondition {
    CleanWater,
    DirectSunlight,
    NightSky,
    ExistingFlame,
}

impl FormulaCondition {
    pub const fn name(self) -> &'static str {
        match self {
            Self::CleanWater => "beside clean water",
            Self::DirectSunlight => "under direct sunlight",
            Self::NightSky => "beneath the night sky",
            Self::ExistingFlame => "beside an existing flame",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormulaRule {
    pub id: FormulaId,
    pub name: String,
    pub effect: MagicEffect,
    pub principles: Vec<MagicPrinciple>,
    pub reagents: Vec<MaterialKind>,
    pub condition: FormulaCondition,
    pub potency: u8,
}

impl FormulaRule {
    pub fn is_coherent(&self) -> bool {
        !self.principles.is_empty()
            && !self.reagents.is_empty()
            && self.reagents.iter().copied().collect::<BTreeSet<_>>().len() == self.reagents.len()
            && self.potency > 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CraftRule {
    pub form: ItemForm,
    pub materials: Vec<MaterialKind>,
    pub required_trade: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldRules {
    pub campaign_seed: u64,
    pub magical_tradition: String,
    pub formulas: Vec<FormulaRule>,
    pub crafts: Vec<CraftRule>,
}

impl WorldRules {
    pub fn generate(campaign_seed: u64) -> Self {
        let traditions = [
            "The River Covenant",
            "The Ashen Measure",
            "The Lantern Discipline",
            "The Doctrine of Remembered Names",
        ];
        let healing_variants = [
            (
                [MagicPrinciple::Life, MagicPrinciple::Memory],
                [MaterialKind::Ashroot, MaterialKind::RiverPearl],
                FormulaCondition::CleanWater,
                "Rite of the Returning Current",
            ),
            (
                [MagicPrinciple::Life, MagicPrinciple::Light],
                [MaterialKind::Moonmoss, MaterialKind::SilverSalt],
                FormulaCondition::NightSky,
                "Vigil of the Pale Lantern",
            ),
            (
                [MagicPrinciple::Life, MagicPrinciple::Heat],
                [MaterialKind::Emberseed, MaterialKind::CleanLinen],
                FormulaCondition::ExistingFlame,
                "Mercy of the Banked Hearth",
            ),
        ];
        let flame_variants = [
            (
                [MaterialKind::Sulfur, MaterialKind::CopperSalt],
                FormulaCondition::DirectSunlight,
                "Copper-Sun Invocation",
            ),
            (
                [MaterialKind::Emberseed, MaterialKind::Ashroot],
                FormulaCondition::ExistingFlame,
                "Canticle of the Second Ember",
            ),
            (
                [MaterialKind::Moonmoss, MaterialKind::Sulfur],
                FormulaCondition::NightSky,
                "Cold Star Kindling",
            ),
        ];
        let healing = &healing_variants[(campaign_seed as usize) % healing_variants.len()];
        let flame =
            &flame_variants[((campaign_seed.rotate_left(17)) as usize) % flame_variants.len()];

        Self {
            campaign_seed,
            magical_tradition: traditions[(campaign_seed as usize) % traditions.len()].to_string(),
            formulas: vec![
                FormulaRule {
                    id: FormulaId(1),
                    name: healing.3.to_string(),
                    effect: MagicEffect::Heal,
                    principles: healing.0.to_vec(),
                    reagents: healing.1.to_vec(),
                    condition: healing.2,
                    potency: 5,
                },
                FormulaRule {
                    id: FormulaId(2),
                    name: flame.2.to_string(),
                    effect: MagicEffect::Kindle,
                    principles: vec![MagicPrinciple::Heat, MagicPrinciple::Light],
                    reagents: flame.0.to_vec(),
                    condition: flame.1,
                    potency: 4,
                },
            ],
            crafts: vec![
                CraftRule {
                    form: ItemForm::Longsword,
                    materials: vec![MaterialKind::Iron, MaterialKind::Oak],
                    required_trade: "smith",
                },
                CraftRule {
                    form: ItemForm::Grimoire,
                    materials: vec![MaterialKind::CleanLinen],
                    required_trade: "scribe",
                },
            ],
        }
    }

    pub fn formula(&self, id: FormulaId) -> Option<&FormulaRule> {
        self.formulas.iter().find(|formula| formula.id == id)
    }

    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut ids = BTreeSet::new();
        for formula in &self.formulas {
            if !ids.insert(formula.id) {
                problems.push(format!("duplicate formula {}", formula.id.0));
            }
            if !formula.is_coherent() {
                problems.push(format!("incoherent formula {}", formula.id.0));
            }
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_rules_are_deterministic_and_solvable() {
        let first = WorldRules::generate(77);
        let second = WorldRules::generate(77);

        assert_eq!(first, second);
        assert!(first.validate().is_empty());
        assert!(
            first
                .formulas
                .iter()
                .any(|rule| rule.effect == MagicEffect::Heal)
        );
        assert!(
            first
                .formulas
                .iter()
                .any(|rule| rule.effect == MagicEffect::Kindle)
        );
    }

    #[test]
    fn campaign_seeds_change_rules_without_changing_effect_vocabulary() {
        let first = WorldRules::generate(1);
        let second = WorldRules::generate(2);

        assert_ne!(first.formulas[0].reagents, second.formulas[0].reagents);
        assert_eq!(first.formulas[0].effect, second.formulas[0].effect);
    }
}
