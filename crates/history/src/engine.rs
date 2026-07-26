use std::collections::{BTreeMap, BTreeSet};

use ultimate_fate_content::{ItemForm, MaterialKind, ObjectId};
use ultimate_fate_world_atlas::{SitePreference, WaterBody};

use ultimate_fate_core::{RandomStream, StreamId};

use crate::{
    event::{
        ClaimAudience, Consequence, EntityRef, EventDraft, EventPublicity, HistoricalEventKind,
        Proposition, TruthValue,
    },
    ids::{
        EventId, FactionId, FamilyId, GoalId, LawId, PartyId, PersonId, ProjectId, RouteId, SiteId,
        WorldItemId,
    },
    model::{
        BeliefSource, Drive, Faction, Family, ItemCustodian, ItemProvenance, Law, LawKind,
        Occupation, Person, PhysicalEvidenceKind, Principle, RegionalGoal, RegionalGoalApproach,
        RegionalGoalKind, RegionalGoalStatus, RegionalParty, RegionalPartyKind,
        RegionalPartyStatus, RegionalRoute, RegionalSettlement, ResourceKind, SettlementProject,
        SettlementProjectKind, SettlementProjectPhase, SettlementRole, SignificantItem,
        SignificantItemKind, Site, StrategicActorRole, StrategicFront, StrategicObjective,
        StrategicObjectiveKind, WorldDate,
    },
    simulation::{SystemCadence, WorldIntent, WorldSimulator},
    world::{ClaimAssertion, HistoricalWorld, WorldError},
};

