use crate::{
    ids::{
        EventId, FactionId, FamilyId, GoalId, LawId, PartyId, PersonId, ProjectId, RouteId, SiteId,
    },
    model::{
        BeliefSource, Law, Person, PhysicalEvidenceKind, Principle, ResourceKind, StrategicFront,
        WorldDate,
    },
};
use ultimate_fate_content::{FormulaId, MagicEffect, MaterialKind, ObjectId};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EntityRef {
    Person(PersonId),
    Family(FamilyId),
    Faction(FactionId),
    Site(SiteId),
    Law(LawId),
    Project(ProjectId),
    Route(RouteId),
    Goal(GoalId),
    Party(PartyId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoricalEventKind {
    SettlementFounded,
    Birth,
    Death,
    Harvest,
    ShortageRecognized,
    LawEnacted,
    Protest,
    Migration,
    LeadershipSuccession,
    RumorSpread,
    PlayerIntervention,
    CareDelivered,
    ArtifactRecovered,
    FormulaReconstructed,
    DungeonCleared,
    ProjectPlanned,
    ProjectStalled,
    SupplyShipment,
    ProjectStarted,
    ProjectCompleted,
    ProjectDamaged,
    ProjectRepaired,
    StrategicBalanceShifted,
    RegionalShortage,
    RegionalRecovery,
    RegionalTrade,
    RouteDisrupted,
    RouteReopened,
    RegionalGoalProposed,
    RegionalGoalResolved,
    RegionalPartyArrived,
    RegionalPartyDefeated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventPublicity {
    Private,
    Local,
    Public,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimAudience {
    Witnesses,
    Local,
    Faction(FactionId),
    Public,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimOrigin {
    Event,
    Person(PersonId),
    Faction(FactionId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TruthValue {
    True,
    False,
    Unknown,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Proposition {
    EventOccurred(EventId),
    FactionResponsibleFor {
        event: EventId,
        faction: FactionId,
    },
    LawWasNecessary(LawId),
    PersonDied(PersonId),
    ObjectSurvived(ObjectId),
    FormulaProduces {
        formula: FormulaId,
        effect: MagicEffect,
    },
    FormulaRequires {
        formula: FormulaId,
        reagent: MaterialKind,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Claim {
    pub id: crate::ids::ClaimId,
    pub proposition: Proposition,
    pub truth: TruthValue,
    pub created_by_event: EventId,
    pub origin: ClaimOrigin,
    pub audience: ClaimAudience,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Consequence {
    ChangeResource {
        site: SiteId,
        resource: ResourceKind,
        amount: i64,
    },
    AddPerson(Box<Person>),
    PersonDied {
        person: PersonId,
    },
    SetFactionLeader {
        faction: FactionId,
        leader: PersonId,
    },
    EnactLaw {
        site: SiteId,
        law: Law,
    },
    SetLawActive {
        site: SiteId,
        law: LawId,
        active: bool,
    },
    ChangeFactionRelation {
        first: FactionId,
        second: FactionId,
        amount: i16,
    },
    ChangeFamilyWealth {
        family: FamilyId,
        amount: i32,
    },
    ChangeFactionTreasury {
        faction: FactionId,
        amount: i64,
    },
    ChangeRegionalPopulation {
        site: SiteId,
        amount: i32,
    },
    SetRouteDisrupted {
        route: RouteId,
        disrupted: bool,
    },
    ShiftStrategicFront {
        front: StrategicFront,
        amount: i16,
    },
    CreatePhysicalEvidence {
        site: SiteId,
        kind: PhysicalEvidenceKind,
        associated_person: Option<PersonId>,
        description: String,
    },
    AssertFactionResponsible {
        event: EventId,
        blamed: FactionId,
        believers: Vec<PersonId>,
        confidence: u8,
        source: BeliefSource,
        audience: ClaimAudience,
    },
    JudgeLawNecessary {
        law: LawId,
        truth: TruthValue,
        believers: Vec<PersonId>,
        confidence: u8,
        source: BeliefSource,
        audience: ClaimAudience,
    },
    ShareClaim {
        claim: crate::ids::ClaimId,
        source: PersonId,
        recipient: PersonId,
        confidence: u8,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoricalEvent {
    pub id: EventId,
    pub date: WorldDate,
    pub location: SiteId,
    pub kind: HistoricalEventKind,
    pub participants: Vec<EntityRef>,
    pub causes: Vec<EventId>,
    pub consequences: Vec<Consequence>,
    pub witnesses: Vec<PersonId>,
    pub principle: Option<Principle>,
    pub publicity: EventPublicity,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventDraft {
    pub location: SiteId,
    pub kind: HistoricalEventKind,
    pub participants: Vec<EntityRef>,
    pub causes: Vec<EventId>,
    pub consequences: Vec<Consequence>,
    pub witnesses: Vec<PersonId>,
    pub principle: Option<Principle>,
    pub publicity: EventPublicity,
    pub summary: String,
}
