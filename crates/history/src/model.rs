use std::collections::{BTreeMap, BTreeSet};

use ultimate_fate_content::{FormulaId, ItemForm, MaterialKind, ObjectId};
use ultimate_fate_world_atlas::AtlasPosition;

use crate::ids::{
    ClaimId, EventId, FactionId, FamilyId, GoalId, LawId, PartyId, PersonId, ProjectId, RouteId,
    SiteId, WorldItemId,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WorldDate {
    pub year: i32,
    pub month: u8,
}

impl WorldDate {
    pub fn new(year: i32, month: u8) -> Self {
        assert!(
            (1..=12).contains(&month),
            "world month must be 1 through 12"
        );
        Self { year, month }
    }

    pub fn years_since(self, earlier: Self) -> i32 {
        self.year - earlier.year
    }

    pub fn next_month(self) -> Self {
        if self.month == 12 {
            Self::new(self.year + 1, 1)
        } else {
            Self::new(self.year, self.month + 1)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Principle {
    Compassion,
    Truth,
    Duty,
    Freedom,
    Justice,
    Responsibility,
    Courage,
    Stewardship,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Drive {
    Survival,
    Wealth,
    Status,
    Family,
    Loyalty,
    Freedom,
    Justice,
    Faith,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Occupation {
    Farmer,
    Miller,
    Merchant,
    Guard,
    Priest,
    Healer,
    Smith,
    Laborer,
    Innkeeper,
    Official,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResourceKind {
    Food,
    Timber,
    Iron,
    Medicine,
    Coin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Person {
    pub id: PersonId,
    pub given_name: String,
    pub family: FamilyId,
    pub born: WorldDate,
    pub died: Option<WorldDate>,
    pub parents: Vec<PersonId>,
    pub occupation: Occupation,
    pub faction: FactionId,
    pub home: SiteId,
    pub drives: BTreeMap<Drive, u8>,
}

impl Person {
    pub fn is_alive(&self) -> bool {
        self.died.is_none()
    }

    pub fn age_at(&self, date: WorldDate) -> i32 {
        date.years_since(self.born)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Family {
    pub id: FamilyId,
    pub surname: String,
    pub members: BTreeSet<PersonId>,
    pub wealth: i32,
    pub standing: i16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Faction {
    pub id: FactionId,
    pub name: String,
    pub principle: Principle,
    pub leader: PersonId,
    pub members: BTreeSet<PersonId>,
    pub treasury: i64,
    pub relations: BTreeMap<FactionId, i16>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LawKind {
    FoodRationing,
    PriceControls,
    CompulsoryLabor,
    PropertySeizure,
    Curfew,
    OpenGranaries,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Law {
    pub id: LawId,
    pub kind: LawKind,
    pub enacted: WorldDate,
    pub authority: FactionId,
    pub justification: Principle,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PhysicalEvidenceKind {
    PublicGranary,
    Fortification,
    RefugeeDistrict,
    AbandonedFarm,
    Grave,
    Memorial,
    BurnedBuilding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalEvidence {
    pub kind: PhysicalEvidenceKind,
    pub created: WorldDate,
    pub originating_event: crate::ids::EventId,
    pub associated_person: Option<PersonId>,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Site {
    pub id: SiteId,
    pub name: String,
    pub population: BTreeSet<PersonId>,
    pub resources: BTreeMap<ResourceKind, i64>,
    pub laws: BTreeMap<LawId, Law>,
    pub physical_evidence: Vec<PhysicalEvidence>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SettlementRole {
    Capital,
    Agrarian,
    Forest,
    Mining,
    Crossroads,
    River,
    Monastic,
    Fortress,
}

impl SettlementRole {
    pub fn name(self) -> &'static str {
        match self {
            Self::Capital => "regional town",
            Self::Agrarian => "farming settlement",
            Self::Forest => "timber settlement",
            Self::Mining => "mining settlement",
            Self::Crossroads => "market settlement",
            Self::River => "river settlement",
            Self::Monastic => "monastic settlement",
            Self::Fortress => "border fortress",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionalSettlement {
    pub site: SiteId,
    pub role: SettlementRole,
    pub position: AtlasPosition,
    pub controller: FactionId,
    /// Aggregated population. Named people are the materialized subset.
    pub population: u32,
    pub monthly_production: BTreeMap<ResourceKind, i64>,
    pub monthly_consumption: BTreeMap<ResourceKind, i64>,
    pub shortage: bool,
    pub unrest: u8,
    pub last_event: EventId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionalRoute {
    pub id: RouteId,
    pub name: String,
    pub first: SiteId,
    pub second: SiteId,
    /// Authoritative terrain-cost path through the physical atlas.
    pub path: Vec<AtlasPosition>,
    pub condition: u8,
    pub danger: u8,
    pub disrupted: bool,
    pub disrupted_months: u8,
    pub last_event: EventId,
}

impl RegionalRoute {
    pub fn connects(&self, site: SiteId) -> bool {
        self.first == site || self.second == site
    }

    pub fn other_end(&self, site: SiteId) -> Option<SiteId> {
        if self.first == site {
            Some(self.second)
        } else if self.second == site {
            Some(self.first)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RegionalGoalKind {
    SecureRoute(RouteId),
    RelieveShortage(SiteId),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RegionalGoalStatus {
    Open,
    Resolved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionalGoal {
    pub id: GoalId,
    pub kind: RegionalGoalKind,
    pub sponsor: FactionId,
    pub created: WorldDate,
    pub cause: EventId,
    pub status: RegionalGoalStatus,
    pub title: String,
    pub description: String,
    pub resolved_by: Option<EventId>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RegionalGoalApproach {
    RestoreByForce,
    NegotiatePassage,
    ExploitDisruption,
    DeliverRelief,
    DivertShipment,
    EnforceRationing,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RegionalPartyKind {
    TradeCaravan { resource: ResourceKind, amount: i64 },
    ReturningCaravan,
    Refugees { population: u32 },
    Patrol { goal: GoalId },
    Raiders { strength: u8 },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RegionalPartyStatus {
    Traveling,
    Stationed,
    Arrived,
    Defeated,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionalParty {
    pub id: PartyId,
    pub name: String,
    pub kind: RegionalPartyKind,
    pub status: RegionalPartyStatus,
    pub faction: Option<FactionId>,
    pub leader: Option<PersonId>,
    pub route: RouteId,
    pub origin: SiteId,
    pub destination: SiteId,
    /// Abstract route progress from the origin (0) to the destination (1,000).
    pub progress: u16,
    pub created: WorldDate,
    pub cause: EventId,
    pub last_event: EventId,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StrategicFront {
    Territory,
    Military,
    Economy,
    Political,
    Spiritual,
    Magical,
}

impl StrategicFront {
    pub const ALL: [Self; 6] = [
        Self::Territory,
        Self::Military,
        Self::Economy,
        Self::Political,
        Self::Spiritual,
        Self::Magical,
    ];
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StrategicActorRole {
    DefendingCoalition,
    DarkPower,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StrategicObjectiveKind {
    RestoreRoute(RouteId),
    DisruptRoute(RouteId),
    RelieveSettlement(SiteId),
    ExploitSettlement(SiteId),
    ConsolidateInfluence(SiteId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrategicObjective {
    pub kind: StrategicObjectiveKind,
    pub front: StrategicFront,
    pub started: WorldDate,
    pub cause: EventId,
    pub progress: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrategicActor {
    pub role: StrategicActorRole,
    pub name: String,
    pub capacity: u8,
    pub reserves: i64,
    pub influence: i16,
    pub preferred_front: StrategicFront,
    pub objective: Option<StrategicObjective>,
    pub last_event: Option<EventId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GrandStruggle {
    pub defending_coalition: String,
    pub dark_power: String,
    /// Positive values favor the defending coalition; negative values favor the
    /// Dark Power. Each front is clamped to -100 through 100.
    pub fronts: BTreeMap<StrategicFront, i16>,
    pub actors: BTreeMap<StrategicActorRole, StrategicActor>,
    pub last_event: Option<EventId>,
}

impl GrandStruggle {
    pub(crate) fn seeded(campaign_seed: u64) -> Self {
        const DARK_POWERS: [&str; 5] = [
            "The Ashen Dominion",
            "The Horned Sovereign",
            "The Black Gate Host",
            "The Crown Below",
            "The Devouring Court",
        ];
        let defending_coalition = "The Free Realms".to_string();
        let dark_power = DARK_POWERS[campaign_seed as usize % DARK_POWERS.len()].to_string();
        let actors = BTreeMap::from([
            (
                StrategicActorRole::DefendingCoalition,
                StrategicActor {
                    role: StrategicActorRole::DefendingCoalition,
                    name: defending_coalition.clone(),
                    capacity: 45 + (campaign_seed % 16) as u8,
                    reserves: 80,
                    influence: 0,
                    preferred_front: StrategicFront::Political,
                    objective: None,
                    last_event: None,
                },
            ),
            (
                StrategicActorRole::DarkPower,
                StrategicActor {
                    role: StrategicActorRole::DarkPower,
                    name: dark_power.clone(),
                    capacity: 45 + (campaign_seed.rotate_left(17) % 16) as u8,
                    reserves: 80,
                    influence: 0,
                    preferred_front: StrategicFront::Military,
                    objective: None,
                    last_event: None,
                },
            ),
        ]);
        Self {
            defending_coalition,
            dark_power,
            fronts: StrategicFront::ALL
                .into_iter()
                .map(|front| (front, 0))
                .collect(),
            actors,
            last_event: None,
        }
    }

    pub fn balance(&self, front: StrategicFront) -> i16 {
        self.fronts.get(&front).copied().unwrap_or_default()
    }

    pub fn total_balance(&self) -> i32 {
        self.fronts
            .values()
            .map(|balance| i32::from(*balance))
            .sum()
    }

    pub fn actor(&self, role: StrategicActorRole) -> &StrategicActor {
        &self.actors[&role]
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SignificantItemKind {
    Relic,
    Weapon,
    Ledger,
    Seal,
    Crown,
    Grimoire,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ItemCustodian {
    Person(PersonId),
    Faction(FactionId),
    Site(SiteId),
    Player,
    Lost,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemProvenance {
    pub date: WorldDate,
    pub event: EventId,
    pub custodian: ItemCustodian,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignificantItem {
    pub id: WorldItemId,
    pub object: ObjectId,
    pub name: String,
    pub kind: SignificantItemKind,
    pub form: ItemForm,
    pub materials: Vec<MaterialKind>,
    pub inscribed_formula: Option<FormulaId>,
    pub created: WorldDate,
    pub location: SiteId,
    pub custodian: ItemCustodian,
    pub strategic_front: StrategicFront,
    pub provenance: Vec<ItemProvenance>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SettlementProjectKind {
    PublicGranary,
    WatchHouse,
    MarketHall,
    ReliefHousing,
    CivicWorkshop,
}

impl SettlementProjectKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::PublicGranary => "Public Granary",
            Self::WatchHouse => "Watch House",
            Self::MarketHall => "Market Hall",
            Self::ReliefHousing => "Relief Housing",
            Self::CivicWorkshop => "Civic Workshop",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SettlementProjectPhase {
    Planned,
    Stalled,
    Foundation,
    Structure,
    Completed,
    Damaged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementProject {
    pub id: ProjectId,
    pub site: SiteId,
    pub sponsor: FactionId,
    pub kind: SettlementProjectKind,
    pub name: String,
    pub phase: SettlementProjectPhase,
    pub created: WorldDate,
    pub related_event: EventId,
    pub last_event: EventId,
    pub material_costs: BTreeMap<ResourceKind, i64>,
    pub funding_cost: i64,
    pub workers: Vec<PersonId>,
    pub progress_months: u8,
    pub required_months: u8,
    pub months_in_phase: u8,
    pub damage_count: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BeliefSource {
    Witnessed,
    ToldBy(PersonId),
    FactionDoctrine(FactionId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Belief {
    pub claim: ClaimId,
    pub confidence: u8,
    pub source: BeliefSource,
    pub willing_to_share: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Knowledge {
    pub beliefs: BTreeMap<PersonId, BTreeMap<ClaimId, Belief>>,
}