const FOUNDATION_STREAM: StreamId = StreamId(0x464f_554e_4441_544e);
const FACTION_DOCTRINE_STREAM: StreamId = StreamId(0x4641_4354_444f_4354);
const HARVEST_STREAM: StreamId = StreamId(0x4841_5256_4553_5420);
const DEMOGRAPHY_STREAM: StreamId = StreamId(0x4445_4d4f_4752_4150);
const RUMOR_STREAM: StreamId = StreamId(0x5255_4d4f_5220_2020);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YearSummary {
    pub year: i32,
    pub population: usize,
    pub food: i64,
    pub events_created: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonthSummary {
    pub date: WorldDate,
    pub events: Vec<EventId>,
    pub changed_projects: Vec<ProjectId>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CrisisResolutionKind {
    EnforceEmergencyLaw,
    OpenPublicStores,
    BrokerCompromise,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrisisResolutionOption {
    pub kind: CrisisResolutionKind,
    pub title: String,
    pub description: String,
    pub supported_faction: FactionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrisisResolutionOutcome {
    pub kind: CrisisResolutionKind,
    pub event: EventId,
    pub summary: String,
    pub reaction_faction: FactionId,
    pub aftermath_prompt: String,
    pub food_after: i64,
    pub coin_after: i64,
    pub active_laws: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum AidResolutionKind {
    ReleasedByConsent,
    Purchased,
    TakenWithoutConsent,
    AlternativeTreatment,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionalGoalOption {
    pub approach: RegionalGoalApproach,
    pub title: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionalGoalOutcome {
    pub goal: GoalId,
    pub approach: RegionalGoalApproach,
    pub event: EventId,
    pub summary: String,
}

pub struct HistoryEngine {
    world: HistoricalWorld,
    primary_site: SiteId,
    foundation_event: EventId,
    simulator: WorldSimulator,
}

impl HistoryEngine {
    pub fn seeded_town(campaign_seed: u64) -> Result<Self, WorldError> {
        let mut world = HistoricalWorld::empty(campaign_seed, WorldDate::new(0, 1));
        let mut rng = RandomStream::new(campaign_seed, FOUNDATION_STREAM);

        let site_id = world.allocate_site_id();
        let town_name = TOWN_NAMES[bounded(&mut rng, TOWN_NAMES.len())].to_string();
        world.insert_site(Site {
            id: site_id,
            name: town_name.clone(),
            population: BTreeSet::new(),
            resources: BTreeMap::from([
                (ResourceKind::Food, 180),
                (ResourceKind::Timber, 90),
                (ResourceKind::Iron, 35),
                (ResourceKind::Medicine, 20),
                (ResourceKind::Coin, 240),
            ]),
            laws: BTreeMap::new(),
            physical_evidence: Vec::new(),
        });

        let mut family_ids = Vec::new();
        let surname_offset = bounded(&mut rng, SURNAMES.len());
        for index in 0..3 {
            let id = world.allocate_family_id();
            family_ids.push(id);
            world.insert_family(Family {
                id,
                surname: SURNAMES[(surname_offset + index) % SURNAMES.len()].to_string(),
                members: BTreeSet::new(),
                wealth: 70 + index as i32 * 35,
                standing: 10 - index as i16 * 4,
            });
        }

        // Founding institutions keep stable social roles, but their public
        // doctrines belong to this world's history. A separate stream prevents
        // this choice from perturbing families, names, and demographics.
        let faction_principles = founding_faction_principles(campaign_seed);
        let faction_specs = [
            ("Civic Council", faction_principles[0], 150),
            ("Hearth Guild", faction_principles[1], 95),
            ("Free River Fellowship", faction_principles[2], 60),
        ];
        let mut faction_ids = Vec::new();
        for (name, principle, treasury) in faction_specs {
            let id = world.allocate_faction_id();
            faction_ids.push(id);
            world.insert_faction(Faction {
                id,
                name: format!("{town_name} {name}"),
                principle,
                leader: PersonId(0),
                members: BTreeSet::new(),
                treasury,
                relations: BTreeMap::new(),
            });
        }
        for first in &faction_ids {
            for second in &faction_ids {
                if first != second {
                    world
                        .factions
                        .get_mut(first)
                        .expect("seeded faction")
                        .relations
                        .insert(*second, 0);
                }
            }
        }

        let occupations = [
            Occupation::Farmer,
            Occupation::Farmer,
            Occupation::Farmer,
            Occupation::Miller,
            Occupation::Merchant,
            Occupation::Guard,
            Occupation::Priest,
            Occupation::Healer,
            Occupation::Smith,
            Occupation::Official,
        ];
        let given_name_offset = bounded(&mut rng, GIVEN_NAMES.len());
        for index in 0..30 {
            let id = world.allocate_person_id();
            let family = family_ids[index % family_ids.len()];
            let faction = faction_ids[(index / 2) % faction_ids.len()];
            let age = 16 + bounded(&mut rng, 45) as i32;
            let person = Person {
                id,
                given_name: GIVEN_NAMES[(given_name_offset + index) % GIVEN_NAMES.len()]
                    .to_string(),
                family,
                born: WorldDate::new(-age, 1 + bounded(&mut rng, 12) as u8),
                died: None,
                parents: Vec::new(),
                occupation: occupations[index % occupations.len()],
                faction,
                home: site_id,
                drives: generated_drives(&mut rng),
            };
            insert_initial_person(&mut world, person);
        }

        for faction in world.factions.values_mut() {
            faction.leader = faction
                .members
                .iter()
                .copied()
                .min_by_key(|person| world.people[person].born)
                .expect("seeded faction has members");
        }

        let witnesses: Vec<_> = world.people.keys().copied().collect();
        let foundation_event = world.record_event(EventDraft {
            location: site_id,
            kind: HistoricalEventKind::SettlementFounded,
            participants: std::iter::once(EntityRef::Site(site_id))
                .chain(faction_ids.iter().copied().map(EntityRef::Faction))
                .collect(),
            causes: Vec::new(),
            consequences: vec![Consequence::CreatePhysicalEvidence {
                site: site_id,
                kind: PhysicalEvidenceKind::PublicGranary,
                associated_person: None,
                description: format!(
                    "The first public granary of {town_name}, raised by all three factions"
                ),
            }],
            witnesses,
            principle: Some(faction_principles[0]),
            publicity: EventPublicity::Public,
            summary: format!(
                "{town_name} was incorporated around its river crossing and common granary under a charter of {:?}",
                faction_principles[0]
            ),
        })?;

        let mut engine = Self {
            world,
            primary_site: site_id,
            foundation_event,
            simulator: WorldSimulator::default(),
        };
        // Geography and neighboring settlements must exist before history is
        // simulated so resources, roads, distance, trade, conflict, and
        // migration can shape the generated past rather than being attached
        // only when the playable campaign begins.
        engine.seed_region()?;
        Ok(engine)
    }

    pub fn world(&self) -> &HistoricalWorld {
        &self.world
    }

    pub fn primary_site(&self) -> SiteId {
        self.primary_site
    }

    pub fn foundation_event(&self) -> EventId {
        self.foundation_event
    }

    pub fn begin_living_simulation(&mut self) -> Result<ProjectId, WorldError> {
        // Campaign-start systems settle in deterministic passes. This preserves
        // module isolation while allowing later modules to react to state
        // created by earlier ones, such as traffic activating after regional
        // routes have been generated.
        for _ in 0..8 {
            let intents = self.simulator.intents(
                SystemCadence::CampaignStart,
                &self.world,
                self.primary_site,
                self.foundation_event,
            );
            if intents.is_empty() {
                break;
            }
            for scheduled in intents {
                self.resolve_world_intent(scheduled.intent)?;
            }
        }
        self.pulse_living_simulation()?;
        Ok(self
            .world
            .projects()
            .values()
            .next()
            .expect("planning system creates settlement projects")
            .id)
    }

    pub fn pulse_living_simulation(&mut self) -> Result<Vec<EventId>, WorldError> {
        let intents = self.simulator.intents(
            SystemCadence::LivingPulse,
            &self.world,
            self.primary_site,
            self.foundation_event,
        );
        let mut events = Vec::new();
        for scheduled in intents {
            if let Some(event) = self.resolve_world_intent(scheduled.intent)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    fn plan_initial_settlement(&mut self) -> Result<ProjectId, WorldError> {
        if let Some(project) = self.world.projects().values().next() {
            return Ok(project.id);
        }
        let related_event = self
            .world
            .events()
            .values()
            .rev()
            .find(|event| event.kind == HistoricalEventKind::ShortageRecognized)
            .map(|event| event.id)
            .unwrap_or(self.foundation_event);
        let site = self.primary_site;
        let active_law = self.world.sites()[&site]
            .laws
            .values()
            .filter(|law| law.active)
            .min_by_key(|law| law.id)
            .map(|law| (law.kind, law.authority));
        let primary_sponsor = active_law
            .map(|(_, authority)| authority)
            .unwrap_or_else(|| {
                self.world
                    .factions()
                    .values()
                    .max_by_key(|faction| (faction.treasury, std::cmp::Reverse(faction.id)))
                    .expect("seeded world has factions")
                    .id
            });
        let primary_kind = active_law.map_or(SettlementProjectKind::PublicGranary, |(law, _)| {
            project_kind_for_law(law)
        });
        let mut sponsors = std::iter::once(primary_sponsor)
            .chain(
                self.world
                    .factions()
                    .keys()
                    .copied()
                    .filter(|faction| *faction != primary_sponsor),
            )
            .collect::<Vec<_>>();
        sponsors.truncate(3);
        let mut used_kinds = BTreeSet::new();
        let mut projects = Vec::new();
        for (index, sponsor) in sponsors.into_iter().enumerate() {
            let preferred = if index == 0 {
                primary_kind
            } else {
                project_kind_for_principle(self.world.factions()[&sponsor].principle)
            };
            let kind = unique_project_kind(preferred, &used_kinds);
            used_kinds.insert(kind);
            projects.push(self.plan_living_project(site, sponsor, kind, related_event)?);
        }
        Ok(projects[0])
    }

    fn plan_living_project(
        &mut self,
        site: SiteId,
        sponsor: FactionId,
        kind: SettlementProjectKind,
        related_event: EventId,
    ) -> Result<ProjectId, WorldError> {
        let (material_costs, funding_cost, required_months) = project_requirements(kind);
        let id = self.world.allocate_project_id();
        let name = format!("New {} {}", self.world.sites()[&site].name, kind.name());
        let mut workers = self.world.factions()[&sponsor]
            .members
            .iter()
            .copied()
            .filter(|person| self.world.people()[person].is_alive())
            .collect::<Vec<_>>();
        workers.sort_by_key(|person| {
            let occupation = self.world.people()[person].occupation;
            let priority = match occupation {
                Occupation::Laborer => 0,
                Occupation::Smith => 1,
                Occupation::Farmer | Occupation::Miller => 2,
                _ => 3,
            };
            (priority, *person)
        });
        workers.truncate(3);
        self.world.insert_project(SettlementProject {
            id,
            site,
            sponsor,
            kind,
            name: name.clone(),
            phase: SettlementProjectPhase::Planned,
            created: self.world.date,
            related_event,
            last_event: related_event,
            material_costs,
            funding_cost,
            workers: workers.clone(),
            progress_months: 0,
            required_months,
            months_in_phase: 0,
            damage_count: 0,
        });
        let event = self.world.record_event(EventDraft {
            location: site,
            kind: HistoricalEventKind::ProjectPlanned,
            participants: [
                EntityRef::Project(id),
                EntityRef::Faction(sponsor),
                EntityRef::Site(site),
            ]
            .into_iter()
            .chain(workers.iter().copied().map(EntityRef::Person))
            .collect(),
            causes: vec![related_event],
            consequences: Vec::new(),
            witnesses: self.world.sites()[&site]
                .population
                .iter()
                .copied()
                .collect(),
            principle: Some(self.world.factions()[&sponsor].principle),
            publicity: EventPublicity::Public,
            summary: format!(
                "{} announced plans for {name} and assigned {} residents to the work",
                self.world.factions()[&sponsor].name,
                workers.len()
            ),
        })?;
        self.world
            .projects
            .get_mut(&id)
            .expect("new project exists")
            .last_event = event;
        Ok(id)
    }

    pub fn advance_month(&mut self) -> Result<MonthSummary, WorldError> {
        self.world.date = self.world.date.next_month();
        let events = self.resolve_monthly_systems()?;
        Ok(MonthSummary {
            date: self.world.date,
            events,
            changed_projects: self.world.projects().keys().copied().collect(),
        })
    }

    fn resolve_monthly_systems(&mut self) -> Result<Vec<EventId>, WorldError> {
        let intents = self.simulator.intents(
            SystemCadence::Monthly,
            &self.world,
            self.primary_site,
            self.foundation_event,
        );
        let mut events = Vec::new();
        for scheduled in intents {
            if let Some(event) = self.resolve_world_intent(scheduled.intent)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    fn resolve_world_intent(&mut self, intent: WorldIntent) -> Result<Option<EventId>, WorldError> {
        match intent {
            WorldIntent::PlanSettlement => self.plan_initial_settlement().map(|_| None),
            WorldIntent::SeedStrategicItems => self.seed_strategic_items(),
            WorldIntent::SeedRegion => self.seed_region(),
            WorldIntent::SeedRegionalTraffic => self.seed_regional_traffic(),
            WorldIntent::AdvanceRegionalEconomy(site) => self.advance_regional_economy(site),
            WorldIntent::AdvanceRegionalParty { party, step } => {
                self.advance_regional_party(party, step)
            }
            WorldIntent::MoveRegionalTrade {
                route,
                from,
                to,
                resource,
                amount,
            } => self.move_regional_trade(route, from, to, resource, amount),
            WorldIntent::ImportProjectSupplies(project) => {
                self.maybe_import_project_supplies(project)
            }
            WorldIntent::MaintainProject(id) => {
                let project = self.world.projects()[&id].clone();
                match project.phase {
                    SettlementProjectPhase::Planned | SettlementProjectPhase::Stalled => {
                        self.start_project(project)
                    }
                    SettlementProjectPhase::Foundation | SettlementProjectPhase::Structure => {
                        self.progress_project(project)
                    }
                    SettlementProjectPhase::Damaged => self.maybe_repair_project(project),
                    SettlementProjectPhase::Completed => Ok(None),
                }
            }
            WorldIntent::ApplyProjectPressure(id) => {
                let project = self.world.projects()[&id].clone();
                self.maybe_damage_project(project)
            }
            WorldIntent::ApplyRegionalPressure(route) => self.apply_regional_pressure(route),
            WorldIntent::ProposeRouteGoal(route) => self.propose_route_goal(route),
            WorldIntent::ProposeReliefGoal(site) => self.propose_relief_goal(site),
            WorldIntent::MigrateRegionalPopulation { from, to, amount } => {
                self.migrate_regional_population(from, to, amount)
            }
            WorldIntent::AssessGrandStrategy => self.assess_grand_strategy(),
        }
    }

    fn seed_strategic_items(&mut self) -> Result<Option<EventId>, WorldError> {
        if !self.world.significant_items().is_empty() {
            return Ok(None);
        }
        let related_event = self
            .world
            .events()
            .values()
            .rev()
            .find(|event| event.kind == HistoricalEventKind::ShortageRecognized)
            .map(|event| event.id)
            .unwrap_or(self.foundation_event);
        let law = self.world.sites()[&self.primary_site]
            .laws
            .values()
            .filter(|law| law.active)
            .min_by_key(|law| law.id)
            .map(|law| law.kind)
            .unwrap_or(LawKind::FoodRationing);
        let (name, kind, front) = strategic_item_for_law(law);
        let id = self.world.allocate_world_item_id();
        let date = self.world.events()[&related_event].date;
        let formula = self
            .world
            .rules()
            .formulas
            .iter()
            .find(|formula| formula.effect == ultimate_fate_content::MagicEffect::Heal)
            .cloned();
        let inscribed_formula = formula.as_ref().map(|formula| formula.id);
        let object = ObjectId(id.0);
        self.world.insert_significant_item(SignificantItem {
            id,
            object,
            name: name.to_string(),
            kind,
            form: if matches!(kind, SignificantItemKind::Ledger | SignificantItemKind::Grimoire) {
                ItemForm::Grimoire
            } else {
                ItemForm::RitualVessel
            },
            materials: vec![MaterialKind::CleanLinen],
            inscribed_formula,
            created: date,
            location: self.primary_site,
            custodian: ItemCustodian::Lost,
            strategic_front: front,
            provenance: vec![ItemProvenance {
                date,
                event: related_event,
                custodian: ItemCustodian::Lost,
                description: format!(
                    "{name} disappeared into sealed stores during the crisis and remains strategically important to both sides"
                ),
            }],
        });
        let believers = self.world.sites()[&self.primary_site]
            .population
            .iter()
            .take(2)
            .copied()
            .collect::<Vec<_>>();
        self.world.record_proposition(ClaimAssertion {
            proposition: Proposition::ObjectSurvived(object),
            truth: TruthValue::True,
            event: related_event,
            origin: crate::event::ClaimOrigin::Event,
            audience: ClaimAudience::Local,
            believers: &believers,
            confidence: 70,
            source: BeliefSource::Witnessed,
        });
        if let Some(formula) = formula {
            self.world.record_proposition(ClaimAssertion {
                proposition: Proposition::FormulaProduces {
                    formula: formula.id,
                    effect: formula.effect,
                },
                truth: TruthValue::True,
                event: related_event,
                origin: crate::event::ClaimOrigin::Event,
                audience: ClaimAudience::Local,
                believers: &believers,
                confidence: 75,
                source: BeliefSource::Witnessed,
            });
            if let Some(reagent) = formula.reagents.first().copied() {
                self.world.record_proposition(ClaimAssertion {
                    proposition: Proposition::FormulaRequires {
                        formula: formula.id,
                        reagent,
                    },
                    truth: TruthValue::True,
                    event: related_event,
                    origin: crate::event::ClaimOrigin::Event,
                    audience: ClaimAudience::Local,
                    believers: &believers,
                    confidence: 60,
                    source: BeliefSource::Witnessed,
                });
            }
        }
        Ok(None)
    }

    fn seed_region(&mut self) -> Result<Option<EventId>, WorldError> {
        if !self.world.regional_settlements().is_empty() {
            return Ok(None);
        }
        let primary_resident = self.world.sites()[&self.primary_site]
            .population
            .iter()
            .next()
            .copied()
            .expect("the primary settlement has named residents");
        let primary_controller = self.world.people()[&primary_resident].faction;
        let primary_population = self.world.sites()[&self.primary_site].population.len() as u32;
        let landmass = self.world.atlas().largest_landmass();
        let primary_position = self
            .world
            .atlas()
            .choose_site(SitePreference::Capital, &[], Some(landmass))
            .expect("generated atlas always supports a capital");
        self.world.insert_regional_settlement(RegionalSettlement {
            site: self.primary_site,
            role: SettlementRole::Capital,
            position: primary_position,
            controller: primary_controller,
            population: primary_population,
            monthly_production: BTreeMap::from([
                (ResourceKind::Food, 38),
                (ResourceKind::Timber, 4),
                (ResourceKind::Iron, 2),
                (ResourceKind::Medicine, 2),
                (ResourceKind::Coin, 8),
            ]),
            monthly_consumption: BTreeMap::from([
                (ResourceKind::Food, 34),
                (ResourceKind::Timber, 4),
                (ResourceKind::Iron, 2),
                (ResourceKind::Medicine, 2),
                (ResourceKind::Coin, 8),
            ]),
            shortage: false,
            unrest: 5,
            last_event: self.foundation_event,
        });

        let total_sites = 5 + (self.world.campaign_seed as usize % 4);
        let roles = [
            SettlementRole::Agrarian,
            SettlementRole::Forest,
            SettlementRole::Mining,
            SettlementRole::Crossroads,
            SettlementRole::River,
            SettlementRole::Monastic,
            SettlementRole::Fortress,
        ];
        let factions = self.world.factions().keys().copied().collect::<Vec<_>>();
        let name_offset = self.world.campaign_seed as usize % REGIONAL_SITE_NAMES.len();
        let mut outlying_sites = Vec::new();
        let mut occupied = vec![primary_position];
        for index in 0..(total_sites - 1) {
            let site = self.world.allocate_site_id();
            let role = roles[index % roles.len()];
            let position = self
                .world
                .atlas()
                .choose_site(site_preference(role), &occupied, Some(landmass))
                .expect("largest landmass supports every regional settlement");
            occupied.push(position);
            let name =
                REGIONAL_SITE_NAMES[(name_offset + index) % REGIONAL_SITE_NAMES.len()].to_string();
            let population =
                70 + ((self.world.campaign_seed.rotate_left(index as u32) % 91) as u32);
            let (resources, production, consumption) = regional_economy_profile(role, population);
            self.world.insert_site(Site {
                id: site,
                name,
                population: BTreeSet::new(),
                resources,
                laws: BTreeMap::new(),
                physical_evidence: Vec::new(),
            });
            self.world.insert_regional_settlement(RegionalSettlement {
                site,
                role,
                position,
                controller: factions[index % factions.len()],
                population,
                monthly_production: production,
                monthly_consumption: consumption,
                shortage: false,
                unrest: 4 + index as u8,
                last_event: self.foundation_event,
            });
            outlying_sites.push(site);
        }

        for (index, site) in outlying_sites.iter().copied().enumerate() {
            self.connect_regional_route(self.primary_site, site, index)?;
        }
        for (index, pair) in outlying_sites.windows(2).enumerate() {
            self.connect_regional_route(pair[0], pair[1], total_sites + index)?;
        }
        Ok(None)
    }

    fn connect_regional_route(
        &mut self,
        first: SiteId,
        second: SiteId,
        salt: usize,
    ) -> Result<RouteId, WorldError> {
        let id = self.world.allocate_route_id();
        let first_name = self.world.sites()[&first].name.clone();
        let second_name = self.world.sites()[&second].name.clone();
        let first_position = self.world.regional_settlements()[&first].position;
        let second_position = self.world.regional_settlements()[&second].position;
        let path = self
            .world
            .atlas()
            .route(first_position, second_position)
            .expect("settlements on one landmass always have a terrain route");
        self.world.insert_route(RegionalRoute {
            id,
            name: format!("{first_name}–{second_name} road"),
            first,
            second,
            path,
            condition: 62 + (salt as u8 * 7) % 29,
            danger: 12 + (salt as u8 * 11) % 24,
            disrupted: false,
            disrupted_months: 0,
            last_event: self.foundation_event,
        });
        Ok(id)
    }

    fn advance_regional_economy(&mut self, site: SiteId) -> Result<Option<EventId>, WorldError> {
        let settlement = self
            .world
            .regional_settlements()
            .get(&site)
            .cloned()
            .ok_or(WorldError::MissingSite(site))?;
        for resource in [
            ResourceKind::Food,
            ResourceKind::Timber,
            ResourceKind::Iron,
            ResourceKind::Medicine,
            ResourceKind::Coin,
        ] {
            let produced = settlement
                .monthly_production
                .get(&resource)
                .copied()
                .unwrap_or_default();
            let consumed = settlement
                .monthly_consumption
                .get(&resource)
                .copied()
                .unwrap_or_default();
            let stock = self.world.sites()[&site]
                .resources
                .get(&resource)
                .copied()
                .unwrap_or_default();
            let change = (produced - consumed).max(-stock);
            *self
                .world
                .sites
                .get_mut(&site)
                .expect("regional site exists")
                .resources
                .entry(resource)
                .or_default() += change;
        }

        let food = self.world.sites()[&site].resources[&ResourceKind::Food];
        let food_need = settlement
            .monthly_consumption
            .get(&ResourceKind::Food)
            .copied()
            .unwrap_or(1)
            .max(1);
        let shortage = food < food_need.saturating_mul(2);
        if shortage == settlement.shortage {
            let state = self
                .world
                .regional_settlements
                .get_mut(&site)
                .expect("regional settlement exists");
            if shortage {
                state.unrest = state.unrest.saturating_add(2).min(100);
            } else {
                state.unrest = state.unrest.saturating_sub(1);
            }
            return Ok(None);
        }

        let site_name = self.world.sites()[&site].name.clone();
        let (kind, summary, consequences) = if shortage {
            (
                HistoricalEventKind::RegionalShortage,
                format!(
                    "{site_name} entered shortage as local production and open trade could no longer cover consumption"
                ),
                vec![
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Economy,
                        amount: -1,
                    },
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Political,
                        amount: -1,
                    },
                ],
            )
        } else {
            (
                HistoricalEventKind::RegionalRecovery,
                format!(
                    "{site_name} recovered from shortage after production, trade, and population changes restored its reserves"
                ),
                vec![
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Economy,
                        amount: 1,
                    },
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Political,
                        amount: 1,
                    },
                ],
            )
        };
        let event = self.world.record_event(EventDraft {
            location: site,
            kind,
            participants: vec![EntityRef::Site(site)],
            causes: vec![settlement.last_event],
            consequences,
            witnesses: self.project_witnesses(site),
            principle: Some(Principle::Stewardship),
            publicity: EventPublicity::Public,
            summary,
        })?;
        let state = self
            .world
            .regional_settlements
            .get_mut(&site)
            .expect("regional settlement exists");
        state.shortage = shortage;
        state.unrest = if shortage {
            state.unrest.saturating_add(8).min(100)
        } else {
            state.unrest.saturating_sub(10)
        };
        state.last_event = event;
        if !shortage {
            self.close_open_goals(RegionalGoalKind::RelieveShortage(site), event);
        }
        Ok(Some(event))
    }

    /// Advances every persistent regional party by an abstract route step. The
    /// desktop client calls this after authoritative game turns; the monthly
    /// party system uses a full-route step during off-screen history simulation.
    pub fn advance_regional_parties(&mut self, step: u16) -> Result<Vec<EventId>, WorldError> {
        let parties = self
            .world
            .regional_parties()
            .values()
            .filter(|party| party.status == RegionalPartyStatus::Traveling)
            .map(|party| party.id)
            .collect::<Vec<_>>();
        let mut events = Vec::new();
        for party in parties {
            if let Some(event) = self.advance_regional_party(party, step)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    /// Advances each traveling party by roughly one generated atlas cell.
    ///
    /// Unlike percentage-based stepping, this makes a long mountain road take
    /// longer than a short neighboring route and keeps regional movement in
    /// the same spatial units as player exploration.
    pub fn advance_regional_parties_one_tile(&mut self) -> Result<Vec<EventId>, WorldError> {
        let parties = self
            .world
            .regional_parties()
            .values()
            .filter(|party| party.status == RegionalPartyStatus::Traveling)
            .map(|party| party.id)
            .collect::<Vec<_>>();
        let mut events = Vec::new();
        for party in parties {
            let route = self.world.regional_parties()[&party].route;
            let route_steps = self.world.routes()[&route]
                .path
                .len()
                .saturating_sub(1)
                .max(1);
            let step = 1_000_usize.div_ceil(route_steps).min(1_000) as u16;
            if let Some(event) = self.advance_regional_party(party, step)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    fn advance_regional_party(
        &mut self,
        party_id: PartyId,
        step: u16,
    ) -> Result<Option<EventId>, WorldError> {
        let Some(party) = self.world.regional_parties().get(&party_id).cloned() else {
            return Ok(None);
        };
        if party.status != RegionalPartyStatus::Traveling {
            return Ok(None);
        }
        let progress = party.progress.saturating_add(step).min(1_000);
        self.world
            .regional_parties
            .get_mut(&party_id)
            .expect("regional party exists")
            .progress = progress;
        if progress < 1_000 {
            return Ok(None);
        }

        let destination_name = self.world.sites()[&party.destination].name.clone();
        let route_name = self.world.routes()[&party.route].name.clone();
        let (status, consequences, summary) = match party.kind {
            RegionalPartyKind::TradeCaravan { resource, amount } => {
                let shortage = self.world.regional_settlements()[&party.destination].shortage;
                let mut consequences = vec![Consequence::ChangeResource {
                    site: party.destination,
                    resource,
                    amount,
                }];
                if shortage {
                    consequences.push(Consequence::ShiftStrategicFront {
                        front: StrategicFront::Economy,
                        amount: 1,
                    });
                }
                (
                    RegionalPartyStatus::Arrived,
                    consequences,
                    format!(
                        "{} reached {destination_name} with {amount} {resource:?}",
                        party.name
                    ),
                )
            }
            RegionalPartyKind::ReturningCaravan => (
                RegionalPartyStatus::Arrived,
                Vec::new(),
                format!(
                    "{} returned to {destination_name} after completing its journey",
                    party.name
                ),
            ),
            RegionalPartyKind::Refugees { population } => (
                RegionalPartyStatus::Arrived,
                vec![Consequence::ChangeRegionalPopulation {
                    site: party.destination,
                    amount: population as i32,
                }],
                format!(
                    "{} reached {destination_name}; {population} displaced people entered the settlement",
                    party.name
                ),
            ),
            RegionalPartyKind::Patrol { .. } => (
                RegionalPartyStatus::Stationed,
                Vec::new(),
                format!(
                    "{} reached its watch on {route_name} and established a patrol camp",
                    party.name
                ),
            ),
            RegionalPartyKind::Raiders { .. } => (
                RegionalPartyStatus::Stationed,
                Vec::new(),
                format!("{} occupied a choke point on {route_name}", party.name),
            ),
        };
        let mut participants = vec![
            EntityRef::Party(party.id),
            EntityRef::Route(party.route),
            EntityRef::Site(party.destination),
        ];
        if let Some(faction) = party.faction {
            participants.push(EntityRef::Faction(faction));
        }
        if let Some(leader) = party.leader {
            participants.push(EntityRef::Person(leader));
        }
        let event = self.world.record_event(EventDraft {
            location: party.destination,
            kind: HistoricalEventKind::RegionalPartyArrived,
            participants,
            causes: vec![party.last_event],
            consequences,
            witnesses: self.project_witnesses(party.destination),
            principle: None,
            publicity: EventPublicity::Public,
            summary,
        })?;
        let record = self
            .world
            .regional_parties
            .get_mut(&party_id)
            .expect("regional party exists");
        if matches!(party.kind, RegionalPartyKind::TradeCaravan { .. }) {
            record.kind = RegionalPartyKind::ReturningCaravan;
            record.status = RegionalPartyStatus::Traveling;
            record.origin = party.destination;
            record.destination = party.origin;
            record.progress = 0;
        } else {
            record.status = status;
        }
        record.last_event = event;
        self.world
            .regional_settlements
            .get_mut(&party.destination)
            .expect("regional settlement exists")
            .last_event = event;
        Ok(Some(event))
    }

    pub fn defeat_regional_party(&mut self, party_id: PartyId) -> Result<EventId, WorldError> {
        let party = self
            .world
            .regional_parties()
            .get(&party_id)
            .cloned()
            .ok_or(WorldError::MissingParty(party_id))?;
        if matches!(
            party.status,
            RegionalPartyStatus::Arrived | RegionalPartyStatus::Defeated
        ) {
            return Err(WorldError::RegionalPartyInactive(party_id));
        }
        let event = self.world.record_event(EventDraft {
            location: party.origin,
            kind: HistoricalEventKind::RegionalPartyDefeated,
            participants: vec![
                EntityRef::Party(party.id),
                EntityRef::Route(party.route),
                EntityRef::Site(party.origin),
                EntityRef::Site(party.destination),
            ],
            causes: vec![party.last_event],
            consequences: if matches!(party.kind, RegionalPartyKind::Raiders { .. }) {
                vec![
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Military,
                        amount: 1,
                    },
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Territory,
                        amount: 1,
                    },
                ]
            } else {
                Vec::new()
            },
            witnesses: self.project_witnesses(party.origin),
            principle: Some(Principle::Courage),
            publicity: EventPublicity::Public,
            summary: format!(
                "{} was defeated on {}",
                party.name,
                self.world.routes()[&party.route].name
            ),
        })?;
        let record = self
            .world
            .regional_parties
            .get_mut(&party_id)
            .expect("regional party exists");
        record.status = RegionalPartyStatus::Defeated;
        record.last_event = event;
        Ok(event)
    }

    fn prepare_party_leader(
        &mut self,
        faction: FactionId,
        origin: SiteId,
        occupation: Occupation,
    ) -> Option<(PersonId, String, Option<Person>)> {
        let available = self
            .world
            .regional_parties()
            .values()
            .rev()
            .find(|party| {
                party.status == RegionalPartyStatus::Arrived
                    && party.destination == origin
                    && party.faction == Some(faction)
            })
            .and_then(|party| party.leader)
            .filter(|leader| {
                self.world
                    .people()
                    .get(leader)
                    .is_some_and(|person| person.is_alive() && person.occupation == occupation)
            });
        if let Some(leader) = available {
            return Some((leader, self.full_name(leader), None));
        }
        let busy = self
            .world
            .regional_parties()
            .values()
            .filter(|party| {
                matches!(
                    party.status,
                    RegionalPartyStatus::Traveling | RegionalPartyStatus::Stationed
                )
            })
            .filter_map(|party| party.leader)
            .collect::<BTreeSet<_>>();
        let faction_leader = self.world.factions()[&faction].leader;
        let available_resident = self
            .world
            .people()
            .values()
            .filter(|person| {
                person.is_alive()
                    && person.home == origin
                    && person.faction == faction
                    && person.id != faction_leader
                    && !busy.contains(&person.id)
            })
            .max_by_key(|person| (person.occupation == occupation, person.id))
            .map(|person| person.id);
        if let Some(leader) = available_resident {
            return Some((leader, self.full_name(leader), None));
        }
        let named_residents = self
            .world
            .living_people()
            .filter(|person| person.home == origin)
            .count() as u32;
        let has_anonymous_resident =
            self.world.regional_settlements()[&origin].population > named_residents;
        if !has_anonymous_resident {
            return None;
        }

        let family = self
            .world
            .factions()
            .get(&faction)
            .expect("regional controller is a faction")
            .members
            .iter()
            .filter_map(|person| self.world.people().get(person))
            .min_by_key(|person| person.id)
            .expect("seeded factions have named members")
            .family;
        let id = self.world.allocate_person_id();
        let name_index =
            (self.world.campaign_seed ^ id.0.rotate_left(17)) as usize % GIVEN_NAMES.len();
        let person = Person {
            id,
            given_name: GIVEN_NAMES[name_index].to_string(),
            family,
            born: WorldDate::new(
                self.world.date.year.saturating_sub(25),
                self.world.date.month,
            ),
            died: None,
            parents: Vec::new(),
            occupation,
            faction,
            home: origin,
            drives: BTreeMap::from([
                (Drive::Survival, 75),
                (Drive::Family, 55),
                (Drive::Loyalty, 65),
                (Drive::Wealth, 45),
            ]),
        };
        let name = format!(
            "{} {}",
            person.given_name,
            self.world.families()[&person.family].surname
        );
        Some((id, name, Some(person)))
    }

    fn move_regional_trade(
        &mut self,
        route: RouteId,
        from: SiteId,
        to: SiteId,
        resource: ResourceKind,
        requested: i64,
    ) -> Result<Option<EventId>, WorldError> {
        let route_state = self
            .world
            .routes()
            .get(&route)
            .cloned()
            .ok_or(WorldError::MissingRoute(route))?;
        if route_state.disrupted
            || !route_state.connects(from)
            || route_state.other_end(from) != Some(to)
        {
            return Ok(None);
        }
        let stock = self.world.sites()[&from]
            .resources
            .get(&resource)
            .copied()
            .unwrap_or_default();
        let reserve = self.world.regional_settlements()[&from]
            .monthly_consumption
            .get(&resource)
            .copied()
            .unwrap_or_default()
            .saturating_mul(2);
        let amount = requested.min(stock.saturating_sub(reserve)).max(0);
        if amount == 0 {
            return Ok(None);
        }
        let from_name = self.world.sites()[&from].name.clone();
        let to_name = self.world.sites()[&to].name.clone();
        let destination = self.world.regional_settlements()[&to].clone();
        let origin = self.world.regional_settlements()[&from].clone();
        let faction = self.world.regional_settlements()[&from].controller;
        let party_id = self.world.allocate_party_id();
        let Some((leader_id, leader_name, new_leader)) =
            self.prepare_party_leader(faction, from, Occupation::Merchant)
        else {
            return Ok(None);
        };
        let party_name = format!("{leader_name}'s caravan");
        let mut causes = vec![
            route_state.last_event,
            origin.last_event,
            destination.last_event,
        ];
        causes.sort_unstable();
        causes.dedup();
        let consequences = new_leader
            .map(|leader| Consequence::AddPerson(Box::new(leader)))
            .into_iter()
            .chain([Consequence::ChangeResource {
                site: from,
                resource,
                amount: -amount,
            }])
            .collect();
        let event = self.world.record_event(EventDraft {
            location: from,
            kind: HistoricalEventKind::RegionalTrade,
            participants: vec![
                EntityRef::Party(party_id),
                EntityRef::Person(leader_id),
                EntityRef::Route(route),
                EntityRef::Site(from),
                EntityRef::Site(to),
            ],
            causes,
            consequences,
            witnesses: self.project_witnesses(to),
            principle: Some(Principle::Stewardship),
            publicity: EventPublicity::Public,
            summary: format!(
                "{party_name} departed {from_name} with {amount} {resource:?} for {to_name} along {}",
                route_state.name
            ),
        })?;
        self.world.insert_regional_party(RegionalParty {
            id: party_id,
            name: party_name,
            kind: RegionalPartyKind::TradeCaravan { resource, amount },
            status: RegionalPartyStatus::Traveling,
            faction: Some(faction),
            leader: Some(leader_id),
            route,
            origin: from,
            destination: to,
            progress: 0,
            created: self.world.date,
            cause: route_state.last_event,
            last_event: event,
        });
        self.world
            .routes
            .get_mut(&route)
            .expect("regional route exists")
            .last_event = event;
        self.world
            .regional_settlements
            .get_mut(&from)
            .expect("regional settlement exists")
            .last_event = event;
        Ok(Some(event))
    }

    fn seed_regional_traffic(&mut self) -> Result<Option<EventId>, WorldError> {
        const DESIRED_PARTIES: usize = 3;
        const TRADE_RESOURCES: [ResourceKind; 4] = [
            ResourceKind::Food,
            ResourceKind::Timber,
            ResourceKind::Iron,
            ResourceKind::Medicine,
        ];

        let active_parties = self
            .world
            .regional_parties()
            .values()
            .filter(|party| party.status == RegionalPartyStatus::Traveling)
            .count();
        if active_parties >= DESIRED_PARTIES {
            return Ok(None);
        }

        let routes = self.world.routes().values().cloned().collect::<Vec<_>>();
        let mut first_event = None;
        let mut created = active_parties;
        for route in routes.into_iter().filter(|route| !route.disrupted) {
            if created >= DESIRED_PARTIES {
                break;
            }
            for resource in TRADE_RESOURCES {
                let first_surplus = self.regional_trade_surplus(route.first, resource);
                let second_surplus = self.regional_trade_surplus(route.second, resource);
                let (from, to, surplus) = if first_surplus >= second_surplus {
                    (route.first, route.second, first_surplus)
                } else {
                    (route.second, route.first, second_surplus)
                };
                if surplus < 4 {
                    continue;
                }
                let requested = (surplus / 4).clamp(4, 20);
                if let Some(event) =
                    self.move_regional_trade(route.id, from, to, resource, requested)?
                {
                    first_event.get_or_insert(event);
                    created += 1;
                    break;
                }
            }
        }
        Ok(first_event)
    }

    fn regional_trade_surplus(&self, site: SiteId, resource: ResourceKind) -> i64 {
        let stock = self.world.sites()[&site]
            .resources
            .get(&resource)
            .copied()
            .unwrap_or_default();
        let reserve = self.world.regional_settlements()[&site]
            .monthly_consumption
            .get(&resource)
            .copied()
            .unwrap_or_default()
            .saturating_mul(2);
        stock.saturating_sub(reserve)
    }

    fn apply_regional_pressure(&mut self, route: RouteId) -> Result<Option<EventId>, WorldError> {
        let route_state = self
            .world
            .routes()
            .get(&route)
            .cloned()
            .ok_or(WorldError::MissingRoute(route))?;
        if route_state.disrupted {
            let repair_months = 2 + (100_u8.saturating_sub(route_state.condition) / 20);
            let state = self
                .world
                .routes
                .get_mut(&route)
                .expect("regional route exists");
            state.disrupted_months = state.disrupted_months.saturating_add(1);
            if state.disrupted_months < repair_months {
                return Ok(None);
            }
            let first_name = self.world.sites()[&route_state.first].name.clone();
            let second_name = self.world.sites()[&route_state.second].name.clone();
            let event = self.world.record_event(EventDraft {
                location: route_state.first,
                kind: HistoricalEventKind::RouteReopened,
                participants: vec![
                    EntityRef::Route(route),
                    EntityRef::Site(route_state.first),
                    EntityRef::Site(route_state.second),
                ],
                causes: vec![route_state.last_event],
                consequences: vec![
                    Consequence::SetRouteDisrupted {
                        route,
                        disrupted: false,
                    },
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Military,
                        amount: 1,
                    },
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Economy,
                        amount: 1,
                    },
                ],
                witnesses: self.project_witnesses(route_state.first),
                principle: Some(Principle::Duty),
                publicity: EventPublicity::Public,
                summary: format!(
                    "Patrols reopened {} between {first_name} and {second_name} after {} months of disruption",
                    route_state.name, route_state.disrupted_months
                ),
            })?;
            self.world
                .routes
                .get_mut(&route)
                .expect("regional route exists")
                .last_event = event;
            self.close_open_goals(RegionalGoalKind::SecureRoute(route), event);
            self.retire_route_parties(route, event, false);
            return Ok(Some(event));
        }

        let first_food = self.world.sites()[&route_state.first].resources[&ResourceKind::Food];
        let second_food = self.world.sites()[&route_state.second].resources[&ResourceKind::Food];
        let first_loss = first_food.min(10);
        let second_loss = second_food.min(10);
        let dark_power = self.world.struggle().dark_power.clone();
        let event = self.world.record_event(EventDraft {
            location: route_state.first,
            kind: HistoricalEventKind::RouteDisrupted,
            participants: vec![
                EntityRef::Route(route),
                EntityRef::Site(route_state.first),
                EntityRef::Site(route_state.second),
            ],
            causes: vec![route_state.last_event],
            consequences: vec![
                Consequence::SetRouteDisrupted {
                    route,
                    disrupted: true,
                },
                Consequence::ChangeResource {
                    site: route_state.first,
                    resource: ResourceKind::Food,
                    amount: -first_loss,
                },
                Consequence::ChangeResource {
                    site: route_state.second,
                    resource: ResourceKind::Food,
                    amount: -second_loss,
                },
                Consequence::ShiftStrategicFront {
                    front: StrategicFront::Military,
                    amount: -1,
                },
                Consequence::ShiftStrategicFront {
                    front: StrategicFront::Economy,
                    amount: -1,
                },
                Consequence::ShiftStrategicFront {
                    front: StrategicFront::Territory,
                    amount: -1,
                },
            ],
            witnesses: self.project_witnesses(route_state.first),
            principle: Some(Principle::Courage),
            publicity: EventPublicity::Public,
            summary: format!(
                "Raiders serving {dark_power} disrupted {}, destroying supplies and isolating both settlements",
                route_state.name
            ),
        })?;
        self.world
            .routes
            .get_mut(&route)
            .expect("regional route exists")
            .last_event = event;
        let party_id = self.world.allocate_party_id();
        self.world.insert_regional_party(RegionalParty {
            id: party_id,
            name: format!("{dark_power} raiding band"),
            kind: RegionalPartyKind::Raiders {
                strength: route_state.danger.max(10),
            },
            status: RegionalPartyStatus::Traveling,
            faction: None,
            leader: None,
            route,
            origin: route_state.first,
            destination: route_state.second,
            progress: 350,
            created: self.world.date,
            cause: event,
            last_event: event,
        });
        Ok(Some(event))
    }

    fn migrate_regional_population(
        &mut self,
        from: SiteId,
        to: SiteId,
        requested: u32,
    ) -> Result<Option<EventId>, WorldError> {
        let origin = self
            .world
            .regional_settlements()
            .get(&from)
            .cloned()
            .ok_or(WorldError::MissingSite(from))?;
        let destination = self
            .world
            .regional_settlements()
            .get(&to)
            .cloned()
            .ok_or(WorldError::MissingSite(to))?;
        let route = self
            .world
            .routes()
            .values()
            .find(|route| route.connects(from) && route.other_end(from) == Some(to))
            .cloned();
        let Some(route) = route.filter(|route| !route.disrupted) else {
            return Ok(None);
        };
        if !origin.shortage {
            return Ok(None);
        }
        let named_residents = self
            .world
            .living_people()
            .filter(|person| person.home == from)
            .count() as u32;
        // Keep one aggregate place available in case the anonymous guide is
        // materialized as a named person by this departure.
        let movable = origin
            .population
            .saturating_sub(named_residents.saturating_add(1));
        let amount = requested.min(movable);
        if amount == 0 {
            return Ok(None);
        }
        let from_name = self.world.sites()[&from].name.clone();
        let to_name = self.world.sites()[&to].name.clone();
        let faction = origin.controller;
        let party_id = self.world.allocate_party_id();
        let Some((leader_id, leader_name, new_leader)) =
            self.prepare_party_leader(faction, from, Occupation::Laborer)
        else {
            return Ok(None);
        };
        let party_name = format!("{leader_name}'s refugee column");
        let mut causes = vec![origin.last_event, destination.last_event, route.last_event];
        if let Some(shortage) = self.world.events().values().rev().find(|event| {
            event.kind == HistoricalEventKind::RegionalShortage && event.location == from
        }) {
            causes.push(shortage.id);
        }
        causes.sort_unstable();
        causes.dedup();
        let event = self.world.record_event(EventDraft {
            location: from,
            kind: HistoricalEventKind::Migration,
            participants: vec![
                EntityRef::Party(party_id),
                EntityRef::Person(leader_id),
                EntityRef::Route(route.id),
                EntityRef::Site(from),
                EntityRef::Site(to),
            ],
            causes,
            consequences: new_leader
                .map(|leader| Consequence::AddPerson(Box::new(leader)))
                .into_iter()
                .chain([
                    Consequence::ChangeRegionalPopulation {
                    site: from,
                    amount: -(amount as i32),
                    },
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Political,
                        amount: -1,
                    },
                ])
                .collect(),
            witnesses: self.project_witnesses(to),
            principle: Some(Principle::Compassion),
            publicity: EventPublicity::Public,
            summary: format!(
                "{party_name}, {amount} people, departed shortage-struck {from_name} for {to_name} along {}",
                route.name
            ),
        })?;
        self.world.insert_regional_party(RegionalParty {
            id: party_id,
            name: party_name,
            kind: RegionalPartyKind::Refugees { population: amount },
            status: RegionalPartyStatus::Traveling,
            faction: Some(faction),
            leader: Some(leader_id),
            route: route.id,
            origin: from,
            destination: to,
            progress: 0,
            created: self.world.date,
            cause: origin.last_event,
            last_event: event,
        });
        let settlement = self
            .world
            .regional_settlements
            .get_mut(&from)
            .expect("regional settlement exists");
        settlement.last_event = event;
        settlement.unrest = settlement.unrest.saturating_add(3).min(100);
        self.world
            .routes
            .get_mut(&route.id)
            .expect("regional route exists")
            .last_event = event;
        Ok(Some(event))
    }

    fn propose_route_goal(&mut self, route: RouteId) -> Result<Option<EventId>, WorldError> {
        let route_state = self
            .world
            .routes()
            .get(&route)
            .cloned()
            .ok_or(WorldError::MissingRoute(route))?;
        if !route_state.disrupted
            || self.world.regional_goals().values().any(|goal| {
                goal.status == RegionalGoalStatus::Open
                    && goal.kind == RegionalGoalKind::SecureRoute(route)
            })
        {
            return Ok(None);
        }
        let sponsor = self.world.regional_settlements()[&route_state.first].controller;
        let id = self.world.allocate_goal_id();
        let title = format!("Secure {}", route_state.name);
        let description = format!(
            "{} asks for help restoring or deciding the fate of the blocked road between {} and {}.",
            self.world.factions()[&sponsor].name,
            self.world.sites()[&route_state.first].name,
            self.world.sites()[&route_state.second].name
        );
        let party_id = self.world.allocate_party_id();
        let Some((leader_id, leader_name, new_leader)) =
            self.prepare_party_leader(sponsor, route_state.first, Occupation::Guard)
        else {
            return Ok(None);
        };
        let event = self.world.record_event(EventDraft {
            location: route_state.first,
            kind: HistoricalEventKind::RegionalGoalProposed,
            participants: vec![
                EntityRef::Goal(id),
                EntityRef::Party(party_id),
                EntityRef::Person(leader_id),
                EntityRef::Route(route),
                EntityRef::Faction(sponsor),
            ],
            causes: vec![route_state.last_event],
            consequences: new_leader
                .map(|leader| Consequence::AddPerson(Box::new(leader)))
                .into_iter()
                .collect(),
            witnesses: self.project_witnesses(route_state.first),
            principle: Some(self.world.factions()[&sponsor].principle),
            publicity: EventPublicity::Public,
            summary: format!(
                "{} issued a regional contract to secure {}",
                self.world.factions()[&sponsor].name,
                route_state.name
            ),
        })?;
        self.world.insert_regional_goal(RegionalGoal {
            id,
            kind: RegionalGoalKind::SecureRoute(route),
            sponsor,
            created: self.world.date,
            cause: route_state.last_event,
            status: RegionalGoalStatus::Open,
            title,
            description,
            resolved_by: None,
        });
        self.world.insert_regional_party(RegionalParty {
            id: party_id,
            name: format!("{leader_name}'s road patrol"),
            kind: RegionalPartyKind::Patrol { goal: id },
            status: RegionalPartyStatus::Traveling,
            faction: Some(sponsor),
            leader: Some(leader_id),
            route,
            origin: route_state.first,
            destination: route_state.second,
            progress: 0,
            created: self.world.date,
            cause: event,
            last_event: event,
        });
        Ok(Some(event))
    }

    fn propose_relief_goal(&mut self, site: SiteId) -> Result<Option<EventId>, WorldError> {
        let settlement = self
            .world
            .regional_settlements()
            .get(&site)
            .cloned()
            .ok_or(WorldError::MissingSite(site))?;
        if !settlement.shortage
            || self.world.regional_goals().values().any(|goal| {
                goal.status == RegionalGoalStatus::Open
                    && goal.kind == RegionalGoalKind::RelieveShortage(site)
            })
        {
            return Ok(None);
        }
        let id = self.world.allocate_goal_id();
        let site_name = self.world.sites()[&site].name.clone();
        let title = format!("Relieve {site_name}");
        let description = format!(
            "{} seeks food for {site_name}, where reserves, migration, and unrest are worsening.",
            self.world.factions()[&settlement.controller].name
        );
        let event = self.world.record_event(EventDraft {
            location: site,
            kind: HistoricalEventKind::RegionalGoalProposed,
            participants: vec![
                EntityRef::Goal(id),
                EntityRef::Site(site),
                EntityRef::Faction(settlement.controller),
            ],
            causes: vec![settlement.last_event],
            consequences: Vec::new(),
            witnesses: self.project_witnesses(site),
            principle: Some(Principle::Compassion),
            publicity: EventPublicity::Public,
            summary: format!(
                "{} issued a regional relief contract for {site_name}",
                self.world.factions()[&settlement.controller].name
            ),
        })?;
        self.world.insert_regional_goal(RegionalGoal {
            id,
            kind: RegionalGoalKind::RelieveShortage(site),
            sponsor: settlement.controller,
            created: self.world.date,
            cause: settlement.last_event,
            status: RegionalGoalStatus::Open,
            title,
            description,
            resolved_by: None,
        });
        Ok(Some(event))
    }

    pub fn regional_goal_options(
        &self,
        goal: GoalId,
    ) -> Result<Vec<RegionalGoalOption>, WorldError> {
        let goal = self
            .world
            .regional_goals()
            .get(&goal)
            .ok_or(WorldError::MissingGoal(goal))?;
        if goal.status != RegionalGoalStatus::Open {
            return Err(WorldError::RegionalGoalAlreadyResolved(goal.id));
        }
        let options = match goal.kind {
            RegionalGoalKind::SecureRoute(_) => vec![
                RegionalGoalOption {
                    approach: RegionalGoalApproach::RestoreByForce,
                    title: "Lead a patrol".to_string(),
                    description:
                        "Break the blockade and establish patrols. Costs coin, favors military control."
                            .to_string(),
                },
                RegionalGoalOption {
                    approach: RegionalGoalApproach::NegotiatePassage,
                    title: "Negotiate passage".to_string(),
                    description:
                        "Pay and bargain for safe traffic. Improves political legitimacy but leaves concessions."
                            .to_string(),
                },
                RegionalGoalOption {
                    approach: RegionalGoalApproach::ExploitDisruption,
                    title: "Exploit the blockade".to_string(),
                    description:
                        "Take abandoned supplies and leave the road unsafe. Profitable locally, harmful regionally."
                            .to_string(),
                },
            ],
            RegionalGoalKind::RelieveShortage(_) => vec![
                RegionalGoalOption {
                    approach: RegionalGoalApproach::DeliverRelief,
                    title: "Purchase relief".to_string(),
                    description:
                        "Use coalition coin to deliver enough food for several months.".to_string(),
                },
                RegionalGoalOption {
                    approach: RegionalGoalApproach::DivertShipment,
                    title: "Divert a shipment".to_string(),
                    description:
                        "Redirect another settlement's reserves without its consent.".to_string(),
                },
                RegionalGoalOption {
                    approach: RegionalGoalApproach::EnforceRationing,
                    title: "Enforce rationing".to_string(),
                    description:
                        "Stretch existing stores through severe restrictions, increasing resentment."
                            .to_string(),
                },
            ],
        };
        Ok(options)
    }

    pub fn active_route_raiders(&self, route: RouteId) -> Vec<PartyId> {
        self.world
            .regional_parties()
            .values()
            .filter(|party| {
                party.route == route
                    && matches!(party.kind, RegionalPartyKind::Raiders { .. })
                    && matches!(
                        party.status,
                        RegionalPartyStatus::Traveling | RegionalPartyStatus::Stationed
                    )
            })
            .map(|party| party.id)
            .collect()
    }

    pub fn resolve_regional_goal(
        &mut self,
        goal_id: GoalId,
        approach: RegionalGoalApproach,
    ) -> Result<RegionalGoalOutcome, WorldError> {
        let goal = self
            .world
            .regional_goals()
            .get(&goal_id)
            .cloned()
            .ok_or(WorldError::MissingGoal(goal_id))?;
        if goal.status != RegionalGoalStatus::Open {
            return Err(WorldError::RegionalGoalAlreadyResolved(goal_id));
        }
        if let (RegionalGoalKind::SecureRoute(route), RegionalGoalApproach::RestoreByForce) =
            (goal.kind, approach)
            && !self.active_route_raiders(route).is_empty()
        {
            return Err(WorldError::RegionalGoalRequiresCombat(goal_id));
        }
        let (location, summary, consequences) = match (goal.kind, approach) {
            (RegionalGoalKind::SecureRoute(route), RegionalGoalApproach::RestoreByForce) => {
                let route = &self.world.routes()[&route];
                let available_coin = self.world.sites()[&self.primary_site]
                    .resources
                    .get(&ResourceKind::Coin)
                    .copied()
                    .unwrap_or_default();
                (
                    route.first,
                    format!(
                        "The outsider led patrols that broke the blockade on {}",
                        route.name
                    ),
                    vec![
                        Consequence::SetRouteDisrupted {
                            route: route.id,
                            disrupted: false,
                        },
                        Consequence::ChangeResource {
                            site: self.primary_site,
                            resource: ResourceKind::Coin,
                            amount: -available_coin.min(8),
                        },
                        Consequence::ShiftStrategicFront {
                            front: StrategicFront::Military,
                            amount: 2,
                        },
                        Consequence::ShiftStrategicFront {
                            front: StrategicFront::Territory,
                            amount: 1,
                        },
                    ],
                )
            }
            (RegionalGoalKind::SecureRoute(route), RegionalGoalApproach::NegotiatePassage) => {
                let route = &self.world.routes()[&route];
                (
                    route.first,
                    format!(
                        "The outsider negotiated safe passage along {}, exchanging concessions for open traffic",
                        route.name
                    ),
                    vec![
                        Consequence::SetRouteDisrupted {
                            route: route.id,
                            disrupted: false,
                        },
                        Consequence::ChangeFactionTreasury {
                            faction: goal.sponsor,
                            amount: -5,
                        },
                        Consequence::ShiftStrategicFront {
                            front: StrategicFront::Political,
                            amount: 2,
                        },
                        Consequence::ShiftStrategicFront {
                            front: StrategicFront::Economy,
                            amount: 1,
                        },
                    ],
                )
            }
            (RegionalGoalKind::SecureRoute(route), RegionalGoalApproach::ExploitDisruption) => {
                let route = &self.world.routes()[&route];
                (
                    route.first,
                    format!(
                        "The outsider stripped abandoned supplies from {} and left the blockade intact",
                        route.name
                    ),
                    vec![
                        Consequence::ChangeResource {
                            site: self.primary_site,
                            resource: ResourceKind::Food,
                            amount: 15,
                        },
                        Consequence::ShiftStrategicFront {
                            front: StrategicFront::Economy,
                            amount: -1,
                        },
                        Consequence::ShiftStrategicFront {
                            front: StrategicFront::Political,
                            amount: -2,
                        },
                    ],
                )
            }
            (RegionalGoalKind::RelieveShortage(site), RegionalGoalApproach::DeliverRelief) => {
                let food = self.world.sites()[&site].resources[&ResourceKind::Food];
                let need = self.world.regional_settlements()[&site].monthly_consumption
                    [&ResourceKind::Food];
                let amount = (need.saturating_mul(3) - food).max(need);
                let available_coin =
                    self.world.sites()[&self.primary_site].resources[&ResourceKind::Coin];
                (
                    site,
                    format!(
                        "The outsider purchased and delivered {amount} food to {}",
                        self.world.sites()[&site].name
                    ),
                    vec![
                        Consequence::ChangeResource {
                            site,
                            resource: ResourceKind::Food,
                            amount,
                        },
                        Consequence::ChangeResource {
                            site: self.primary_site,
                            resource: ResourceKind::Coin,
                            amount: -available_coin.min((amount / 3).max(1)),
                        },
                        Consequence::ShiftStrategicFront {
                            front: StrategicFront::Economy,
                            amount: 2,
                        },
                        Consequence::ShiftStrategicFront {
                            front: StrategicFront::Spiritual,
                            amount: 1,
                        },
                    ],
                )
            }
            (RegionalGoalKind::RelieveShortage(site), RegionalGoalApproach::DivertShipment) => {
                let source = self
                    .world
                    .regional_settlements()
                    .keys()
                    .copied()
                    .filter(|candidate| *candidate != site)
                    .max_by_key(|candidate| {
                        self.world.sites()[candidate].resources[&ResourceKind::Food]
                    })
                    .ok_or(WorldError::InvalidRegionalGoalApproach(goal_id))?;
                let food = self.world.sites()[&site].resources[&ResourceKind::Food];
                let need = self.world.regional_settlements()[&site].monthly_consumption
                    [&ResourceKind::Food];
                let requested = (need.saturating_mul(3) - food).max(need);
                let amount =
                    requested.min(self.world.sites()[&source].resources[&ResourceKind::Food]);
                let source_faction = self.world.regional_settlements()[&source].controller;
                let mut effects = vec![
                    Consequence::ChangeResource {
                        site: source,
                        resource: ResourceKind::Food,
                        amount: -amount,
                    },
                    Consequence::ChangeResource {
                        site,
                        resource: ResourceKind::Food,
                        amount,
                    },
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Economy,
                        amount: 1,
                    },
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Political,
                        amount: -2,
                    },
                ];
                if source_faction != goal.sponsor {
                    effects.push(Consequence::ChangeFactionRelation {
                        first: source_faction,
                        second: goal.sponsor,
                        amount: -12,
                    });
                }
                (
                    site,
                    format!(
                        "The outsider diverted {amount} food from {} to shortage-struck {}",
                        self.world.sites()[&source].name,
                        self.world.sites()[&site].name
                    ),
                    effects,
                )
            }
            (RegionalGoalKind::RelieveShortage(site), RegionalGoalApproach::EnforceRationing) => (
                site,
                format!(
                    "The outsider imposed severe rationing in {}, stretching its remaining stores",
                    self.world.sites()[&site].name
                ),
                vec![
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Economy,
                        amount: 1,
                    },
                    Consequence::ShiftStrategicFront {
                        front: StrategicFront::Political,
                        amount: -2,
                    },
                ],
            ),
            _ => return Err(WorldError::InvalidRegionalGoalApproach(goal_id)),
        };

        let event = self.world.record_event(EventDraft {
            location,
            kind: HistoricalEventKind::RegionalGoalResolved,
            participants: vec![
                EntityRef::Goal(goal_id),
                EntityRef::Faction(goal.sponsor),
                EntityRef::Site(location),
            ],
            causes: vec![goal.cause],
            consequences,
            witnesses: self.project_witnesses(location),
            principle: Some(self.world.factions()[&goal.sponsor].principle),
            publicity: EventPublicity::Public,
            summary: summary.clone(),
        })?;

        match goal.kind {
            RegionalGoalKind::SecureRoute(route) => {
                self.world
                    .routes
                    .get_mut(&route)
                    .expect("goal route exists")
                    .last_event = event;
                if !self.world.routes()[&route].disrupted {
                    self.retire_route_parties(
                        route,
                        event,
                        approach == RegionalGoalApproach::RestoreByForce,
                    );
                }
            }
            RegionalGoalKind::RelieveShortage(site) => {
                let food = self.world.sites()[&site].resources[&ResourceKind::Food];
                let settlement = self
                    .world
                    .regional_settlements
                    .get_mut(&site)
                    .expect("goal settlement exists");
                if approach == RegionalGoalApproach::EnforceRationing {
                    settlement
                        .monthly_consumption
                        .insert(ResourceKind::Food, (food / 3).max(1));
                    settlement.unrest = settlement.unrest.saturating_add(15).min(100);
                }
                let need = settlement.monthly_consumption[&ResourceKind::Food];
                settlement.shortage = food < need.saturating_mul(2);
                settlement.last_event = event;
            }
        }
        let record = self
            .world
            .regional_goals
            .get_mut(&goal_id)
            .expect("regional goal exists");
        record.status = RegionalGoalStatus::Resolved;
        record.resolved_by = Some(event);
        Ok(RegionalGoalOutcome {
            goal: goal_id,
            approach,
            event,
            summary,
        })
    }

    fn close_open_goals(&mut self, kind: RegionalGoalKind, event: EventId) {
        for goal in self
            .world
            .regional_goals
            .values_mut()
            .filter(|goal| goal.kind == kind && goal.status == RegionalGoalStatus::Open)
        {
            goal.status = RegionalGoalStatus::Resolved;
            goal.resolved_by = Some(event);
        }
    }

    fn retire_route_parties(&mut self, route: RouteId, event: EventId, defeated: bool) {
        for party in self.world.regional_parties.values_mut().filter(|party| {
            party.route == route
                && matches!(
                    party.kind,
                    RegionalPartyKind::Raiders { .. } | RegionalPartyKind::Patrol { .. }
                )
                && matches!(
                    party.status,
                    RegionalPartyStatus::Traveling | RegionalPartyStatus::Stationed
                )
        }) {
            party.status = if defeated && matches!(party.kind, RegionalPartyKind::Raiders { .. }) {
                RegionalPartyStatus::Defeated
            } else {
                RegionalPartyStatus::Arrived
            };
            party.last_event = event;
        }
    }

    fn start_project(&mut self, project: SettlementProject) -> Result<Option<EventId>, WorldError> {
        if !self.can_afford_project(&project, false) {
            let months = project.months_in_phase.saturating_add(1);
            if project.phase == SettlementProjectPhase::Planned && months >= 2 {
                let rival = self
                    .world
                    .projects()
                    .values()
                    .find(|candidate| {
                        candidate.id != project.id
                            && candidate.sponsor != project.sponsor
                            && matches!(
                                candidate.phase,
                                SettlementProjectPhase::Foundation
                                    | SettlementProjectPhase::Structure
                                    | SettlementProjectPhase::Completed
                            )
                    })
                    .map(|candidate| candidate.sponsor);
                let consequences = rival
                    .map(|rival| Consequence::ChangeFactionRelation {
                        first: project.sponsor,
                        second: rival,
                        amount: -6,
                    })
                    .into_iter()
                    .chain(std::iter::once(Consequence::ShiftStrategicFront {
                        front: StrategicFront::Political,
                        amount: -1,
                    }))
                    .collect();
                let event = self.world.record_event(EventDraft {
                    location: project.site,
                    kind: HistoricalEventKind::ProjectStalled,
                    participants: self.project_participants(&project),
                    causes: vec![project.last_event],
                    consequences,
                    witnesses: self.project_witnesses(project.site),
                    principle: Some(self.world.factions()[&project.sponsor].principle),
                    publicity: EventPublicity::Public,
                    summary: format!(
                        "{} stalled before work began because shared materials or funding were unavailable",
                        project.name
                    ),
                })?;
                let project = self
                    .world
                    .projects
                    .get_mut(&project.id)
                    .expect("project exists");
                project.phase = SettlementProjectPhase::Stalled;
                project.months_in_phase = 0;
                project.last_event = event;
                return Ok(Some(event));
            }
            self.world
                .projects
                .get_mut(&project.id)
                .expect("project exists")
                .months_in_phase = months;
            return Ok(None);
        }
        let mut consequences = project
            .material_costs
            .iter()
            .map(|(resource, amount)| Consequence::ChangeResource {
                site: project.site,
                resource: *resource,
                amount: -*amount,
            })
            .collect::<Vec<_>>();
        consequences.push(Consequence::ChangeFactionTreasury {
            faction: project.sponsor,
            amount: -project.funding_cost,
        });
        let event = self.world.record_event(EventDraft {
            location: project.site,
            kind: HistoricalEventKind::ProjectStarted,
            participants: self.project_participants(&project),
            causes: vec![project.last_event],
            consequences,
            witnesses: self.project_witnesses(project.site),
            principle: Some(self.world.factions()[&project.sponsor].principle),
            publicity: EventPublicity::Public,
            summary: format!(
                "{} committed materials, coin, and labor to begin {}",
                self.world.factions()[&project.sponsor].name,
                project.name
            ),
        })?;
        let project = self
            .world
            .projects
            .get_mut(&project.id)
            .expect("project exists");
        project.phase = SettlementProjectPhase::Foundation;
        project.progress_months = 1;
        project.months_in_phase = 0;
        project.last_event = event;
        Ok(Some(event))
    }

    fn progress_project(
        &mut self,
        project: SettlementProject,
    ) -> Result<Option<EventId>, WorldError> {
        let labor_rate = (project.workers.len() as u8).clamp(1, 2);
        let next_progress = project.progress_months.saturating_add(labor_rate);
        if next_progress < project.required_months {
            let project = self
                .world
                .projects
                .get_mut(&project.id)
                .expect("project exists");
            project.progress_months = next_progress;
            project.phase = if next_progress.saturating_mul(2) >= project.required_months {
                SettlementProjectPhase::Structure
            } else {
                SettlementProjectPhase::Foundation
            };
            return Ok(None);
        }
        let (resource, amount) = project_benefit(project.kind);
        let event = self.world.record_event(EventDraft {
            location: project.site,
            kind: HistoricalEventKind::ProjectCompleted,
            participants: self.project_participants(&project),
            causes: vec![project.last_event],
            consequences: vec![
                Consequence::ChangeResource {
                    site: project.site,
                    resource,
                    amount,
                },
                Consequence::ShiftStrategicFront {
                    front: project_strategic_front(project.kind),
                    amount: 2,
                },
            ],
            witnesses: self.project_witnesses(project.site),
            principle: Some(Principle::Stewardship),
            publicity: EventPublicity::Public,
            summary: format!(
                "{} completed {}; its operation changed the town's {:?} reserve",
                self.world.factions()[&project.sponsor].name,
                project.name,
                resource
            ),
        })?;
        let project = self
            .world
            .projects
            .get_mut(&project.id)
            .expect("project exists");
        project.phase = SettlementProjectPhase::Completed;
        project.progress_months = project.required_months;
        project.months_in_phase = 0;
        project.last_event = event;
        Ok(Some(event))
    }

    fn maybe_damage_project(
        &mut self,
        project: SettlementProject,
    ) -> Result<Option<EventId>, WorldError> {
        let months = project.months_in_phase.saturating_add(1);
        let pressure = self.world.sites()[&project.site]
            .resources
            .get(&ResourceKind::Food)
            .copied()
            .unwrap_or_default()
            < 140
            || self.world.factions()[&project.sponsor]
                .relations
                .values()
                .any(|relation| *relation < -10);
        let seeded_delay = 3 + ((self.world.campaign_seed ^ project.id.0) % 3) as u8;
        if project.damage_count > 0 || !pressure || months < seeded_delay {
            self.world
                .projects
                .get_mut(&project.id)
                .expect("project exists")
                .months_in_phase = months;
            return Ok(None);
        }
        let event = self.world.record_event(EventDraft {
            location: project.site,
            kind: HistoricalEventKind::ProjectDamaged,
            participants: self.project_participants(&project),
            causes: vec![project.last_event, project.related_event],
            consequences: vec![
                Consequence::CreatePhysicalEvidence {
                    site: project.site,
                    kind: PhysicalEvidenceKind::BurnedBuilding,
                    associated_person: None,
                    description: format!(
                        "Scorched and collapsed sections of {}, damaged while the town remained under pressure",
                        project.name
                    ),
                },
                Consequence::ShiftStrategicFront {
                    front: project_strategic_front(project.kind),
                    amount: -2,
                },
            ],
            witnesses: self.project_witnesses(project.site),
            principle: Some(Principle::Responsibility),
            publicity: EventPublicity::Public,
            summary: format!(
                "{} was damaged amid scarcity and unresolved factional hostility",
                project.name
            ),
        })?;
        let project = self
            .world
            .projects
            .get_mut(&project.id)
            .expect("project exists");
        project.phase = SettlementProjectPhase::Damaged;
        project.months_in_phase = 0;
        project.damage_count = project.damage_count.saturating_add(1);
        project.last_event = event;
        Ok(Some(event))
    }

    fn maybe_repair_project(
        &mut self,
        project: SettlementProject,
    ) -> Result<Option<EventId>, WorldError> {
        let months = project.months_in_phase.saturating_add(1);
        if months < 2 || !self.can_afford_project(&project, true) {
            self.world
                .projects
                .get_mut(&project.id)
                .expect("project exists")
                .months_in_phase = months;
            return Ok(None);
        }
        let mut consequences = project
            .material_costs
            .iter()
            .map(|(resource, amount)| Consequence::ChangeResource {
                site: project.site,
                resource: *resource,
                amount: -repair_cost(*amount),
            })
            .collect::<Vec<_>>();
        consequences.push(Consequence::ChangeFactionTreasury {
            faction: project.sponsor,
            amount: -repair_cost(project.funding_cost),
        });
        consequences.push(Consequence::ShiftStrategicFront {
            front: project_strategic_front(project.kind),
            amount: 1,
        });
        let event = self.world.record_event(EventDraft {
            location: project.site,
            kind: HistoricalEventKind::ProjectRepaired,
            participants: self.project_participants(&project),
            causes: vec![project.last_event],
            consequences,
            witnesses: self.project_witnesses(project.site),
            principle: Some(Principle::Stewardship),
            publicity: EventPublicity::Public,
            summary: format!(
                "{} funded repairs to {} rather than abandon it",
                self.world.factions()[&project.sponsor].name,
                project.name
            ),
        })?;
        let project = self
            .world
            .projects
            .get_mut(&project.id)
            .expect("project exists");
        project.phase = SettlementProjectPhase::Structure;
        project.progress_months = project.required_months.saturating_sub(1);
        project.months_in_phase = 0;
        project.last_event = event;
        Ok(Some(event))
    }

    fn assess_grand_strategy(&mut self) -> Result<Option<EventId>, WorldError> {
        if self.world.date.month != 12 {
            return Ok(None);
        }
        let defending_kind = self
            .world
            .routes()
            .values()
            .find(|route| route.disrupted)
            .map(|route| StrategicObjectiveKind::RestoreRoute(route.id))
            .or_else(|| {
                self.world
                    .regional_settlements()
                    .values()
                    .filter(|settlement| settlement.shortage)
                    .max_by_key(|settlement| (settlement.unrest, settlement.site))
                    .map(|settlement| StrategicObjectiveKind::RelieveSettlement(settlement.site))
            })
            .unwrap_or(StrategicObjectiveKind::ConsolidateInfluence(
                self.primary_site,
            ));
        let dark_kind = self
            .world
            .routes()
            .values()
            .filter(|route| !route.disrupted)
            .max_by_key(|route| (route.danger, route.id))
            .map(|route| StrategicObjectiveKind::DisruptRoute(route.id))
            .or_else(|| {
                self.world
                    .regional_settlements()
                    .values()
                    .max_by_key(|settlement| (settlement.unrest, settlement.site))
                    .map(|settlement| StrategicObjectiveKind::ExploitSettlement(settlement.site))
            })
            .unwrap_or(StrategicObjectiveKind::ConsolidateInfluence(
                self.primary_site,
            ));
        let defending_front = strategic_objective_front(defending_kind);
        let dark_front = strategic_objective_front(dark_kind);
        let defending_cause =
            strategic_objective_cause(&self.world, defending_kind, self.foundation_event);
        let dark_cause = strategic_objective_cause(&self.world, dark_kind, self.foundation_event);

        let year = self.world.date.year as i64 as u64;
        let defending_actor = self
            .world
            .struggle()
            .actor(StrategicActorRole::DefendingCoalition);
        let dark_actor = self.world.struggle().actor(StrategicActorRole::DarkPower);
        let defending_roll = ((self.world.campaign_seed ^ year.rotate_left(11))
            .wrapping_mul(0x9e37_79b9)
            % 21) as i16;
        let dark_roll = ((self.world.campaign_seed.rotate_left(29) ^ year)
            .wrapping_mul(0xbf58_476d)
            % 21) as i16;
        let defending_score = i16::from(defending_actor.capacity)
            + (defending_actor.reserves / 5) as i16
            + defending_actor.influence / 4
            + defending_roll;
        let dark_score = i16::from(dark_actor.capacity)
            + (dark_actor.reserves / 5) as i16
            + dark_actor.influence / 4
            + dark_roll;
        let defending_won = defending_score >= dark_score;
        let winning_kind = if defending_won {
            defending_kind
        } else {
            dark_kind
        };
        let winning_front = strategic_objective_front(winning_kind);
        let mut consequences = strategic_objective_consequences(winning_kind, defending_won);
        consequences.push(Consequence::ShiftStrategicFront {
            front: winning_front,
            amount: if defending_won { 2 } else { -2 },
        });
        let defending_name = defending_actor.name.clone();
        let dark_name = dark_actor.name.clone();
        let mut causes = vec![
            self.world
                .struggle()
                .last_event
                .unwrap_or(self.foundation_event),
            defending_cause,
            dark_cause,
        ];
        causes.sort_unstable();
        causes.dedup();
        let event = self.world.record_event(EventDraft {
            location: self.primary_site,
            kind: HistoricalEventKind::StrategicBalanceShifted,
            participants: vec![EntityRef::Site(self.primary_site)],
            causes,
            consequences,
            witnesses: self.project_witnesses(self.primary_site),
            principle: Some(Principle::Courage),
            publicity: EventPublicity::Public,
            summary: format!(
                "{} pursued {} while {} pursued {}; {} gained the initiative",
                defending_name,
                strategic_objective_description(&self.world, defending_kind),
                dark_name,
                strategic_objective_description(&self.world, dark_kind),
                if defending_won {
                    &defending_name
                } else {
                    &dark_name
                }
            ),
        })?;

        match winning_kind {
            StrategicObjectiveKind::RestoreRoute(route) => {
                self.world
                    .routes
                    .get_mut(&route)
                    .expect("strategic route exists")
                    .last_event = event;
                self.close_open_goals(RegionalGoalKind::SecureRoute(route), event);
                self.retire_route_parties(route, event, false);
            }
            StrategicObjectiveKind::DisruptRoute(route) => {
                let route_state = self.world.routes()[&route].clone();
                self.world
                    .routes
                    .get_mut(&route)
                    .expect("strategic route exists")
                    .last_event = event;
                if self.active_route_raiders(route).is_empty() {
                    let party_id = self.world.allocate_party_id();
                    self.world.insert_regional_party(RegionalParty {
                        id: party_id,
                        name: format!("{dark_name} strategic raiding band"),
                        kind: RegionalPartyKind::Raiders {
                            strength: route_state.danger.max(10),
                        },
                        status: RegionalPartyStatus::Traveling,
                        faction: None,
                        leader: None,
                        route,
                        origin: route_state.first,
                        destination: route_state.second,
                        progress: 350,
                        created: self.world.date,
                        cause: event,
                        last_event: event,
                    });
                }
            }
            StrategicObjectiveKind::RelieveSettlement(site)
            | StrategicObjectiveKind::ExploitSettlement(site)
            | StrategicObjectiveKind::ConsolidateInfluence(site) => {
                if let Some(settlement) = self.world.regional_settlements.get_mut(&site) {
                    settlement.last_event = event;
                }
            }
        }
        let struggle = &mut self.world.struggle;
        struggle.last_event = Some(event);
        let defending_actor = struggle
            .actors
            .get_mut(&StrategicActorRole::DefendingCoalition)
            .expect("defending actor");
        defending_actor.objective = Some(StrategicObjective {
            kind: defending_kind,
            front: defending_front,
            started: self.world.date,
            cause: defending_cause,
            progress: if defending_won { 100 } else { 45 },
        });
        defending_actor.reserves =
            (defending_actor.reserves - if defending_won { 10 } else { 5 } + 8).clamp(0, 120);
        defending_actor.influence =
            (defending_actor.influence + if defending_won { 2 } else { -1 }).clamp(-100, 100);
        defending_actor.last_event = Some(event);
        let dark_actor = struggle
            .actors
            .get_mut(&StrategicActorRole::DarkPower)
            .expect("dark actor");
        dark_actor.objective = Some(StrategicObjective {
            kind: dark_kind,
            front: dark_front,
            started: self.world.date,
            cause: dark_cause,
            progress: if defending_won { 45 } else { 100 },
        });
        dark_actor.reserves =
            (dark_actor.reserves - if defending_won { 5 } else { 10 } + 8).clamp(0, 120);
        dark_actor.influence =
            (dark_actor.influence + if defending_won { -1 } else { 2 }).clamp(-100, 100);
        dark_actor.last_event = Some(event);
        Ok(Some(event))
    }

    fn can_afford_project(&self, project: &SettlementProject, repair: bool) -> bool {
        let site = &self.world.sites()[&project.site];
        project.material_costs.iter().all(|(resource, amount)| {
            let required = if repair {
                repair_cost(*amount)
            } else {
                *amount
            };
            site.resources.get(resource).copied().unwrap_or_default() >= required
        }) && self.world.factions()[&project.sponsor].treasury
            >= if repair {
                repair_cost(project.funding_cost)
            } else {
                project.funding_cost
            }
    }

    fn maybe_import_project_supplies(
        &mut self,
        id: ProjectId,
    ) -> Result<Option<EventId>, WorldError> {
        let project = self.world.projects()[&id].clone();
        if !matches!(
            project.phase,
            SettlementProjectPhase::Stalled | SettlementProjectPhase::Damaged
        ) || project.months_in_phase < 2
        {
            return Ok(None);
        }
        let site = &self.world.sites()[&project.site];
        let missing = project
            .material_costs
            .iter()
            .find_map(|(resource, required)| {
                let required = if project.phase == SettlementProjectPhase::Damaged {
                    repair_cost(*required)
                } else {
                    *required
                };
                let available = site.resources.get(resource).copied().unwrap_or_default();
                (available < required).then_some((*resource, required - available + 4))
            });
        let Some((resource, amount)) = missing else {
            return Ok(None);
        };
        let freight_cost = amount.saturating_mul(2);
        if self.world.factions()[&project.sponsor].treasury < freight_cost {
            return Ok(None);
        }
        let event = self.world.record_event(EventDraft {
            location: project.site,
            kind: HistoricalEventKind::SupplyShipment,
            participants: self.project_participants(&project),
            causes: vec![project.last_event],
            consequences: vec![
                Consequence::ChangeFactionTreasury {
                    faction: project.sponsor,
                    amount: -freight_cost,
                },
                Consequence::ChangeResource {
                    site: project.site,
                    resource,
                    amount,
                },
                Consequence::ShiftStrategicFront {
                    front: StrategicFront::Economy,
                    amount: 1,
                },
            ],
            witnesses: self.project_witnesses(project.site),
            principle: Some(self.world.factions()[&project.sponsor].principle),
            publicity: EventPublicity::Public,
            summary: format!(
                "{} paid for a shipment of {amount} {:?} so work on {} could resume",
                self.world.factions()[&project.sponsor].name,
                resource,
                project.name
            ),
        })?;
        self.world
            .projects
            .get_mut(&id)
            .expect("project exists")
            .last_event = event;
        Ok(Some(event))
    }

    fn project_participants(&self, project: &SettlementProject) -> Vec<EntityRef> {
        [
            EntityRef::Project(project.id),
            EntityRef::Faction(project.sponsor),
        ]
        .into_iter()
        .chain(project.workers.iter().copied().map(EntityRef::Person))
        .collect()
    }

    fn project_witnesses(&self, site: SiteId) -> Vec<PersonId> {
        self.world.sites()[&site]
            .population
            .iter()
            .copied()
            .collect()
    }

    pub fn crisis_resolution_options(
        &self,
        crisis: EventId,
    ) -> Result<Vec<CrisisResolutionOption>, WorldError> {
        let event = self
            .world
            .events()
            .get(&crisis)
            .ok_or(WorldError::MissingEvent(crisis))?;
        if self.world.events().values().any(|candidate| {
            candidate.kind == HistoricalEventKind::PlayerIntervention
                && candidate.causes.contains(&crisis)
        }) {
            return Ok(Vec::new());
        }
        let site = &self.world.sites()[&event.location];
        let Some(law) = site
            .laws
            .values()
            .filter(|law| law.active)
            .min_by_key(|law| law.id)
        else {
            return Err(WorldError::NoActiveCrisisLaw(site.id));
        };
        let authority = &self.world.factions()[&law.authority];
        let opposition = self
            .world
            .factions()
            .values()
            .filter(|faction| faction.id != authority.id)
            .min_by_key(|faction| {
                (
                    authority
                        .relations
                        .get(&faction.id)
                        .copied()
                        .unwrap_or_default(),
                    faction.id,
                )
            })
            .expect("generated crisis has multiple factions");
        let law_label = law_label(law.kind);

        Ok(vec![
            CrisisResolutionOption {
                kind: CrisisResolutionKind::EnforceEmergencyLaw,
                title: format!("UPHOLD {law_label}"),
                description: format!(
                    "Back {} and enforce the existing measure. The reserve is protected, but relations with {} will deteriorate.",
                    authority.name, opposition.name
                ),
                supported_faction: authority.id,
            },
            CrisisResolutionOption {
                kind: CrisisResolutionKind::OpenPublicStores,
                title: "OPEN THE PUBLIC STORES".to_string(),
                description: format!(
                    "Back {} and suspend {law_label}. Eighteen measures of food are released immediately, risking the later reserve.",
                    opposition.name
                ),
                supported_faction: opposition.id,
            },
            CrisisResolutionOption {
                kind: CrisisResolutionKind::BrokerCompromise,
                title: "BROKER SUPERVISED DISTRIBUTION".to_string(),
                description: format!(
                    "Spend coin and food on a supervised release. {law_label} ends, a narrower open-granary rule replaces it, and the factions gain room to cooperate."
                ),
                supported_faction: authority.id,
            },
        ])
    }

    pub fn resolve_crisis(
        &mut self,
        crisis: EventId,
        choice: CrisisResolutionKind,
    ) -> Result<CrisisResolutionOutcome, WorldError> {
        if self.world.events().values().any(|candidate| {
            candidate.kind == HistoricalEventKind::PlayerIntervention
                && candidate.causes.contains(&crisis)
        }) {
            return Err(WorldError::CrisisAlreadyResolved(crisis));
        }
        let crisis_event = self
            .world
            .events()
            .get(&crisis)
            .ok_or(WorldError::MissingEvent(crisis))?;
        let site_id = crisis_event.location;
        let law = self.world.sites()[&site_id]
            .laws
            .values()
            .filter(|law| law.active)
            .min_by_key(|law| law.id)
            .cloned()
            .ok_or(WorldError::NoActiveCrisisLaw(site_id))?;
        let authority = law.authority;
        let opposition = self
            .world
            .factions()
            .values()
            .filter(|faction| faction.id != authority)
            .min_by_key(|faction| {
                (
                    self.world.factions()[&authority]
                        .relations
                        .get(&faction.id)
                        .copied()
                        .unwrap_or_default(),
                    faction.id,
                )
            })
            .map(|faction| faction.id)
            .expect("generated crisis has multiple factions");
        let witnesses = self.world.sites()[&site_id]
            .population
            .iter()
            .copied()
            .collect::<Vec<_>>();
        let authority_name = self.world.factions()[&authority].name.clone();
        let opposition_name = self.world.factions()[&opposition].name.clone();
        let neutral = self
            .world
            .factions()
            .keys()
            .copied()
            .find(|faction| *faction != authority && *faction != opposition)
            .unwrap_or(authority);
        let mut participants = vec![
            EntityRef::Site(site_id),
            EntityRef::Faction(authority),
            EntityRef::Faction(opposition),
            EntityRef::Law(law.id),
        ];
        let (summary, principle, reaction_faction, aftermath_prompt, mut consequences) =
            match choice {
                CrisisResolutionKind::EnforceEmergencyLaw => (
                    format!(
                        "The outsider upheld {} under {}, preserving the reserve while deepening the dispute with {}",
                        law_label(law.kind),
                        authority_name,
                        opposition_name
                    ),
                    Principle::Duty,
                    opposition,
                    format!("Hear how {opposition_name} responds to enforcement"),
                    vec![
                        Consequence::ChangeResource {
                            site: site_id,
                            resource: ResourceKind::Coin,
                            amount: -5,
                        },
                        Consequence::ChangeFactionRelation {
                            first: authority,
                            second: opposition,
                            amount: -10,
                        },
                    ],
                ),
                CrisisResolutionKind::OpenPublicStores => (
                    format!(
                        "The outsider backed {} and opened the public stores, ending {} at an immediate cost to the food reserve",
                        opposition_name,
                        law_label(law.kind)
                    ),
                    Principle::Compassion,
                    authority,
                    format!("Hear how {authority_name} responds to the opened stores"),
                    vec![
                        Consequence::SetLawActive {
                            site: site_id,
                            law: law.id,
                            active: false,
                        },
                        Consequence::ChangeResource {
                            site: site_id,
                            resource: ResourceKind::Food,
                            amount: -18,
                        },
                        Consequence::ChangeFactionRelation {
                            first: authority,
                            second: opposition,
                            amount: -6,
                        },
                    ],
                ),
                CrisisResolutionKind::BrokerCompromise => {
                    let replacement_id = self.world.allocate_law_id();
                    let replacement = Law {
                        id: replacement_id,
                        kind: LawKind::OpenGranaries,
                        enacted: self.world.date,
                        authority,
                        justification: Principle::Responsibility,
                        active: true,
                    };
                    participants.push(EntityRef::Law(replacement_id));
                    (
                        format!(
                            "The outsider brokered supervised distribution between {} and {}, replacing {} with a narrower open-granary rule",
                            authority_name,
                            opposition_name,
                            law_label(law.kind)
                        ),
                        Principle::Responsibility,
                        neutral,
                        format!(
                            "Ask {} whether the compromise is holding",
                            self.world.factions()[&neutral].name
                        ),
                        vec![
                            Consequence::SetLawActive {
                                site: site_id,
                                law: law.id,
                                active: false,
                            },
                            Consequence::EnactLaw {
                                site: site_id,
                                law: replacement,
                            },
                            Consequence::ChangeResource {
                                site: site_id,
                                resource: ResourceKind::Food,
                                amount: -8,
                            },
                            Consequence::ChangeResource {
                                site: site_id,
                                resource: ResourceKind::Coin,
                                amount: -12,
                            },
                            Consequence::ChangeFactionRelation {
                                first: authority,
                                second: opposition,
                                amount: 8,
                            },
                        ],
                    )
                }
            };
        consequences.push(Consequence::CreatePhysicalEvidence {
            site: site_id,
            kind: PhysicalEvidenceKind::Memorial,
            associated_person: None,
            description: format!("A public notice records that {summary}."),
        });
        let event = self.world.record_event(EventDraft {
            location: site_id,
            kind: HistoricalEventKind::PlayerIntervention,
            participants,
            causes: vec![crisis],
            consequences,
            witnesses,
            principle: Some(principle),
            publicity: EventPublicity::Public,
            summary: summary.clone(),
        })?;
        let site = &self.world.sites()[&site_id];
        Ok(CrisisResolutionOutcome {
            kind: choice,
            event,
            summary,
            reaction_faction,
            aftermath_prompt,
            food_after: site
                .resources
                .get(&ResourceKind::Food)
                .copied()
                .unwrap_or_default(),
            coin_after: site
                .resources
                .get(&ResourceKind::Coin)
                .copied()
                .unwrap_or_default(),
            active_laws: site.laws.values().filter(|law| law.active).count(),
        })
    }

    pub fn record_dungeon_cleared(
        &mut self,
        related_event: EventId,
        dungeon_name: &str,
        relic_name: &str,
    ) -> Result<EventId, WorldError> {
        let world_item = self
            .world
            .significant_items()
            .values()
            .find(|item| {
                item.name == relic_name
                    || item
                        .provenance
                        .iter()
                        .any(|entry| entry.event == related_event)
            })
            .map(|item| item.id)
            .ok_or(WorldError::MissingStrategicItem(related_event))?;
        self.record_dungeon_cleared_with_item(related_event, dungeon_name, world_item)
    }

    /// Records the public aftermath of material care reaching a generated
    /// patient. The caller supplies the observed route; this method does not own
    /// a quest flag and is idempotent for the same patient and causal crisis.
    pub fn record_aid_delivered(
        &mut self,
        cause: EventId,
        patient: PersonId,
        custodian: PersonId,
        advocate: PersonId,
        restricting_law: Option<LawId>,
        kind: AidResolutionKind,
    ) -> Result<EventId, WorldError> {
        let location = self
            .world
            .events()
            .get(&cause)
            .ok_or(WorldError::MissingEvent(cause))?
            .location;
        for person in [patient, custodian, advocate] {
            if !self.world.people().contains_key(&person) {
                return Err(WorldError::MissingPerson(person));
            }
        }
        if let Some(law) = restricting_law
            && !self.world.sites()[&location].laws.contains_key(&law)
        {
            return Err(WorldError::MissingLaw(law));
        }
        if let Some(existing) = self.world.events().values().find(|event| {
            event.kind == HistoricalEventKind::CareDelivered
                && event.causes.contains(&cause)
                && event.participants.contains(&EntityRef::Person(patient))
        }) {
            return Ok(existing.id);
        }

        let patient_name = self.full_name(patient);
        let custodian_name = self.full_name(custodian);
        let advocate_name = self.full_name(advocate);
        let (principle, summary) = match kind {
            AidResolutionKind::ReleasedByConsent => (
                Principle::Compassion,
                format!(
                    "{custodian_name} released medicine after {advocate_name} supported the outsider's appeal; {patient_name} received care"
                ),
            ),
            AidResolutionKind::Purchased => (
                Principle::Responsibility,
                format!(
                    "The outsider purchased medicine from {custodian_name} and delivered it to {patient_name}"
                ),
            ),
            AidResolutionKind::TakenWithoutConsent => (
                Principle::Freedom,
                format!(
                    "The outsider took medicine held by {custodian_name} without consent and delivered it to {patient_name}"
                ),
            ),
            AidResolutionKind::AlternativeTreatment => (
                Principle::Stewardship,
                format!(
                    "The outsider supplied an alternative treatment to {patient_name}, leaving {custodian_name}'s restricted medicine in place"
                ),
            ),
        };
        let mut participants = vec![
            EntityRef::Person(patient),
            EntityRef::Person(custodian),
            EntityRef::Person(advocate),
            EntityRef::Site(location),
        ];
        if let Some(law) = restricting_law {
            participants.push(EntityRef::Law(law));
        }
        self.world.record_event(EventDraft {
            location,
            kind: HistoricalEventKind::CareDelivered,
            participants,
            causes: vec![cause],
            consequences: Vec::new(),
            witnesses: vec![custodian, advocate],
            principle: Some(principle),
            publicity: EventPublicity::Local,
            summary,
        })
    }

    pub fn record_item_recovered_by_player(
        &mut self,
        world_item: WorldItemId,
        location_name: &str,
    ) -> Result<EventId, WorldError> {
        let item = self
            .world
            .significant_items()
            .get(&world_item)
            .ok_or(WorldError::MissingWorldItem(world_item))?;
        if item.custodian == ItemCustodian::Player {
            return Ok(item.provenance.last().expect("item provenance").event);
        }
        let site = item.location;
        let item_name = item.name.clone();
        let cause = item.provenance.last().expect("item provenance").event;
        let witnesses = self.world.sites()[&site]
            .population
            .iter()
            .take(2)
            .copied()
            .collect::<Vec<_>>();
        let event = self.world.record_event(EventDraft {
            location: site,
            kind: HistoricalEventKind::ArtifactRecovered,
            participants: vec![EntityRef::Site(site)],
            causes: vec![cause],
            consequences: Vec::new(),
            witnesses,
            principle: Some(Principle::Truth),
            publicity: EventPublicity::Private,
            summary: format!(
                "The outsider recovered {item_name} from {location_name} and became its custodian"
            ),
        })?;
        let item = self
            .world
            .items
            .get_mut(&world_item)
            .ok_or(WorldError::MissingWorldItem(world_item))?;
        item.custodian = ItemCustodian::Player;
        item.provenance.push(ItemProvenance {
            date: self.world.date,
            event,
            custodian: ItemCustodian::Player,
            description: format!(
                "{item_name} passed into the outsider's custody after its recovery from {location_name}"
            ),
        });
        Ok(event)
    }

    pub fn record_formula_reconstructed_by_player(
        &mut self,
        world_item: WorldItemId,
    ) -> Result<EventId, WorldError> {
        let item = self
            .world
            .significant_items()
            .get(&world_item)
            .ok_or(WorldError::MissingWorldItem(world_item))?;
        let formula_id = item
            .inscribed_formula
            .ok_or(WorldError::WorldItemHasNoFormula(world_item))?;
        if item.custodian != ItemCustodian::Player {
            return Err(WorldError::WorldItemNotHeldByPlayer(world_item));
        }
        if let Some(last) = item.provenance.last()
            && self.world.events()[&last.event].kind == HistoricalEventKind::FormulaReconstructed
        {
            return Ok(last.event);
        }
        let site = item.location;
        let item_name = item.name.clone();
        let formula_name = self
            .world
            .rules()
            .formula(formula_id)
            .expect("significant item formulas are validated")
            .name
            .clone();
        let cause = item.provenance.last().expect("item provenance").event;
        let event = self.world.record_event(EventDraft {
            location: site,
            kind: HistoricalEventKind::FormulaReconstructed,
            participants: vec![EntityRef::Site(site)],
            causes: vec![cause],
            consequences: Vec::new(),
            witnesses: Vec::new(),
            principle: Some(Principle::Truth),
            publicity: EventPublicity::Private,
            summary: format!(
                "The outsider reconstructed {formula_name} from the inscription on {item_name}"
            ),
        })?;
        let item = self
            .world
            .items
            .get_mut(&world_item)
            .ok_or(WorldError::MissingWorldItem(world_item))?;
        item.provenance.push(ItemProvenance {
            date: self.world.date,
            event,
            custodian: ItemCustodian::Player,
            description: format!(
                "While holding {item_name}, the outsider reconstructed {formula_name}"
            ),
        });
        Ok(event)
    }

    pub fn record_dungeon_cleared_with_item(
        &mut self,
        related_event: EventId,
        dungeon_name: &str,
        world_item: WorldItemId,
    ) -> Result<EventId, WorldError> {
        let cause = self
            .world
            .events()
            .get(&related_event)
            .ok_or(WorldError::MissingEvent(related_event))?;
        let site = cause.location;
        let item = self
            .world
            .significant_items()
            .get(&world_item)
            .ok_or(WorldError::MissingWorldItem(world_item))?;
        let relic_name = item.name.clone();
        let strategic_front = item.strategic_front;
        let custody_cause = item.provenance.last().map(|entry| entry.event);
        if let Some(existing) = self.world.events().values().find(|event| {
            event.kind == HistoricalEventKind::DungeonCleared
                && event.causes.contains(&related_event)
        }) {
            return Ok(existing.id);
        }
        let witnesses = self.world.sites()[&site]
            .population
            .iter()
            .copied()
            .collect::<Vec<_>>();
        let summary = format!(
            "The outsider cleared {dungeon_name} and recovered {relic_name}, exposing material records of the earlier crisis"
        );
        let event = self.world.record_event(EventDraft {
            location: site,
            kind: HistoricalEventKind::DungeonCleared,
            participants: vec![EntityRef::Site(site)],
            causes: {
                let mut causes = vec![related_event];
                if let Some(custody_cause) = custody_cause
                    && custody_cause != related_event
                {
                    causes.push(custody_cause);
                }
                causes
            },
            consequences: vec![
                Consequence::ChangeResource {
                    site,
                    resource: ResourceKind::Iron,
                    amount: 6,
                },
                Consequence::CreatePhysicalEvidence {
                    site,
                    kind: PhysicalEvidenceKind::Fortification,
                    associated_person: None,
                    description: format!(
                        "The opened entrance to {dungeon_name}, where {relic_name} was recovered"
                    ),
                },
                Consequence::ShiftStrategicFront {
                    front: strategic_front,
                    amount: 4,
                },
                Consequence::ShiftStrategicFront {
                    front: StrategicFront::Territory,
                    amount: 2,
                },
            ],
            witnesses,
            principle: Some(Principle::Courage),
            publicity: EventPublicity::Public,
            summary,
        })?;
        let item = self
            .world
            .items
            .get_mut(&world_item)
            .ok_or(WorldError::MissingWorldItem(world_item))?;
        item.custodian = ItemCustodian::Site(site);
        item.location = site;
        item.provenance.push(ItemProvenance {
            date: self.world.date,
            event,
            custodian: ItemCustodian::Site(site),
            description: format!(
                "{relic_name} was recovered from {dungeon_name}; its testimony now strengthens the Free Realms"
            ),
        });
        Ok(event)
    }

    pub fn simulate_years(&mut self, years: u32) -> Result<Vec<YearSummary>, WorldError> {
        (0..years).map(|_| self.step_year()).collect()
    }

    pub fn step_year(&mut self) -> Result<YearSummary, WorldError> {
        let event_count_before = self.world.events().len();
        let year = self.world.date.year + 1;
        for month in 1..=12 {
            self.world.date = WorldDate::new(year, month);
            self.resolve_monthly_systems()?;
        }
        let mut harvest_rng = annual_stream(self.world.campaign_seed, year, HARVEST_STREAM);
        let mut demographic_rng = annual_stream(self.world.campaign_seed, year, DEMOGRAPHY_STREAM);
        let mut rumor_rng = annual_stream(self.world.campaign_seed, year, RUMOR_STREAM);

        let harvest_event = self.resolve_harvest(&mut harvest_rng)?;
        let food = self.site_food();
        let population = self.world.living_people().count();
        let shortage_threshold = population as i64 * 3;
        let mut current_crisis = None;

        if food < shortage_threshold {
            let shortage = self.recognize_shortage(harvest_event)?;
            current_crisis = Some(shortage);
            self.respond_to_shortage(shortage)?;
        }

        self.resolve_demography(&mut demographic_rng, current_crisis)?;
        self.spread_rumors(&mut rumor_rng)?;

        Ok(YearSummary {
            year,
            population: self.world.living_people().count(),
            food: self.site_food(),
            events_created: self.world.events().len() - event_count_before,
        })
    }

    fn resolve_harvest(&mut self, rng: &mut RandomStream) -> Result<EventId, WorldError> {
        let farmers: Vec<_> = self
            .world
            .living_people()
            .filter(|person| person.occupation == Occupation::Farmer)
            .map(|person| person.id)
            .collect();
        let population = self.world.living_people().count() as i64;
        let laws: Vec<_> = self.world.sites[&self.primary_site]
            .laws
            .values()
            .filter(|law| law.active)
            .map(|law| law.kind)
            .collect();
        let labor_bonus = if laws.contains(&LawKind::CompulsoryLabor) {
            farmers.len() as i64 * 4
        } else {
            0
        };
        let consumption_per_person = if laws.contains(&LawKind::FoodRationing) {
            3
        } else {
            4
        };
        let production = farmers.len() as i64 * 15 + labor_bonus;
        let variation = bounded(rng, 111) as i64 - 70;
        let consumption = population * consumption_per_person;
        let net_food = production + variation - consumption;
        let leaders = self.faction_leaders();
        let witnesses = unique_people(farmers.iter().copied().take(6).chain(leaders));

        self.world.record_event(EventDraft {
            location: self.primary_site,
            kind: HistoricalEventKind::Harvest,
            participants: farmers
                .iter()
                .copied()
                .map(EntityRef::Person)
                .chain(std::iter::once(EntityRef::Site(self.primary_site)))
                .collect(),
            causes: Vec::new(),
            consequences: vec![Consequence::ChangeResource {
                site: self.primary_site,
                resource: ResourceKind::Food,
                amount: net_food,
            }],
            witnesses,
            principle: Some(Principle::Stewardship),
            publicity: EventPublicity::Local,
            summary: format!(
                "Year {} harvest changed the town food reserve by {net_food}",
                self.world.date.year
            ),
        })
    }

    fn recognize_shortage(&mut self, harvest_event: EventId) -> Result<EventId, WorldError> {
        let food = self.site_food();
        self.world.record_event(EventDraft {
            location: self.primary_site,
            kind: HistoricalEventKind::ShortageRecognized,
            participants: self
                .world
                .factions
                .keys()
                .copied()
                .map(EntityRef::Faction)
                .collect(),
            causes: vec![harvest_event],
            consequences: Vec::new(),
            witnesses: self.faction_leaders(),
            principle: Some(Principle::Responsibility),
            publicity: EventPublicity::Public,
            summary: format!(
                "The town council recognized a food shortage with {food} measures remaining"
            ),
        })
    }

    fn respond_to_shortage(&mut self, shortage: EventId) -> Result<(), WorldError> {
        let authority = self
            .world
            .factions
            .values()
            .max_by_key(|faction| faction.treasury)
            .expect("seeded town has factions")
            .id;
        let authority_principle = self.world.factions[&authority].principle;
        let existing_laws: Vec<_> = self.world.sites[&self.primary_site]
            .laws
            .values()
            .filter(|law| law.active)
            .map(|law| law.kind)
            .collect();
        let food = self.site_food();
        let policy = choose_policy(authority_principle, food, &existing_laws);
        let Some(policy) = policy else {
            return Ok(());
        };

        let law_id = self.world.allocate_law_id();
        let law = Law {
            id: law_id,
            kind: policy,
            enacted: self.world.date,
            authority,
            justification: authority_principle,
            active: true,
        };
        let authority_members: Vec<_> = self.world.factions[&authority]
            .members
            .iter()
            .copied()
            .collect();
        let opposition = self
            .world
            .factions
            .keys()
            .copied()
            .filter(|faction| *faction != authority)
            .min_by_key(|faction| {
                self.world.factions[&authority]
                    .relations
                    .get(faction)
                    .copied()
                    .unwrap_or_default()
            })
            .expect("seeded town has opposition");
        let opposition_members: Vec<_> = self.world.factions[&opposition]
            .members
            .iter()
            .copied()
            .collect();
        let richest_family = self
            .world
            .families
            .values()
            .max_by_key(|family| family.wealth)
            .expect("seeded town has families")
            .id;

        let mut consequences = vec![
            Consequence::EnactLaw {
                site: self.primary_site,
                law,
            },
            Consequence::JudgeLawNecessary {
                law: law_id,
                truth: TruthValue::Unknown,
                believers: authority_members.clone(),
                confidence: 85,
                source: BeliefSource::FactionDoctrine(authority),
                audience: ClaimAudience::Faction(authority),
            },
            Consequence::AssertFactionResponsible {
                event: shortage,
                blamed: opposition,
                believers: authority_members.clone(),
                confidence: 65,
                source: BeliefSource::FactionDoctrine(authority),
                audience: ClaimAudience::Public,
            },
        ];
        match policy {
            LawKind::OpenGranaries => {
                consequences.push(Consequence::ChangeResource {
                    site: self.primary_site,
                    resource: ResourceKind::Food,
                    amount: 35,
                });
            }
            LawKind::PropertySeizure => {
                consequences.extend([
                    Consequence::ChangeResource {
                        site: self.primary_site,
                        resource: ResourceKind::Food,
                        amount: 45,
                    },
                    Consequence::ChangeFamilyWealth {
                        family: richest_family,
                        amount: -30,
                    },
                    Consequence::CreatePhysicalEvidence {
                        site: self.primary_site,
                        kind: PhysicalEvidenceKind::AbandonedFarm,
                        associated_person: None,
                        description: "A farm confiscated during the shortage stands abandoned"
                            .to_string(),
                    },
                ]);
            }
            LawKind::CompulsoryLabor => {
                consequences.push(Consequence::ChangeFactionRelation {
                    first: authority,
                    second: opposition,
                    amount: -12,
                });
            }
            LawKind::FoodRationing | LawKind::PriceControls | LawKind::Curfew => {}
        }

        let law_event = self.world.record_event(EventDraft {
            location: self.primary_site,
            kind: HistoricalEventKind::LawEnacted,
            participants: vec![
                EntityRef::Faction(authority),
                EntityRef::Faction(opposition),
                EntityRef::Law(law_id),
            ],
            causes: vec![shortage],
            consequences,
            witnesses: unique_people(
                authority_members
                    .iter()
                    .copied()
                    .chain(opposition_members.iter().copied()),
            ),
            principle: Some(authority_principle),
            publicity: EventPublicity::Public,
            summary: format!(
                "{} enacted {policy:?}, citing {authority_principle:?}",
                self.world.factions[&authority].name
            ),
        })?;

        if matches!(
            policy,
            LawKind::CompulsoryLabor | LawKind::PropertySeizure | LawKind::FoodRationing
        ) {
            self.world.record_event(EventDraft {
                location: self.primary_site,
                kind: HistoricalEventKind::Protest,
                participants: vec![
                    EntityRef::Faction(opposition),
                    EntityRef::Faction(authority),
                    EntityRef::Law(law_id),
                ],
                causes: vec![law_event],
                consequences: vec![
                    Consequence::ChangeFactionRelation {
                        first: authority,
                        second: opposition,
                        amount: -15,
                    },
                    Consequence::JudgeLawNecessary {
                        law: law_id,
                        truth: TruthValue::Unknown,
                        believers: opposition_members.clone(),
                        confidence: 80,
                        source: BeliefSource::FactionDoctrine(opposition),
                        audience: ClaimAudience::Faction(opposition),
                    },
                ],
                witnesses: opposition_members,
                principle: Some(Principle::Freedom),
                publicity: EventPublicity::Public,
                summary: format!(
                    "{} protested the new {policy:?} law",
                    self.world.factions[&opposition].name
                ),
            })?;
        }

        Ok(())
    }

    fn resolve_demography(
        &mut self,
        rng: &mut RandomStream,
        crisis: Option<EventId>,
    ) -> Result<(), WorldError> {
        let mut death_candidates: Vec<_> = self
            .world
            .living_people()
            .filter(|person| person.age_at(self.world.date) >= 72 || one_in(rng, 110))
            .map(|person| person.id)
            .collect();
        death_candidates.sort();
        death_candidates.truncate(2);
        let mut faction_survivors: BTreeMap<_, _> = self
            .world
            .factions
            .iter()
            .map(|(id, faction)| (*id, faction.members.len()))
            .collect();
        death_candidates.retain(|person| {
            let faction = self.world.people[person].faction;
            let survivors = faction_survivors
                .get_mut(&faction)
                .expect("person belongs to a seeded faction");
            if *survivors <= 1 {
                return false;
            }
            *survivors -= 1;
            true
        });

        for person_id in death_candidates {
            let person = self.world.people[&person_id].clone();
            let family_witnesses: Vec<_> = self.world.families[&person.family]
                .members
                .iter()
                .copied()
                .filter(|member| *member != person_id && self.world.people[member].is_alive())
                .take(5)
                .collect();
            let mut consequences = vec![
                Consequence::PersonDied { person: person_id },
                Consequence::CreatePhysicalEvidence {
                    site: person.home,
                    kind: PhysicalEvidenceKind::Grave,
                    associated_person: Some(person_id),
                    description: format!("The grave of {}", self.full_name(person_id)),
                },
            ];
            if self.world.regional_settlements().contains_key(&person.home) {
                consequences.push(Consequence::ChangeRegionalPopulation {
                    site: person.home,
                    amount: -1,
                });
            }
            let death_event = self.world.record_event(EventDraft {
                location: person.home,
                kind: HistoricalEventKind::Death,
                participants: vec![
                    EntityRef::Person(person_id),
                    EntityRef::Family(person.family),
                ],
                causes: crisis.into_iter().collect(),
                consequences,
                witnesses: family_witnesses,
                principle: None,
                publicity: EventPublicity::Local,
                summary: format!(
                    "{} died at age {}",
                    self.full_name(person_id),
                    person.age_at(self.world.date)
                ),
            })?;

            if self.world.factions[&person.faction].leader == person_id
                && let Some(successor) = self.select_successor(person.faction)
            {
                self.world.record_event(EventDraft {
                    location: person.home,
                    kind: HistoricalEventKind::LeadershipSuccession,
                    participants: vec![
                        EntityRef::Faction(person.faction),
                        EntityRef::Person(person_id),
                        EntityRef::Person(successor),
                    ],
                    causes: vec![death_event],
                    consequences: vec![Consequence::SetFactionLeader {
                        faction: person.faction,
                        leader: successor,
                    }],
                    witnesses: self.world.factions[&person.faction]
                        .members
                        .iter()
                        .copied()
                        .collect(),
                    principle: Some(self.world.factions[&person.faction].principle),
                    publicity: EventPublicity::Public,
                    summary: format!(
                        "{} succeeded {} as leader of {}",
                        self.full_name(successor),
                        self.full_name(person_id),
                        self.world.factions[&person.faction].name
                    ),
                })?;
            }
        }

        let population = self.world.living_people().count();
        let births = if population < 24 {
            2
        } else if population < 42 && one_in(rng, 2) {
            1
        } else {
            0
        };
        for _ in 0..births {
            self.create_birth(rng)?;
        }
        Ok(())
    }

    fn create_birth(&mut self, rng: &mut RandomStream) -> Result<EventId, WorldError> {
        let families: Vec<_> = self.world.families.keys().copied().collect();
        let family = families[bounded(rng, families.len())];
        let potential_parents: Vec<_> = self.world.families[&family]
            .members
            .iter()
            .copied()
            .filter(|person| {
                let person = &self.world.people[person];
                person.is_alive() && (18..=55).contains(&person.age_at(self.world.date))
            })
            .collect();
        if potential_parents.is_empty() {
            return self.create_birth_in_any_family(rng);
        }
        let parent = potential_parents[bounded(rng, potential_parents.len())];
        self.create_child(rng, family, parent)
    }

    fn create_birth_in_any_family(
        &mut self,
        rng: &mut RandomStream,
    ) -> Result<EventId, WorldError> {
        let potential_parents: Vec<_> = self
            .world
            .living_people()
            .filter(|person| (18..=55).contains(&person.age_at(self.world.date)))
            .map(|person| person.id)
            .collect();
        let parent = potential_parents[bounded(rng, potential_parents.len())];
        let family = self.world.people[&parent].family;
        self.create_child(rng, family, parent)
    }

    fn create_child(
        &mut self,
        rng: &mut RandomStream,
        family: FamilyId,
        parent: PersonId,
    ) -> Result<EventId, WorldError> {
        let parent_record = self.world.people[&parent].clone();
        let id = self.world.allocate_person_id();
        let given_name = generated_name(id, rng);
        let person = Person {
            id,
            given_name: given_name.clone(),
            family,
            born: self.world.date,
            died: None,
            parents: vec![parent],
            occupation: Occupation::Laborer,
            faction: parent_record.faction,
            home: parent_record.home,
            drives: generated_drives(rng),
        };
        let home = person.home;
        let mut consequences = vec![Consequence::AddPerson(Box::new(person))];
        if self.world.regional_settlements().contains_key(&home) {
            consequences.push(Consequence::ChangeRegionalPopulation {
                site: home,
                amount: 1,
            });
        }
        self.world.record_event(EventDraft {
            location: home,
            kind: HistoricalEventKind::Birth,
            participants: vec![
                EntityRef::Person(id),
                EntityRef::Person(parent),
                EntityRef::Family(family),
            ],
            causes: Vec::new(),
            consequences,
            witnesses: vec![parent],
            principle: Some(Principle::Compassion),
            publicity: EventPublicity::Private,
            summary: format!(
                "{given_name} {} was born",
                self.world.families[&family].surname
            ),
        })
    }

    fn spread_rumors(&mut self, rng: &mut RandomStream) -> Result<(), WorldError> {
        for _ in 0..3 {
            let sources: Vec<_> = self
                .world
                .knowledge()
                .beliefs
                .iter()
                .filter(|(person, beliefs)| {
                    self.world.people[person].is_alive()
                        && beliefs.values().any(|belief| belief.willing_to_share)
                })
                .map(|(person, _)| *person)
                .collect();
            if sources.is_empty() {
                break;
            }
            let source = sources[bounded(rng, sources.len())];
            let shareable: Vec<_> = self.world.knowledge().beliefs[&source]
                .values()
                .filter(|belief| belief.willing_to_share)
                .copied()
                .collect();
            let belief = shareable[bounded(rng, shareable.len())];
            let recipients: Vec<_> = self
                .world
                .living_people()
                .map(|person| person.id)
                .filter(|person| *person != source)
                .collect();
            let recipient = recipients[bounded(rng, recipients.len())];
            let originating_event = self.world.claims()[&belief.claim].created_by_event;
            let confidence = belief.confidence.saturating_sub(10).max(20);

            self.world.record_event(EventDraft {
                location: self.primary_site,
                kind: HistoricalEventKind::RumorSpread,
                participants: vec![EntityRef::Person(source), EntityRef::Person(recipient)],
                causes: vec![originating_event],
                consequences: vec![Consequence::ShareClaim {
                    claim: belief.claim,
                    source,
                    recipient,
                    confidence,
                }],
                witnesses: vec![source, recipient],
                principle: Some(Principle::Truth),
                publicity: EventPublicity::Private,
                summary: format!(
                    "{} told {} a claim with {confidence}% confidence",
                    self.full_name(source),
                    self.full_name(recipient)
                ),
            })?;
        }
        Ok(())
    }

    fn faction_leaders(&self) -> Vec<PersonId> {
        self.world
            .factions
            .values()
            .map(|faction| faction.leader)
            .filter(|person| self.world.people[person].is_alive())
            .collect()
    }

    fn select_successor(&self, faction: FactionId) -> Option<PersonId> {
        self.world.factions[&faction]
            .members
            .iter()
            .copied()
            .filter(|person| self.world.people[person].is_alive())
            .max_by_key(|person| {
                (
                    self.world.people[person]
                        .drives
                        .get(&Drive::Status)
                        .copied()
                        .unwrap_or_default(),
                    std::cmp::Reverse(person.0),
                )
            })
    }

    fn site_food(&self) -> i64 {
        self.world.sites[&self.primary_site]
            .resources
            .get(&ResourceKind::Food)
            .copied()
            .unwrap_or_default()
    }

    fn full_name(&self, person: PersonId) -> String {
        let person = &self.world.people[&person];
        format!(
            "{} {}",
            person.given_name, self.world.families[&person.family].surname
        )
    }
}

impl HistoricalWorld {
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        problems.extend(
            self.rules
                .validate()
                .into_iter()
                .map(|problem| format!("world rule: {problem}")),
        );

        for person in self.living_people() {
            if !self.families[&person.family].members.contains(&person.id) {
                problems.push(format!("person {} missing from family", person.id));
            }
            if !self.factions[&person.faction].members.contains(&person.id) {
                problems.push(format!("person {} missing from faction", person.id));
            }
            if !self.sites[&person.home].population.contains(&person.id) {
                problems.push(format!("person {} missing from home population", person.id));
            }
        }
        for faction in self.factions.values() {
            if !self.people[&faction.leader].is_alive() {
                problems.push(format!("faction {} has a dead leader", faction.id));
            }
        }
        for project in self.projects.values() {
            if !self.sites.contains_key(&project.site) {
                problems.push(format!("project {} references a missing site", project.id));
            }
            if !self.factions.contains_key(&project.sponsor) {
                problems.push(format!(
                    "project {} references a missing sponsor",
                    project.id
                ));
            }
            if !self.events().contains_key(&project.related_event)
                || !self.events().contains_key(&project.last_event)
            {
                problems.push(format!(
                    "project {} references missing historical events",
                    project.id
                ));
            }
            for worker in &project.workers {
                if !self.people.contains_key(worker) {
                    problems.push(format!(
                        "project {} references missing worker {}",
                        project.id, worker
                    ));
                } else if !self.factions[&project.sponsor].members.contains(worker) {
                    problems.push(format!(
                        "project {} assigns worker {} outside its sponsor faction",
                        project.id, worker
                    ));
                }
            }
        }
        for settlement in self.regional_settlements.values() {
            if !self.sites.contains_key(&settlement.site) {
                problems.push(format!(
                    "regional settlement {} references a missing site",
                    settlement.site
                ));
            }
            if !self.factions.contains_key(&settlement.controller) {
                problems.push(format!(
                    "regional settlement {} references missing controller {}",
                    settlement.site, settlement.controller
                ));
            }
            if !self.events().contains_key(&settlement.last_event) {
                problems.push(format!(
                    "regional settlement {} references missing event {}",
                    settlement.site, settlement.last_event
                ));
            }
            let named_population = self
                .living_people()
                .filter(|person| person.home == settlement.site)
                .count() as u32;
            if settlement.population < named_population {
                problems.push(format!(
                    "regional settlement {} has {} aggregate residents but {} named residents",
                    settlement.site, settlement.population, named_population
                ));
            }
            if settlement
                .monthly_production
                .values()
                .chain(settlement.monthly_consumption.values())
                .any(|amount| *amount < 0)
            {
                problems.push(format!(
                    "regional settlement {} has a negative economic rate",
                    settlement.site
                ));
            }
            if self
                .atlas
                .cell(settlement.position)
                .is_none_or(|cell| !cell.is_passable_land())
            {
                problems.push(format!(
                    "regional settlement {} occupies impassable geography",
                    settlement.site
                ));
            }
        }
        for route in self.routes.values() {
            if route.first == route.second
                || !self.regional_settlements.contains_key(&route.first)
                || !self.regional_settlements.contains_key(&route.second)
            {
                problems.push(format!(
                    "regional route {} has invalid endpoints {} and {}",
                    route.id, route.first, route.second
                ));
            }
            let expected_first = self
                .regional_settlements
                .get(&route.first)
                .map(|settlement| settlement.position);
            let expected_second = self
                .regional_settlements
                .get(&route.second)
                .map(|settlement| settlement.position);
            if route.path.first().copied() != expected_first
                || route.path.last().copied() != expected_second
            {
                problems.push(format!(
                    "regional route {} does not join its settlements",
                    route.id
                ));
            }
            if route
                .path
                .windows(2)
                .any(|step| step[0].distance(step[1]) != 1)
            {
                problems.push(format!(
                    "regional route {} has a discontinuous path",
                    route.id
                ));
            }
            if route.path.iter().any(|position| {
                self.atlas
                    .cell(*position)
                    .is_none_or(|cell| matches!(cell.water, WaterBody::Ocean | WaterBody::Lake))
            }) {
                problems.push(format!(
                    "regional route {} crosses impassable water",
                    route.id
                ));
            }
            if route.condition > 100 || route.danger > 100 {
                problems.push(format!(
                    "regional route {} has invalid condition or danger",
                    route.id
                ));
            }
            if !self.events().contains_key(&route.last_event) {
                problems.push(format!(
                    "regional route {} references missing event {}",
                    route.id, route.last_event
                ));
            }
        }
        for goal in self.regional_goals.values() {
            if !self.factions.contains_key(&goal.sponsor) {
                problems.push(format!(
                    "regional goal {} references missing sponsor {}",
                    goal.id, goal.sponsor
                ));
            }
            if !self.events().contains_key(&goal.cause) {
                problems.push(format!(
                    "regional goal {} references missing cause {}",
                    goal.id, goal.cause
                ));
            }
            match goal.kind {
                RegionalGoalKind::SecureRoute(route) if !self.routes.contains_key(&route) => {
                    problems.push(format!(
                        "regional goal {} references missing route {}",
                        goal.id, route
                    ));
                }
                RegionalGoalKind::RelieveShortage(site)
                    if !self.regional_settlements.contains_key(&site) =>
                {
                    problems.push(format!(
                        "regional goal {} references missing settlement {}",
                        goal.id, site
                    ));
                }
                _ => {}
            }
            if goal.status == RegionalGoalStatus::Resolved
                && goal
                    .resolved_by
                    .is_none_or(|event| !self.events().contains_key(&event))
            {
                problems.push(format!(
                    "resolved regional goal {} has no valid resolution event",
                    goal.id
                ));
            }
        }
        for party in self.regional_parties.values() {
            if !self.routes.contains_key(&party.route)
                || !self.regional_settlements.contains_key(&party.origin)
                || !self.regional_settlements.contains_key(&party.destination)
                || self.routes.get(&party.route).is_some_and(|route| {
                    !route.connects(party.origin)
                        || route.other_end(party.origin) != Some(party.destination)
                })
            {
                problems.push(format!(
                    "regional party {} has an invalid route or endpoints",
                    party.id
                ));
            }
            if party.progress > 1_000 {
                problems.push(format!(
                    "regional party {} has invalid route progress {}",
                    party.id, party.progress
                ));
            }
            if party
                .faction
                .is_some_and(|faction| !self.factions.contains_key(&faction))
            {
                problems.push(format!(
                    "regional party {} references a missing faction",
                    party.id
                ));
            }
            if party
                .leader
                .is_some_and(|leader| !self.people.contains_key(&leader))
            {
                problems.push(format!(
                    "regional party {} references a missing leader",
                    party.id
                ));
            }
            if !self.events().contains_key(&party.cause)
                || !self.events().contains_key(&party.last_event)
            {
                problems.push(format!(
                    "regional party {} references missing history",
                    party.id
                ));
            }
        }
        let mut object_ids = BTreeSet::new();
        for item in self.items.values() {
            if !object_ids.insert(item.object) {
                problems.push(format!("item {} has duplicate object identity", item.id));
            }
            if !self.sites.contains_key(&item.location) {
                problems.push(format!("item {} references a missing location", item.id));
            }
            if item.materials.is_empty() {
                problems.push(format!("item {} has no constituent materials", item.id));
            }
            if item
                .inscribed_formula
                .is_some_and(|formula| self.rules.formula(formula).is_none())
            {
                problems.push(format!("item {} references a missing formula", item.id));
            }
            match item.custodian {
                ItemCustodian::Person(person) if !self.people.contains_key(&person) => {
                    problems.push(format!("item {} has missing custodian {person}", item.id));
                }
                ItemCustodian::Faction(faction) if !self.factions.contains_key(&faction) => {
                    problems.push(format!("item {} has missing custodian {faction}", item.id));
                }
                ItemCustodian::Site(site) if !self.sites.contains_key(&site) => {
                    problems.push(format!("item {} has missing custodian {site}", item.id));
                }
                _ => {}
            }
            if item.provenance.is_empty() {
                problems.push(format!("item {} has no provenance", item.id));
            }
            for provenance in &item.provenance {
                if !self.events().contains_key(&provenance.event) {
                    problems.push(format!(
                        "item {} provenance references missing event {}",
                        item.id, provenance.event
                    ));
                }
                match provenance.custodian {
                    ItemCustodian::Person(person) if !self.people.contains_key(&person) => {
                        problems.push(format!(
                            "item {} provenance has missing custodian {person}",
                            item.id
                        ));
                    }
                    ItemCustodian::Faction(faction) if !self.factions.contains_key(&faction) => {
                        problems.push(format!(
                            "item {} provenance has missing custodian {faction}",
                            item.id
                        ));
                    }
                    ItemCustodian::Site(site) if !self.sites.contains_key(&site) => {
                        problems.push(format!(
                            "item {} provenance has missing custodian {site}",
                            item.id
                        ));
                    }
                    _ => {}
                }
            }
        }
        if self
            .struggle
            .last_event
            .is_some_and(|event| !self.events().contains_key(&event))
        {
            problems.push("grand struggle references a missing event".to_string());
        }
        for (front, balance) in &self.struggle.fronts {
            if !(-100..=100).contains(balance) {
                problems.push(format!(
                    "strategic front {front:?} has invalid balance {balance}"
                ));
            }
        }
        for actor in self.struggle.actors.values() {
            if actor.capacity > 100
                || actor.reserves < 0
                || !(-100..=100).contains(&actor.influence)
            {
                problems.push(format!(
                    "strategic actor {} has invalid capacity, reserves, or influence",
                    actor.name
                ));
            }
            if actor
                .last_event
                .is_some_and(|event| !self.events().contains_key(&event))
            {
                problems.push(format!(
                    "strategic actor {} references a missing event",
                    actor.name
                ));
            }
            if let Some(objective) = &actor.objective {
                if !self.events().contains_key(&objective.cause) {
                    problems.push(format!(
                        "strategic actor {} has an objective with a missing cause",
                        actor.name
                    ));
                }
                match objective.kind {
                    StrategicObjectiveKind::RestoreRoute(route)
                    | StrategicObjectiveKind::DisruptRoute(route)
                        if !self.routes.contains_key(&route) =>
                    {
                        problems.push(format!(
                            "strategic actor {} targets a missing route",
                            actor.name
                        ));
                    }
                    StrategicObjectiveKind::RelieveSettlement(site)
                    | StrategicObjectiveKind::ExploitSettlement(site)
                    | StrategicObjectiveKind::ConsolidateInfluence(site)
                        if !self.regional_settlements.contains_key(&site) =>
                    {
                        problems.push(format!(
                            "strategic actor {} targets a missing settlement",
                            actor.name
                        ));
                    }
                    _ => {}
                }
            }
        }
        for event in self.events().values() {
            for cause in &event.causes {
                if cause >= &event.id {
                    problems.push(format!(
                        "event {} has non-ancestral cause {}",
                        event.id, cause
                    ));
                }
            }
        }
        for evidence in self
            .sites
            .values()
            .flat_map(|site| site.physical_evidence.iter())
        {
            if !self.events().contains_key(&evidence.originating_event) {
                problems.push(format!(
                    "physical evidence references missing event {}",
                    evidence.originating_event
                ));
            }
        }
        for beliefs in self.knowledge().beliefs.values() {
            for belief in beliefs.values() {
                if !self.claims().contains_key(&belief.claim) {
                    problems.push(format!("belief references missing claim {}", belief.claim));
                }
            }
        }
        for claim in self.claims().values() {
            match claim.proposition {
                Proposition::ObjectSurvived(object)
                    if !self.items.values().any(|item| item.object == object) =>
                {
                    problems.push(format!(
                        "claim {} references missing object {}",
                        claim.id, object.0
                    ));
                }
                Proposition::FormulaProduces { formula, effect }
                    if self.rules.formula(formula).is_none()
                        || (claim.truth == TruthValue::True
                            && self
                                .rules
                                .formula(formula)
                                .is_some_and(|rule| rule.effect != effect)) =>
                {
                    problems.push(format!(
                        "claim {} contradicts or references missing formula {}",
                        claim.id, formula.0
                    ));
                }
                Proposition::FormulaRequires { formula, reagent }
                    if self.rules.formula(formula).is_none()
                        || (claim.truth == TruthValue::True
                            && self
                                .rules
                                .formula(formula)
                                .is_some_and(|rule| !rule.reagents.contains(&reagent))) =>
                {
                    problems.push(format!(
                        "claim {} contradicts or references missing formula {}",
                        claim.id, formula.0
                    ));
                }
                _ => {}
            }
        }
        problems
    }

    pub fn causal_ancestors(&self, event: EventId) -> Vec<EventId> {
        fn visit(
            world: &HistoricalWorld,
            event: EventId,
            seen: &mut BTreeSet<EventId>,
            ordered: &mut Vec<EventId>,
        ) {
            let Some(record) = world.events().get(&event) else {
                return;
            };
            for cause in &record.causes {
                if seen.insert(*cause) {
                    visit(world, *cause, seen, ordered);
                    ordered.push(*cause);
                }
            }
        }

        let mut seen = BTreeSet::new();
        let mut ordered = Vec::new();
        visit(self, event, &mut seen, &mut ordered);
        ordered
    }

    pub fn describe_claim(&self, proposition: &Proposition) -> String {
        match proposition {
            Proposition::EventOccurred(event) => {
                format!("event {event} occurred")
            }
            Proposition::FactionResponsibleFor { event, faction } => format!(
                "{} was responsible for event {event}",
                self.factions[faction].name
            ),
            Proposition::LawWasNecessary(law) => format!("law {law} was necessary"),
            Proposition::PersonDied(person) => format!("person {person} died"),
            Proposition::ObjectSurvived(object) => {
                format!("object {} survived", object.0)
            }
            Proposition::FormulaProduces { formula, effect } => {
                format!("formula {} produces {}", formula.0, effect.name())
            }
            Proposition::FormulaRequires { formula, reagent } => {
                format!("formula {} requires {}", formula.0, reagent.name())
            }
        }
    }
}

fn project_requirements(kind: SettlementProjectKind) -> (BTreeMap<ResourceKind, i64>, i64, u8) {
    match kind {
        SettlementProjectKind::PublicGranary => (
            BTreeMap::from([(ResourceKind::Timber, 34), (ResourceKind::Iron, 6)]),
            18,
            4,
        ),
        SettlementProjectKind::WatchHouse => (
            BTreeMap::from([(ResourceKind::Timber, 34), (ResourceKind::Iron, 12)]),
            22,
            5,
        ),
        SettlementProjectKind::MarketHall => (
            BTreeMap::from([(ResourceKind::Timber, 38), (ResourceKind::Iron, 5)]),
            24,
            5,
        ),
        SettlementProjectKind::ReliefHousing => (
            BTreeMap::from([(ResourceKind::Timber, 40), (ResourceKind::Medicine, 4)]),
            16,
            5,
        ),
        SettlementProjectKind::CivicWorkshop => (
            BTreeMap::from([(ResourceKind::Timber, 32), (ResourceKind::Iron, 16)]),
            20,
            6,
        ),
    }
}

fn project_kind_for_law(law: LawKind) -> SettlementProjectKind {
    match law {
        LawKind::FoodRationing | LawKind::OpenGranaries => SettlementProjectKind::PublicGranary,
        LawKind::Curfew => SettlementProjectKind::WatchHouse,
        LawKind::PriceControls => SettlementProjectKind::MarketHall,
        LawKind::PropertySeizure => SettlementProjectKind::ReliefHousing,
        LawKind::CompulsoryLabor => SettlementProjectKind::CivicWorkshop,
    }
}

fn strategic_item_for_law(law: LawKind) -> (&'static str, SignificantItemKind, StrategicFront) {
    match law {
        LawKind::PropertySeizure => (
            "Ledger of Seized Homes",
            SignificantItemKind::Ledger,
            StrategicFront::Political,
        ),
        LawKind::CompulsoryLabor => (
            "Broken Levy Seal",
            SignificantItemKind::Seal,
            StrategicFront::Military,
        ),
        LawKind::PriceControls => (
            "Contraband Price Ledger",
            SignificantItemKind::Ledger,
            StrategicFront::Economy,
        ),
        LawKind::OpenGranaries => (
            "Founders' Grain Seal",
            SignificantItemKind::Seal,
            StrategicFront::Spiritual,
        ),
        LawKind::Curfew => (
            "Curfew Arrest Roll",
            SignificantItemKind::Ledger,
            StrategicFront::Political,
        ),
        LawKind::FoodRationing => (
            "Original Ration Ledger",
            SignificantItemKind::Ledger,
            StrategicFront::Economy,
        ),
    }
}

fn project_kind_for_principle(principle: Principle) -> SettlementProjectKind {
    match principle {
        Principle::Duty | Principle::Responsibility | Principle::Justice => {
            SettlementProjectKind::WatchHouse
        }
        Principle::Stewardship | Principle::Compassion | Principle::Courage => {
            SettlementProjectKind::PublicGranary
        }
        Principle::Freedom | Principle::Truth => SettlementProjectKind::MarketHall,
    }
}

fn unique_project_kind(
    preferred: SettlementProjectKind,
    used: &BTreeSet<SettlementProjectKind>,
) -> SettlementProjectKind {
    std::iter::once(preferred)
        .chain([
            SettlementProjectKind::PublicGranary,
            SettlementProjectKind::WatchHouse,
            SettlementProjectKind::MarketHall,
            SettlementProjectKind::ReliefHousing,
            SettlementProjectKind::CivicWorkshop,
        ])
        .find(|kind| !used.contains(kind))
        .expect("five project kinds cover three seeded factions")
}

fn project_benefit(kind: SettlementProjectKind) -> (ResourceKind, i64) {
    match kind {
        SettlementProjectKind::PublicGranary => (ResourceKind::Food, 30),
        SettlementProjectKind::WatchHouse => (ResourceKind::Coin, 12),
        SettlementProjectKind::MarketHall => (ResourceKind::Coin, 28),
        SettlementProjectKind::ReliefHousing => (ResourceKind::Medicine, 8),
        SettlementProjectKind::CivicWorkshop => (ResourceKind::Iron, 12),
    }
}

fn project_strategic_front(kind: SettlementProjectKind) -> StrategicFront {
    match kind {
        SettlementProjectKind::PublicGranary
        | SettlementProjectKind::MarketHall
        | SettlementProjectKind::CivicWorkshop => StrategicFront::Economy,
        SettlementProjectKind::WatchHouse => StrategicFront::Military,
        SettlementProjectKind::ReliefHousing => StrategicFront::Spiritual,
    }
}

fn strategic_objective_front(kind: StrategicObjectiveKind) -> StrategicFront {
    match kind {
        StrategicObjectiveKind::RestoreRoute(_) | StrategicObjectiveKind::DisruptRoute(_) => {
            StrategicFront::Military
        }
        StrategicObjectiveKind::RelieveSettlement(_)
        | StrategicObjectiveKind::ExploitSettlement(_) => StrategicFront::Economy,
        StrategicObjectiveKind::ConsolidateInfluence(_) => StrategicFront::Political,
    }
}

fn strategic_objective_cause(
    world: &HistoricalWorld,
    kind: StrategicObjectiveKind,
    fallback: EventId,
) -> EventId {
    match kind {
        StrategicObjectiveKind::RestoreRoute(route)
        | StrategicObjectiveKind::DisruptRoute(route) => world
            .routes()
            .get(&route)
            .map(|route| route.last_event)
            .unwrap_or(fallback),
        StrategicObjectiveKind::RelieveSettlement(site)
        | StrategicObjectiveKind::ExploitSettlement(site)
        | StrategicObjectiveKind::ConsolidateInfluence(site) => world
            .regional_settlements()
            .get(&site)
            .map(|settlement| settlement.last_event)
            .unwrap_or(fallback),
    }
}

fn strategic_objective_consequences(
    kind: StrategicObjectiveKind,
    defending: bool,
) -> Vec<Consequence> {
    match kind {
        StrategicObjectiveKind::RestoreRoute(route) => vec![Consequence::SetRouteDisrupted {
            route,
            disrupted: false,
        }],
        StrategicObjectiveKind::DisruptRoute(route) => vec![Consequence::SetRouteDisrupted {
            route,
            disrupted: true,
        }],
        StrategicObjectiveKind::RelieveSettlement(site) => vec![Consequence::ChangeResource {
            site,
            resource: ResourceKind::Food,
            amount: 24,
        }],
        StrategicObjectiveKind::ExploitSettlement(site) => vec![Consequence::ChangeResource {
            site,
            resource: ResourceKind::Food,
            amount: -12,
        }],
        StrategicObjectiveKind::ConsolidateInfluence(site) => {
            vec![Consequence::ChangeResource {
                site,
                resource: ResourceKind::Coin,
                amount: if defending { 8 } else { -8 },
            }]
        }
    }
}

fn strategic_objective_description(
    world: &HistoricalWorld,
    kind: StrategicObjectiveKind,
) -> String {
    match kind {
        StrategicObjectiveKind::RestoreRoute(route) => {
            format!("restoring {}", world.routes()[&route].name)
        }
        StrategicObjectiveKind::DisruptRoute(route) => {
            format!(
                "isolating settlements along {}",
                world.routes()[&route].name
            )
        }
        StrategicObjectiveKind::RelieveSettlement(site) => {
            format!("relieving {}", world.sites()[&site].name)
        }
        StrategicObjectiveKind::ExploitSettlement(site) => {
            format!("deepening scarcity in {}", world.sites()[&site].name)
        }
        StrategicObjectiveKind::ConsolidateInfluence(site) => {
            format!("consolidating influence in {}", world.sites()[&site].name)
        }
    }
}

fn regional_economy_profile(
    role: SettlementRole,
    population: u32,
) -> (
    BTreeMap<ResourceKind, i64>,
    BTreeMap<ResourceKind, i64>,
    BTreeMap<ResourceKind, i64>,
) {
    let food_need = (i64::from(population) / 4).max(18);
    let (food_months, food_output, timber, iron, medicine, coin) = match role {
        SettlementRole::Capital => (4, food_need + 3, 10, 4, 3, 18),
        SettlementRole::Agrarian => (6, food_need + 18, 8, 1, 2, 9),
        SettlementRole::Forest => (3, food_need - 5, 42, 2, 3, 10),
        SettlementRole::Mining => (1, food_need - 14, 6, 28, 1, 13),
        SettlementRole::Crossroads => (3, food_need - 2, 9, 4, 2, 32),
        SettlementRole::River => (5, food_need + 8, 13, 2, 8, 16),
        SettlementRole::Monastic => (3, food_need + 1, 5, 1, 16, 8),
        SettlementRole::Fortress => (1, food_need - 16, 8, 15, 2, 9),
    };
    let production = BTreeMap::from([
        (ResourceKind::Food, food_output.max(0)),
        (ResourceKind::Timber, timber),
        (ResourceKind::Iron, iron),
        (ResourceKind::Medicine, medicine),
        (ResourceKind::Coin, coin),
    ]);
    let consumption = BTreeMap::from([
        (ResourceKind::Food, food_need),
        (ResourceKind::Timber, 4),
        (ResourceKind::Iron, 2),
        (ResourceKind::Medicine, 2),
        (ResourceKind::Coin, 7),
    ]);
    let resources = BTreeMap::from([
        (ResourceKind::Food, food_need * food_months),
        (ResourceKind::Timber, timber * 3 + 20),
        (ResourceKind::Iron, iron * 3 + 8),
        (ResourceKind::Medicine, medicine * 3 + 5),
        (ResourceKind::Coin, coin * 3 + 30),
    ]);
    (resources, production, consumption)
}

fn site_preference(role: SettlementRole) -> SitePreference {
    match role {
        SettlementRole::Capital => SitePreference::Capital,
        SettlementRole::Agrarian => SitePreference::Agrarian,
        SettlementRole::Forest => SitePreference::Forest,
        SettlementRole::Mining => SitePreference::Mining,
        SettlementRole::Crossroads => SitePreference::Crossroads,
        SettlementRole::River => SitePreference::River,
        SettlementRole::Monastic => SitePreference::Monastic,
        SettlementRole::Fortress => SitePreference::Fortress,
    }
}

fn repair_cost(full_cost: i64) -> i64 {
    (full_cost / 2).max(1)
}

fn insert_initial_person(world: &mut HistoricalWorld, person: Person) {
    world
        .families
        .get_mut(&person.family)
        .expect("seeded family")
        .members
        .insert(person.id);
    world
        .factions
        .get_mut(&person.faction)
        .expect("seeded faction")
        .members
        .insert(person.id);
    world
        .sites
        .get_mut(&person.home)
        .expect("seeded site")
        .population
        .insert(person.id);
    world.people.insert(person.id, person);
}

fn annual_stream(campaign_seed: u64, year: i32, stream: StreamId) -> RandomStream {
    let year_bits = year as i64 as u64;
    RandomStream::new(
        campaign_seed ^ year_bits.wrapping_mul(0x9e37_79b9_7f4a_7c15),
        stream,
    )
}

fn bounded(rng: &mut RandomStream, upper_exclusive: usize) -> usize {
    assert!(upper_exclusive > 0);
    (rng.next_u64() % upper_exclusive as u64) as usize
}

fn founding_faction_principles(campaign_seed: u64) -> [Principle; 3] {
    const PRINCIPLES: [Principle; 8] = [
        Principle::Compassion,
        Principle::Truth,
        Principle::Duty,
        Principle::Freedom,
        Principle::Justice,
        Principle::Responsibility,
        Principle::Courage,
        Principle::Stewardship,
    ];
    const COPRIME_STEPS: [usize; 4] = [1, 3, 5, 7];

    let mut rng = RandomStream::new(campaign_seed, FACTION_DOCTRINE_STREAM);
    let offset = bounded(&mut rng, PRINCIPLES.len());
    let step = COPRIME_STEPS[bounded(&mut rng, COPRIME_STEPS.len())];
    [
        PRINCIPLES[offset],
        PRINCIPLES[(offset + step) % PRINCIPLES.len()],
        PRINCIPLES[(offset + step * 2) % PRINCIPLES.len()],
    ]
}

fn one_in(rng: &mut RandomStream, denominator: u64) -> bool {
    rng.next_u64().is_multiple_of(denominator)
}

fn generated_drives(rng: &mut RandomStream) -> BTreeMap<Drive, u8> {
    [
        Drive::Survival,
        Drive::Wealth,
        Drive::Status,
        Drive::Family,
        Drive::Loyalty,
        Drive::Freedom,
        Drive::Justice,
        Drive::Faith,
    ]
    .into_iter()
    .map(|drive| (drive, 25 + bounded(rng, 76) as u8))
    .collect()
}

fn generated_name(_id: PersonId, rng: &mut RandomStream) -> String {
    let first = NAME_STARTS[bounded(rng, NAME_STARTS.len())];
    let ending = NAME_ENDS[bounded(rng, NAME_ENDS.len())];
    format!("{first}{ending}")
}

fn unique_people(people: impl IntoIterator<Item = PersonId>) -> Vec<PersonId> {
    people
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn choose_policy(principle: Principle, food: i64, existing_laws: &[LawKind]) -> Option<LawKind> {
    let preferred = if food < 0 {
        LawKind::PropertySeizure
    } else {
        match principle {
            Principle::Compassion => LawKind::OpenGranaries,
            Principle::Duty | Principle::Responsibility => LawKind::FoodRationing,
            Principle::Justice | Principle::Truth => LawKind::PriceControls,
            Principle::Stewardship => LawKind::CompulsoryLabor,
            Principle::Freedom | Principle::Courage => LawKind::OpenGranaries,
        }
    };
    (!existing_laws.contains(&preferred)).then_some(preferred)
}

fn law_label(kind: LawKind) -> &'static str {
    match kind {
        LawKind::FoodRationing => "FOOD RATIONING",
        LawKind::PriceControls => "PRICE CONTROLS",
        LawKind::CompulsoryLabor => "COMPULSORY LABOR",
        LawKind::PropertySeizure => "PROPERTY SEIZURE",
        LawKind::Curfew => "THE CURFEW",
        LawKind::OpenGranaries => "OPEN GRANARIES",
    }
}

const TOWN_NAMES: &[&str] = &[
    "Carin",
    "Rathmere",
    "Dunwall",
    "Eldenford",
    "Valewick",
    "Marren",
];
const REGIONAL_SITE_NAMES: &[&str] = &[
    "Ashford",
    "Barrowfen",
    "Coldwater",
    "Dunridge",
    "Eastmere",
    "Foxhollow",
    "Grayhaven",
    "High Tor",
    "Ironvale",
    "Juniper Cross",
    "Kingswash",
    "Low Hearth",
    "Mossward",
    "Northpass",
    "Oakenbridge",
    "Red Quarry",
];
const SURNAMES: &[&str] = &[
    "Ardin", "Voss", "Tamer", "Cald", "Merrow", "Hale", "Orin", "Fen",
];
const GIVEN_NAMES: &[&str] = &[
    "Mara", "Elian", "Tomas", "Sera", "Bran", "Ilyra", "Corin", "Nessa", "Alden", "Vera", "Garran",
    "Mira", "Oren", "Talia", "Beren", "Lysa", "Joren", "Kira", "Darin", "Rhea", "Cass", "Nolan",
    "Eris", "Perrin", "Anya", "Lucan", "Thea", "Roric", "Sel", "Wren",
];
const NAME_STARTS: &[&str] = &[
    "Al", "Ber", "Cor", "Dar", "El", "Fen", "Gar", "Ily", "Mar", "Nor", "Or", "Tal",
];
const NAME_ENDS: &[&str] = &[
    "a", "an", "en", "ia", "in", "is", "on", "or", "ra", "ren", "ric", "yn",
];
