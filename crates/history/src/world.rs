use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{Display, Formatter},
};

use ultimate_fate_content::WorldRules;
use ultimate_fate_world_atlas::WorldAtlas;

use crate::{
    event::{
        Claim, ClaimAudience, ClaimOrigin, Consequence, EventDraft, EventPublicity,
        HistoricalEvent, Proposition, TruthValue,
    },
    ids::{
        ClaimId, EventId, FactionId, FamilyId, GoalId, LawId, PartyId, PersonId, ProjectId,
        RouteId, SiteId, WorldItemId,
    },
    model::{
        Belief, BeliefSource, Faction, Family, GrandStruggle, Knowledge, Person, PhysicalEvidence,
        RegionalGoal, RegionalParty, RegionalRoute, RegionalSettlement, SettlementProject,
        SignificantItem, Site, WorldDate,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoricalWorld {
    pub campaign_seed: u64,
    pub date: WorldDate,
    pub(crate) rules: WorldRules,
    pub(crate) atlas: WorldAtlas,
    pub(crate) people: BTreeMap<PersonId, Person>,
    pub(crate) families: BTreeMap<FamilyId, Family>,
    pub(crate) factions: BTreeMap<FactionId, Faction>,
    pub(crate) sites: BTreeMap<SiteId, Site>,
    pub(crate) projects: BTreeMap<ProjectId, SettlementProject>,
    pub(crate) regional_settlements: BTreeMap<SiteId, RegionalSettlement>,
    pub(crate) routes: BTreeMap<RouteId, RegionalRoute>,
    pub(crate) regional_goals: BTreeMap<GoalId, RegionalGoal>,
    pub(crate) regional_parties: BTreeMap<PartyId, RegionalParty>,
    pub(crate) struggle: GrandStruggle,
    pub(crate) items: BTreeMap<WorldItemId, SignificantItem>,
    events: BTreeMap<EventId, HistoricalEvent>,
    claims: BTreeMap<ClaimId, Claim>,
    knowledge: Knowledge,
    next_person: u64,
    next_family: u64,
    next_faction: u64,
    next_site: u64,
    next_event: u64,
    next_law: u64,
    next_claim: u64,
    next_project: u64,
    next_world_item: u64,
    next_route: u64,
    next_goal: u64,
    next_party: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorldError {
    MissingSite(SiteId),
    MissingPerson(PersonId),
    MissingFamily(FamilyId),
    MissingFaction(FactionId),
    MissingEvent(EventId),
    MissingLaw(LawId),
    MissingClaim(ClaimId),
    MissingWorldItem(WorldItemId),
    WorldItemHasNoFormula(WorldItemId),
    WorldItemNotHeldByPlayer(WorldItemId),
    MissingStrategicItem(EventId),
    MissingRoute(RouteId),
    MissingGoal(GoalId),
    MissingParty(PartyId),
    RegionalGoalAlreadyResolved(GoalId),
    InvalidRegionalGoalApproach(GoalId),
    RegionalGoalRequiresCombat(GoalId),
    RegionalPartyInactive(PartyId),
    DuplicatePerson(PersonId),
    DuplicateLaw(LawId),
    CrisisAlreadyResolved(EventId),
    NoActiveCrisisLaw(SiteId),
}

pub(crate) struct ClaimAssertion<'a> {
    pub proposition: Proposition,
    pub truth: TruthValue,
    pub event: EventId,
    pub origin: ClaimOrigin,
    pub audience: ClaimAudience,
    pub believers: &'a [PersonId],
    pub confidence: u8,
    pub source: BeliefSource,
}

impl Display for WorldError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for WorldError {}

impl HistoricalWorld {
    pub(crate) fn empty(campaign_seed: u64, date: WorldDate) -> Self {
        Self {
            campaign_seed,
            date,
            rules: WorldRules::generate(campaign_seed),
            atlas: WorldAtlas::generate(campaign_seed),
            people: BTreeMap::new(),
            families: BTreeMap::new(),
            factions: BTreeMap::new(),
            sites: BTreeMap::new(),
            projects: BTreeMap::new(),
            regional_settlements: BTreeMap::new(),
            routes: BTreeMap::new(),
            regional_goals: BTreeMap::new(),
            regional_parties: BTreeMap::new(),
            struggle: GrandStruggle::seeded(campaign_seed),
            items: BTreeMap::new(),
            events: BTreeMap::new(),
            claims: BTreeMap::new(),
            knowledge: Knowledge::default(),
            next_person: 1,
            next_family: 1,
            next_faction: 1,
            next_site: 1,
            next_event: 1,
            next_law: 1,
            next_claim: 1,
            next_project: 1,
            next_world_item: 1,
            next_route: 1,
            next_goal: 1,
            next_party: 1,
        }
    }

    pub fn people(&self) -> &BTreeMap<PersonId, Person> {
        &self.people
    }

    pub fn atlas(&self) -> &WorldAtlas {
        &self.atlas
    }

    pub fn rules(&self) -> &WorldRules {
        &self.rules
    }

    pub fn families(&self) -> &BTreeMap<FamilyId, Family> {
        &self.families
    }

    pub fn factions(&self) -> &BTreeMap<FactionId, Faction> {
        &self.factions
    }

    pub fn sites(&self) -> &BTreeMap<SiteId, Site> {
        &self.sites
    }

    pub fn projects(&self) -> &BTreeMap<ProjectId, SettlementProject> {
        &self.projects
    }

    pub fn regional_settlements(&self) -> &BTreeMap<SiteId, RegionalSettlement> {
        &self.regional_settlements
    }

    pub fn routes(&self) -> &BTreeMap<RouteId, RegionalRoute> {
        &self.routes
    }

    pub fn regional_goals(&self) -> &BTreeMap<GoalId, RegionalGoal> {
        &self.regional_goals
    }

    pub fn regional_parties(&self) -> &BTreeMap<PartyId, RegionalParty> {
        &self.regional_parties
    }

    pub fn struggle(&self) -> &GrandStruggle {
        &self.struggle
    }

    pub fn significant_items(&self) -> &BTreeMap<WorldItemId, SignificantItem> {
        &self.items
    }

    pub fn events(&self) -> &BTreeMap<EventId, HistoricalEvent> {
        &self.events
    }

    pub fn claims(&self) -> &BTreeMap<ClaimId, Claim> {
        &self.claims
    }

    pub fn knowledge(&self) -> &Knowledge {
        &self.knowledge
    }

    pub(crate) fn record_proposition(&mut self, assertion: ClaimAssertion<'_>) -> ClaimId {
        self.record_claim(assertion)
    }

    pub fn living_people(&self) -> impl Iterator<Item = &Person> {
        self.people.values().filter(|person| person.is_alive())
    }

    pub fn allocate_person_id(&mut self) -> PersonId {
        let id = PersonId(self.next_person);
        self.next_person += 1;
        id
    }

    pub(crate) fn allocate_family_id(&mut self) -> FamilyId {
        let id = FamilyId(self.next_family);
        self.next_family += 1;
        id
    }

    pub(crate) fn allocate_faction_id(&mut self) -> FactionId {
        let id = FactionId(self.next_faction);
        self.next_faction += 1;
        id
    }

    pub(crate) fn allocate_site_id(&mut self) -> SiteId {
        let id = SiteId(self.next_site);
        self.next_site += 1;
        id
    }

    pub fn allocate_law_id(&mut self) -> LawId {
        let id = LawId(self.next_law);
        self.next_law += 1;
        id
    }

    pub(crate) fn allocate_project_id(&mut self) -> ProjectId {
        let id = ProjectId(self.next_project);
        self.next_project += 1;
        id
    }

    pub(crate) fn allocate_world_item_id(&mut self) -> WorldItemId {
        let id = WorldItemId(self.next_world_item);
        self.next_world_item += 1;
        id
    }

    pub(crate) fn allocate_route_id(&mut self) -> RouteId {
        let id = RouteId(self.next_route);
        self.next_route += 1;
        id
    }

    pub(crate) fn allocate_goal_id(&mut self) -> GoalId {
        let id = GoalId(self.next_goal);
        self.next_goal += 1;
        id
    }

    pub fn record_event(&mut self, draft: EventDraft) -> Result<EventId, WorldError> {
        self.validate_event(&draft)?;
        let id = EventId(self.next_event);
        self.next_event += 1;
        let event = HistoricalEvent {
            id,
            date: self.date,
            location: draft.location,
            kind: draft.kind,
            participants: draft.participants,
            causes: draft.causes,
            consequences: draft.consequences,
            witnesses: draft.witnesses,
            principle: draft.principle,
            publicity: draft.publicity,
            summary: draft.summary,
        };

        for consequence in &event.consequences {
            self.apply_consequence(id, consequence);
        }

        let witnesses = event.witnesses.clone();
        let audience = match event.publicity {
            EventPublicity::Private => ClaimAudience::Witnesses,
            EventPublicity::Local => ClaimAudience::Local,
            EventPublicity::Public => ClaimAudience::Public,
        };
        self.events.insert(id, event);
        self.record_claim(ClaimAssertion {
            proposition: Proposition::EventOccurred(id),
            truth: TruthValue::True,
            event: id,
            origin: ClaimOrigin::Event,
            audience,
            believers: &witnesses,
            confidence: 100,
            source: BeliefSource::Witnessed,
        });
        Ok(id)
    }

    fn validate_event(&self, draft: &EventDraft) -> Result<(), WorldError> {
        if !self.sites.contains_key(&draft.location) {
            return Err(WorldError::MissingSite(draft.location));
        }
        for cause in &draft.causes {
            if !self.events.contains_key(cause) {
                return Err(WorldError::MissingEvent(*cause));
            }
        }
        for witness in &draft.witnesses {
            if !self.people.contains_key(witness) {
                return Err(WorldError::MissingPerson(*witness));
            }
        }
        let enacted_laws: Vec<_> = draft
            .consequences
            .iter()
            .filter_map(|consequence| match consequence {
                Consequence::EnactLaw { law, .. } => Some(law.id),
                _ => None,
            })
            .collect();
        for consequence in &draft.consequences {
            self.validate_consequence(consequence, &enacted_laws)?;
        }
        Ok(())
    }

    fn validate_consequence(
        &self,
        consequence: &Consequence,
        enacted_laws: &[LawId],
    ) -> Result<(), WorldError> {
        match consequence {
            Consequence::ChangeResource { site, .. }
            | Consequence::EnactLaw { site, .. }
            | Consequence::CreatePhysicalEvidence { site, .. } => {
                if !self.sites.contains_key(site) {
                    return Err(WorldError::MissingSite(*site));
                }
            }
            Consequence::SetLawActive { site, law, .. } => {
                let site = self.sites.get(site).ok_or(WorldError::MissingSite(*site))?;
                if !site.laws.contains_key(law) {
                    return Err(WorldError::MissingLaw(*law));
                }
            }
            Consequence::AddPerson(person) => {
                if self.people.contains_key(&person.id) {
                    return Err(WorldError::DuplicatePerson(person.id));
                }
                if !self.families.contains_key(&person.family) {
                    return Err(WorldError::MissingFamily(person.family));
                }
                if !self.factions.contains_key(&person.faction) {
                    return Err(WorldError::MissingFaction(person.faction));
                }
                if !self.sites.contains_key(&person.home) {
                    return Err(WorldError::MissingSite(person.home));
                }
            }
            Consequence::PersonDied { person } => {
                if !self.people.contains_key(person) {
                    return Err(WorldError::MissingPerson(*person));
                }
            }
            Consequence::SetFactionLeader { faction, leader } => {
                if !self.factions.contains_key(faction) {
                    return Err(WorldError::MissingFaction(*faction));
                }
                if !self.people.contains_key(leader) {
                    return Err(WorldError::MissingPerson(*leader));
                }
            }
            Consequence::ChangeFactionRelation { first, second, .. } => {
                if !self.factions.contains_key(first) {
                    return Err(WorldError::MissingFaction(*first));
                }
                if !self.factions.contains_key(second) {
                    return Err(WorldError::MissingFaction(*second));
                }
            }
            Consequence::ChangeFamilyWealth { family, .. } => {
                if !self.families.contains_key(family) {
                    return Err(WorldError::MissingFamily(*family));
                }
            }
            Consequence::ChangeFactionTreasury { faction, .. } => {
                if !self.factions.contains_key(faction) {
                    return Err(WorldError::MissingFaction(*faction));
                }
            }
            Consequence::ChangeRegionalPopulation { site, amount } => {
                let settlement = self
                    .regional_settlements
                    .get(site)
                    .ok_or(WorldError::MissingSite(*site))?;
                if *amount < 0 && settlement.population < amount.unsigned_abs() {
                    return Err(WorldError::MissingSite(*site));
                }
            }
            Consequence::SetRouteDisrupted { route, .. } => {
                if !self.routes.contains_key(route) {
                    return Err(WorldError::MissingRoute(*route));
                }
            }
            Consequence::ShiftStrategicFront { .. } => {}
            Consequence::AssertFactionResponsible {
                event,
                blamed,
                believers,
                ..
            } => {
                if !self.events.contains_key(event) {
                    return Err(WorldError::MissingEvent(*event));
                }
                if !self.factions.contains_key(blamed) {
                    return Err(WorldError::MissingFaction(*blamed));
                }
                for believer in believers {
                    if !self.people.contains_key(believer) {
                        return Err(WorldError::MissingPerson(*believer));
                    }
                }
            }
            Consequence::JudgeLawNecessary { law, believers, .. } => {
                let law_exists = self.sites.values().any(|site| site.laws.contains_key(law))
                    || enacted_laws.contains(law);
                if !law_exists {
                    return Err(WorldError::MissingLaw(*law));
                }
                for believer in believers {
                    if !self.people.contains_key(believer) {
                        return Err(WorldError::MissingPerson(*believer));
                    }
                }
            }
            Consequence::ShareClaim {
                claim,
                source,
                recipient,
                ..
            } => {
                if !self.claims.contains_key(claim) {
                    return Err(WorldError::MissingClaim(*claim));
                }
                if !self.people.contains_key(source) {
                    return Err(WorldError::MissingPerson(*source));
                }
                if !self.people.contains_key(recipient) {
                    return Err(WorldError::MissingPerson(*recipient));
                }
            }
        }
        Ok(())
    }

    fn apply_consequence(&mut self, event: EventId, consequence: &Consequence) {
        match consequence {
            Consequence::ChangeResource {
                site,
                resource,
                amount,
            } => {
                *self
                    .sites
                    .get_mut(site)
                    .expect("validated site")
                    .resources
                    .entry(*resource)
                    .or_default() += amount;
            }
            Consequence::AddPerson(person) => {
                let person = person.as_ref().clone();
                self.families
                    .get_mut(&person.family)
                    .expect("validated family")
                    .members
                    .insert(person.id);
                self.factions
                    .get_mut(&person.faction)
                    .expect("validated faction")
                    .members
                    .insert(person.id);
                self.sites
                    .get_mut(&person.home)
                    .expect("validated site")
                    .population
                    .insert(person.id);
                self.people.insert(person.id, person);
            }
            Consequence::PersonDied { person } => {
                let person_record = self.people.get_mut(person).expect("validated person");
                person_record.died = Some(self.date);
                self.sites
                    .get_mut(&person_record.home)
                    .expect("validated home")
                    .population
                    .remove(person);
                self.factions
                    .get_mut(&person_record.faction)
                    .expect("validated faction")
                    .members
                    .remove(person);
            }
            Consequence::SetFactionLeader { faction, leader } => {
                self.factions
                    .get_mut(faction)
                    .expect("validated faction")
                    .leader = *leader;
            }
            Consequence::EnactLaw { site, law } => {
                self.sites
                    .get_mut(site)
                    .expect("validated site")
                    .laws
                    .insert(law.id, law.clone());
            }
            Consequence::SetLawActive { site, law, active } => {
                self.sites
                    .get_mut(site)
                    .expect("validated site")
                    .laws
                    .get_mut(law)
                    .expect("validated law")
                    .active = *active;
            }
            Consequence::ChangeFactionRelation {
                first,
                second,
                amount,
            } => {
                change_relation(&mut self.factions, *first, *second, *amount);
                change_relation(&mut self.factions, *second, *first, *amount);
            }
            Consequence::ChangeFamilyWealth { family, amount } => {
                self.families
                    .get_mut(family)
                    .expect("validated family")
                    .wealth += amount;
            }
            Consequence::ChangeFactionTreasury { faction, amount } => {
                self.factions
                    .get_mut(faction)
                    .expect("validated faction")
                    .treasury += amount;
            }
            Consequence::ChangeRegionalPopulation { site, amount } => {
                let population_change = amount.unsigned_abs();
                let moved_food_need = (i64::from(population_change) / 4).max(1);
                let settlement = self
                    .regional_settlements
                    .get_mut(site)
                    .expect("validated regional settlement");
                if *amount < 0 {
                    settlement.population -= population_change;
                } else {
                    settlement.population = settlement.population.saturating_add(population_change);
                }
                let food = settlement
                    .monthly_consumption
                    .entry(crate::model::ResourceKind::Food)
                    .or_default();
                if *amount < 0 {
                    *food = food.saturating_sub(moved_food_need).max(1);
                } else {
                    *food = food.saturating_add(moved_food_need);
                }
            }
            Consequence::SetRouteDisrupted { route, disrupted } => {
                let route = self.routes.get_mut(route).expect("validated route");
                route.disrupted = *disrupted;
                route.disrupted_months = 0;
            }
            Consequence::ShiftStrategicFront { front, amount } => {
                let balance = self.struggle.fronts.entry(*front).or_default();
                *balance = balance.saturating_add(*amount).clamp(-100, 100);
            }
            Consequence::CreatePhysicalEvidence {
                site,
                kind,
                associated_person,
                description,
            } => {
                self.sites
                    .get_mut(site)
                    .expect("validated site")
                    .physical_evidence
                    .push(PhysicalEvidence {
                        kind: *kind,
                        created: self.date,
                        originating_event: event,
                        associated_person: *associated_person,
                        description: description.clone(),
                    });
            }
            Consequence::AssertFactionResponsible {
                event: blamed_event,
                blamed,
                believers,
                confidence,
                source,
                audience,
            } => {
                self.record_claim(ClaimAssertion {
                    proposition: Proposition::FactionResponsibleFor {
                        event: *blamed_event,
                        faction: *blamed,
                    },
                    truth: TruthValue::False,
                    event,
                    origin: claim_origin(*source),
                    audience: *audience,
                    believers,
                    confidence: *confidence,
                    source: *source,
                });
            }
            Consequence::JudgeLawNecessary {
                law,
                truth,
                believers,
                confidence,
                source,
                audience,
            } => {
                self.record_claim(ClaimAssertion {
                    proposition: Proposition::LawWasNecessary(*law),
                    truth: *truth,
                    event,
                    origin: claim_origin(*source),
                    audience: *audience,
                    believers,
                    confidence: *confidence,
                    source: *source,
                });
            }
            Consequence::ShareClaim {
                claim,
                source,
                recipient,
                confidence,
            } => {
                self.share_claim(*claim, *source, *recipient, *confidence);
            }
        }
    }

    fn record_claim(&mut self, assertion: ClaimAssertion<'_>) -> ClaimId {
        let id = ClaimId(self.next_claim);
        self.next_claim += 1;
        self.claims.insert(
            id,
            Claim {
                id,
                proposition: assertion.proposition,
                truth: assertion.truth,
                created_by_event: assertion.event,
                origin: assertion.origin,
                audience: assertion.audience,
            },
        );
        for person in assertion.believers {
            let belief = Belief {
                claim: id,
                confidence: assertion.confidence.min(100),
                source: assertion.source,
                willing_to_share: assertion.confidence >= 40,
            };
            self.knowledge
                .beliefs
                .entry(*person)
                .or_default()
                .insert(id, belief);
        }
        id
    }

    pub(crate) fn share_claim(
        &mut self,
        claim: ClaimId,
        source_person: PersonId,
        recipient: PersonId,
        confidence: u8,
    ) {
        if !self.claims.contains_key(&claim) {
            return;
        }
        self.knowledge.beliefs.entry(recipient).or_default().insert(
            claim,
            Belief {
                claim,
                confidence: confidence.min(100),
                source: BeliefSource::ToldBy(source_person),
                willing_to_share: confidence >= 40,
            },
        );
    }

    pub(crate) fn insert_family(&mut self, family: Family) {
        self.families.insert(family.id, family);
    }

    pub(crate) fn insert_faction(&mut self, faction: Faction) {
        self.factions.insert(faction.id, faction);
    }

    pub(crate) fn insert_site(&mut self, site: Site) {
        self.sites.insert(site.id, site);
    }

    pub(crate) fn insert_project(&mut self, project: SettlementProject) {
        self.projects.insert(project.id, project);
    }

    pub(crate) fn insert_significant_item(&mut self, item: SignificantItem) {
        self.items.insert(item.id, item);
    }

    pub(crate) fn insert_regional_settlement(&mut self, settlement: RegionalSettlement) {
        self.regional_settlements
            .insert(settlement.site, settlement);
    }

    pub(crate) fn insert_route(&mut self, route: RegionalRoute) {
        self.routes.insert(route.id, route);
    }

    pub(crate) fn insert_regional_goal(&mut self, goal: RegionalGoal) {
        self.regional_goals.insert(goal.id, goal);
    }

    pub(crate) fn allocate_party_id(&mut self) -> PartyId {
        let id = PartyId(self.next_party);
        self.next_party += 1;
        id
    }

    pub(crate) fn insert_regional_party(&mut self, party: RegionalParty) {
        self.regional_parties.insert(party.id, party);
    }
}

fn change_relation(
    factions: &mut BTreeMap<FactionId, Faction>,
    from: FactionId,
    toward: FactionId,
    amount: i16,
) {
    let relation = factions
        .get_mut(&from)
        .expect("validated faction")
        .relations
        .entry(toward)
        .or_default();
    *relation = relation.saturating_add(amount).clamp(-100, 100);
}

fn claim_origin(source: BeliefSource) -> ClaimOrigin {
    match source {
        BeliefSource::Witnessed => ClaimOrigin::Event,
        BeliefSource::ToldBy(person) => ClaimOrigin::Person(person),
        BeliefSource::FactionDoctrine(faction) => ClaimOrigin::Faction(faction),
    }
}
