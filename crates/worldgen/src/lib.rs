//! Converts generated history into a playable semantic site.
//!
//! Spatial templates are authored, but every named resident, faction hall,
//! workplace, evidence location, and initial lead is bound from historical IDs.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use thiserror::Error;
use ultimate_fate_content::{MaterialKind, WorldRules};
use ultimate_fate_core::{
    AmmunitionKind, Combatant, Direction, Entity, EntityId, EntityKind, GameCommand,
    GameplayBuildError, GridPos, Item, ItemId, ItemKind, Landmark, LandmarkKind, MapId, Quest,
    QuestId, QuestObjective, QuestObjectiveKind, QuestStatus, RandomStream, Simulation,
    SimulationBuildError, StreamId, TerrainCell, TerrainKind, Transition, TransitionKind, WorldMap,
    WorldPosition,
};
use ultimate_fate_history::{
    Drive, EntityRef, EventId, FactionId, HistoricalEventKind, HistoricalWorld, LawId, LawKind,
    Occupation, PartyId, PersonId, PhysicalEvidenceKind, ProjectId, RegionalGoalKind,
    RegionalPartyKind, RegionalPartyStatus, RouteId, SettlementProjectKind, SettlementProjectPhase,
    SiteId, WorldItemId,
};
use ultimate_fate_text::{BriefingSectionKind, CampaignStart, physical_evidence_name};
use ultimate_fate_world_atlas::{AtlasPosition, Biome, WaterBody, WorldAtlas};

const SITE_LAYOUT_STREAM: StreamId = StreamId(0x5349_5445_4c41_594f);
const TERRAIN_DETAIL_STREAM: StreamId = StreamId(0x5349_5445_5445_5252);
const ENCOUNTER_STREAM: StreamId = StreamId(0x454e_434f_554e_5445);
const DUNGEON_STREAM: StreamId = StreamId(0x4455_4e47_454f_4e20);
const PLAYER_ENTITY: EntityId = EntityId(1);
const ENCOUNTER_ENTITY: EntityId = EntityId(0xf000_0001);
const DUNGEON_ENEMY_BASE: u64 = 0xf100_0000;
const DUNGEON_BOSS: EntityId = EntityId(0xf1ff_ffff);
const REGIONAL_PARTY_BASE: u64 = 0xe000_0000_0000_0000;
const STARTER_SWORD: ItemId = ItemId(1);
const HUNTING_BOW: ItemId = ItemId(2);
const ARROWS: ItemId = ItemId(3);
const FIELD_DRESSING: ItemId = ItemId(4);
const DUNGEON_RELIC: ItemId = ItemId(0x1000);
const DUNGEON_REAGENT_BASE: u64 = 0x1100;
const ACCESS_MEDICINE: ItemId = ItemId(0x1200);
const DUNGEON_QUEST: QuestId = QuestId(1);
pub const LOCAL_DAY_TURNS: u64 = 240;
const LOCAL_DAY_PHASE_OFFSET: u64 = 144;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationSource {
    ContactWorkplace(PersonId),
    HistoricalEvidence(EventId),
    FactionSeat(FactionId),
    Dungeon(EventId),
    SettlementProject(ProjectId),
    Civic,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedLocation {
    pub name: String,
    pub kind: LandmarkKind,
    pub position: GridPos,
    pub source: LocationSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedResident {
    pub person: PersonId,
    pub entity: EntityId,
    pub name: String,
    pub occupation: Occupation,
    pub faction: FactionId,
    /// The initial and daytime destination. Kept as `position` for compatibility
    /// with interaction and lead code.
    pub position: GridPos,
    pub home_position: GridPos,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResidentActivity {
    Working,
    AtLeisure,
    AtHome,
    SeekingFood,
    SeekingSafety,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncounterKind {
    StarvingWolf,
    FeralBoar,
    AbandonedWarHound,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedEncounter {
    pub entity: EntityId,
    pub kind: EncounterKind,
    pub name: String,
    pub description: String,
    pub related_event: EventId,
    pub position: GridPos,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedDungeonEnemy {
    pub entity: EntityId,
    pub name: String,
    pub position: GridPos,
    pub health: i16,
    pub armor: i16,
    pub experience: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedDungeonLevel {
    pub depth: u8,
    pub name: String,
    pub historical_context: String,
    pub entry: GridPos,
    pub descent: Option<GridPos>,
    pub enemies: Vec<PlannedDungeonEnemy>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedDungeon {
    pub name: String,
    pub description: String,
    pub related_event: EventId,
    pub world_item: WorldItemId,
    pub entrance: GridPos,
    pub levels: Vec<PlannedDungeonLevel>,
    pub boss: EntityId,
    pub boss_name: String,
    pub relic: Item,
    pub reagents: Vec<Item>,
    pub quest: QuestId,
    pub quest_title: String,
    pub quest_description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedLivingProject {
    pub project: ProjectId,
    pub position: GridPos,
    pub phase: SettlementProjectPhase,
}

/// A local material need projected from generated people, law, resources, and
/// history. The plan identifies the initial state; campaign resolution is
/// observed from actual custody and law state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedAidSituation {
    pub cause: EventId,
    pub patient: PersonId,
    pub patient_entity: EntityId,
    pub patient_name: String,
    pub custodian: PersonId,
    pub custodian_entity: EntityId,
    pub custodian_name: String,
    pub advocate: PersonId,
    pub advocate_entity: EntityId,
    pub advocate_name: String,
    pub restricting_law: Option<LawId>,
    pub medicine: Item,
    pub price: i64,
    pub title: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedRegionalSite {
    pub site: SiteId,
    pub name: String,
    pub position: GridPos,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedRegionalRoute {
    pub route: RouteId,
    pub name: String,
    pub first: SiteId,
    pub second: SiteId,
    pub position: GridPos,
    pub path: Vec<GridPos>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedRegionalHistorySite {
    pub event: EventId,
    pub name: String,
    pub description: String,
    pub position: GridPos,
    pub kind: LandmarkKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayableSitePlan {
    pub campaign_seed: u64,
    pub rules: WorldRules,
    pub site: SiteId,
    pub map: MapId,
    pub regional_map: MapId,
    pub town_name: String,
    pub player_spawn: GridPos,
    pub contact: PersonId,
    pub contact_name: String,
    pub contact_location: String,
    pub crisis_event: EventId,
    pub evidence_event: EventId,
    pub evidence_location: String,
    pub evidence_description: String,
    pub encounter: PlannedEncounter,
    pub aid: PlannedAidSituation,
    pub dungeon: PlannedDungeon,
    pub living_projects: Vec<PlannedLivingProject>,
    pub regional_sites: Vec<PlannedRegionalSite>,
    pub regional_routes: Vec<PlannedRegionalRoute>,
    pub regional_history_sites: Vec<PlannedRegionalHistorySite>,
    pub overworld: WorldAtlas,
    pub starter_sword: Item,
    pub starter_sword_provenance: String,
    pub hunting_bow: ItemId,
    pub locations: Vec<PlannedLocation>,
    pub residents: Vec<PlannedResident>,
    transform: LayoutTransform,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SitePlanError {
    #[error("campaign start references missing site {0}")]
    MissingSite(SiteId),
    #[error("campaign start references missing contact {0}")]
    MissingContact(PersonId),
    #[error("campaign start has no physical evidence lead")]
    MissingEvidenceLead,
    #[error("site does not contain evidence created by event {0}")]
    MissingEvidence(EventId),
    #[error("history contains no strategic item born from event {0}")]
    MissingStrategicItem(EventId),
    #[error("generated simulation was invalid: {0:?}")]
    InvalidSimulation(SimulationBuildError),
    #[error("generated gameplay setup was invalid: {0:?}")]
    InvalidGameplay(GameplayBuildError),
    #[error("generated site has fewer than three distinct residents for a local aid situation")]
    InsufficientAidActors,
}

impl PlayableSitePlan {
    pub fn from_history(
        world: &HistoricalWorld,
        start: &CampaignStart,
    ) -> Result<Self, SitePlanError> {
        let site_id = start.briefing.location;
        let site = world
            .sites()
            .get(&site_id)
            .ok_or(SitePlanError::MissingSite(site_id))?;
        let contact = world
            .people()
            .get(&start.arrival_contact)
            .filter(|person| person.is_alive())
            .ok_or(SitePlanError::MissingContact(start.arrival_contact))?;
        let evidence_event = start
            .lead_evidence
            .ok_or(SitePlanError::MissingEvidenceLead)?;
        let crisis_event = start
            .briefing
            .paragraphs
            .iter()
            .find(|paragraph| paragraph.kind == BriefingSectionKind::PresentCrisis)
            .and_then(|paragraph| paragraph.events.first().copied())
            .unwrap_or(evidence_event);
        let evidence = site
            .physical_evidence
            .iter()
            .find(|evidence| evidence.originating_event == evidence_event)
            .ok_or(SitePlanError::MissingEvidence(evidence_event))?;
        let transform = LayoutTransform::from_seed(world.campaign_seed);
        let map = MapId(site_id.0 as u32);
        let contact_name = person_name(world, contact.id);
        let contact_location = workplace_name(world, contact.id, site.name.as_str());
        let evidence_location = evidence_title(evidence.kind);
        let encounter = plan_encounter(world, crisis_event, transform);
        let crisis_law = site
            .laws
            .values()
            .filter(|law| law.active)
            .min_by_key(|law| law.id)
            .map(|law| law.kind)
            .unwrap_or(LawKind::FoodRationing);
        let dungeon = plan_dungeon(world, crisis_event, crisis_law, contact.id, transform)?;
        let contact_faction = &world.factions()[&contact.faction];
        let starter_sword = Item {
            id: STARTER_SWORD,
            name: format!("{} Levy Sword", contact_faction.name),
            kind: ItemKind::MeleeWeapon { damage: 4 },
            quantity: 1,
            weight_grams: 1_300,
            quality: 52,
        };
        let starter_sword_provenance = format!(
            "Issued by {} during the emergency following the year {} crisis.",
            contact_faction.name,
            world.events()[&crisis_event].date.year
        );

        let contact_anchor = transform.apply(GridPos::new(-13, 3, 0));
        let evidence_anchor = transform.apply(GridPos::new(-6, -6, 0));
        let faction_anchors = [
            transform.apply(GridPos::new(6, -6, 0)),
            transform.apply(GridPos::new(-6, 6, 0)),
            transform.apply(GridPos::new(6, 6, 0)),
        ];

        let mut locations = vec![
            PlannedLocation {
                name: contact_location.clone(),
                kind: workplace_kind(contact.occupation),
                position: contact_anchor,
                source: LocationSource::ContactWorkplace(contact.id),
            },
            PlannedLocation {
                name: evidence_location.clone(),
                kind: evidence_kind(evidence.kind),
                position: evidence_anchor,
                source: LocationSource::HistoricalEvidence(evidence_event),
            },
            PlannedLocation {
                name: dungeon.name.clone(),
                kind: LandmarkKind::DungeonEntrance,
                position: dungeon.entrance,
                source: LocationSource::Dungeon(crisis_event),
            },
        ];
        locations.extend(world.factions().values().zip(faction_anchors).map(
            |(faction, position)| PlannedLocation {
                name: faction.name.clone(),
                kind: LandmarkKind::GuildHall,
                position,
                source: LocationSource::FactionSeat(faction.id),
            },
        ));
        locations.extend([
            PlannedLocation {
                name: format!("{} Market Square", site.name),
                kind: LandmarkKind::TownSquare,
                position: transform.apply(GridPos::new(0, 0, 0)),
                source: LocationSource::Civic,
            },
            PlannedLocation {
                name: format!("{} River Dock", site.name),
                kind: LandmarkKind::RiverDock,
                position: transform.apply(GridPos::new(19, 1, 0)),
                source: LocationSource::Civic,
            },
        ]);
        locations.extend(
            [
                GridPos::new(0, -48, 0),
                GridPos::new(48, 0, 0),
                GridPos::new(0, 48, 0),
                GridPos::new(-48, 0, 0),
            ]
            .into_iter()
            .map(|position| PlannedLocation {
                name: "World Road Gate".to_string(),
                kind: LandmarkKind::Gate,
                position: transform.apply(position),
                source: LocationSource::Civic,
            }),
        );
        let project_anchors = [(-15, -8), (-20, 10), (12, 16), (25, 8)];
        let living_projects = world
            .projects()
            .values()
            .filter(|project| project.site == site_id)
            .enumerate()
            .map(|(index, project)| {
                let (x, y) = project_anchors[index % project_anchors.len()];
                PlannedLivingProject {
                    project: project.id,
                    position: transform.apply(GridPos::new(x, y, 0)),
                    phase: project.phase,
                }
            })
            .collect::<Vec<_>>();
        locations.extend(living_projects.iter().filter_map(|planned| {
            let project = world.projects().get(&planned.project)?;
            Some(PlannedLocation {
                name: project.name.clone(),
                kind: project_landmark_kind(project.kind),
                position: planned.position,
                source: LocationSource::SettlementProject(project.id),
            })
        }));

        let residents = plan_residents(
            world,
            site_id,
            contact.id,
            contact_anchor,
            &locations,
            transform,
        );
        let aid = plan_aid_situation(world, site_id, crisis_event, &residents)?;
        let regional_map = MapId(0x8000_0000 ^ map.0);
        let (regional_sites, regional_routes) = plan_region(world);
        let regional_history_sites = plan_regional_history_sites(world, &regional_routes);

        Ok(Self {
            campaign_seed: world.campaign_seed,
            rules: world.rules().clone(),
            site: site_id,
            map,
            regional_map,
            town_name: site.name.clone(),
            player_spawn: transform.apply(GridPos::new(-20, 0, 0)),
            contact: contact.id,
            contact_name,
            contact_location,
            crisis_event,
            evidence_event,
            evidence_location,
            evidence_description: evidence.description.clone(),
            encounter,
            aid,
            dungeon,
            living_projects,
            regional_sites,
            regional_routes,
            regional_history_sites,
            overworld: world.atlas().clone(),
            starter_sword,
            starter_sword_provenance,
            hunting_bow: HUNTING_BOW,
            locations,
            residents,
            transform,
        })
    }

    pub fn first_objective(&self) -> String {
        format!("Meet {} at {}", self.contact_name, self.contact_location)
    }

    pub fn evidence_objective(&self) -> String {
        format!("Inspect {}", self.evidence_location)
    }

    pub fn contact_resident(&self) -> &PlannedResident {
        self.residents
            .iter()
            .find(|resident| resident.person == self.contact)
            .expect("site plan always includes its contact")
    }

    pub fn evidence_location(&self) -> &PlannedLocation {
        self.locations
            .iter()
            .find(|location| {
                location.source == LocationSource::HistoricalEvidence(self.evidence_event)
            })
            .expect("site plan always includes its evidence")
    }

    pub fn regional_goal_target(&self, kind: RegionalGoalKind) -> Option<(&str, GridPos)> {
        match kind {
            RegionalGoalKind::SecureRoute(route) => self
                .regional_routes
                .iter()
                .find(|planned| planned.route == route)
                .map(|planned| (planned.name.as_str(), planned.position)),
            RegionalGoalKind::RelieveShortage(site) => self
                .regional_sites
                .iter()
                .find(|planned| planned.site == site)
                .map(|planned| (planned.name.as_str(), planned.position)),
        }
    }

    pub fn regional_gate(&self) -> GridPos {
        self.regional_gates()[0]
    }

    pub fn regional_gates(&self) -> [GridPos; 4] {
        [
            GridPos::new(0, -48, 0),
            GridPos::new(48, 0, 0),
            GridPos::new(0, 48, 0),
            GridPos::new(-48, 0, 0),
        ]
        .map(|position| self.transform.apply(position))
    }

    pub fn nearest_regional_gate(&self, from: GridPos) -> GridPos {
        self.regional_gates()
            .into_iter()
            .min_by_key(|gate| (gate.x - from.x).abs() + (gate.y - from.y).abs())
            .expect("a town always has four world-road gates")
    }

    pub fn build_simulation(&self) -> Result<Simulation, SitePlanError> {
        let map = self.build_map();
        let player = Entity {
            id: PLAYER_ENTITY,
            kind: EntityKind::Player,
            position: WorldPosition {
                map: self.map,
                grid: self.player_spawn,
            },
            facing: Direction::East,
        };
        let residents = self.residents.iter().map(|resident| Entity {
            id: resident.entity,
            kind: EntityKind::Character,
            position: WorldPosition {
                map: self.map,
                grid: resident.position,
            },
            facing: Direction::South,
        });
        let encounter = Entity {
            id: self.encounter.entity,
            kind: EntityKind::Creature,
            position: WorldPosition {
                map: self.map,
                grid: self.encounter.position,
            },
            facing: Direction::West,
        };
        let dungeon_enemies = self.dungeon.levels.iter().flat_map(|level| {
            level.enemies.iter().map(|enemy| Entity {
                id: enemy.entity,
                kind: EntityKind::Creature,
                position: WorldPosition {
                    map: self.map,
                    grid: enemy.position,
                },
                facing: Direction::West,
            })
        });
        let landmarks = self
            .locations
            .iter()
            .map(|location| Landmark {
                name: location.name.clone(),
                kind: location.kind,
                position: WorldPosition {
                    map: self.map,
                    grid: location.position,
                },
            })
            .chain(self.dungeon.levels.iter().map(|level| Landmark {
                name: level.name.clone(),
                kind: LandmarkKind::Ruin,
                position: WorldPosition {
                    map: self.map,
                    grid: level.entry,
                },
            }))
            .collect();

        let mut simulation = Simulation::from_map_with_rules(
            self.campaign_seed,
            self.rules.clone(),
            map,
            std::iter::once(player)
                .chain(residents)
                .chain(std::iter::once(encounter))
                .chain(dungeon_enemies),
            landmarks,
            PLAYER_ENTITY,
        )
        .map_err(SitePlanError::InvalidSimulation)?;
        for resident in self
            .residents
            .iter()
            .filter(|resident| resident.occupation == Occupation::Healer)
        {
            simulation
                .register_healer(resident.entity)
                .map_err(SitePlanError::InvalidGameplay)?;
        }
        let regional_landmarks = self
            .regional_sites
            .iter()
            .map(|planned| Landmark {
                name: planned.name.clone(),
                kind: LandmarkKind::TownSquare,
                position: WorldPosition {
                    map: self.regional_map,
                    grid: planned.position,
                },
            })
            .chain(self.regional_history_sites.iter().map(|planned| Landmark {
                name: "Old raid site".to_string(),
                kind: planned.kind,
                position: WorldPosition {
                    map: self.regional_map,
                    grid: planned.position,
                },
            }));
        simulation
            .add_map(self.build_region_map(), regional_landmarks)
            .map_err(SitePlanError::InvalidGameplay)?;
        simulation
            .add_combatant(Combatant {
                entity: PLAYER_ENTITY,
                health: 12,
                max_health: 12,
                armor: 0,
                hostile_to_player: false,
                experience_reward: 0,
            })
            .map_err(SitePlanError::InvalidGameplay)?;
        for enemy in self
            .dungeon
            .levels
            .iter()
            .flat_map(|level| level.enemies.iter())
        {
            simulation
                .add_combatant(Combatant {
                    entity: enemy.entity,
                    health: enemy.health,
                    max_health: enemy.health,
                    armor: enemy.armor,
                    hostile_to_player: true,
                    experience_reward: enemy.experience,
                })
                .map_err(SitePlanError::InvalidGameplay)?;
        }
        simulation
            .add_combatant(Combatant {
                entity: self.encounter.entity,
                health: 8,
                max_health: 8,
                armor: 0,
                hostile_to_player: true,
                experience_reward: 35,
            })
            .map_err(SitePlanError::InvalidGameplay)?;
        simulation
            .give_item(self.contact_resident().entity, self.starter_sword.clone())
            .map_err(SitePlanError::InvalidGameplay)?;
        simulation
            .give_item(self.aid.custodian_entity, self.aid.medicine.clone())
            .map_err(SitePlanError::InvalidGameplay)?;
        for item in starter_items() {
            simulation
                .give_item(PLAYER_ENTITY, item)
                .map_err(SitePlanError::InvalidGameplay)?;
        }
        simulation
            .give_item(self.dungeon.boss, self.dungeon.relic.clone())
            .map_err(SitePlanError::InvalidGameplay)?;
        for reagent in &self.dungeon.reagents {
            simulation
                .give_item(self.dungeon.boss, reagent.clone())
                .map_err(SitePlanError::InvalidGameplay)?;
        }
        for transition in dungeon_transitions(self.map, &self.dungeon) {
            simulation
                .add_transition(transition)
                .map_err(SitePlanError::InvalidGameplay)?;
        }
        let town_gates = self.regional_gates();
        let town_gate = self.regional_gate();
        let regional_capital = self
            .regional_sites
            .iter()
            .find(|planned| planned.site == self.site)
            .expect("regional plan contains the detailed town")
            .position;
        for town_exit in town_gates {
            simulation
                .add_transition(Transition {
                    from: WorldPosition {
                        map: self.map,
                        grid: town_exit,
                    },
                    to: WorldPosition {
                        map: self.regional_map,
                        grid: regional_capital,
                    },
                    kind: TransitionKind::Descend,
                    name: "Travel into the region".to_string(),
                })
                .map_err(SitePlanError::InvalidGameplay)?;
        }
        simulation
            .add_transition(Transition {
                from: WorldPosition {
                    map: self.regional_map,
                    grid: regional_capital,
                },
                to: WorldPosition {
                    map: self.map,
                    grid: town_gate,
                },
                kind: TransitionKind::Ascend,
                name: format!("Enter {}", self.town_name),
            })
            .map_err(SitePlanError::InvalidGameplay)?;
        simulation
            .add_quest(Quest {
                id: self.dungeon.quest,
                title: self.dungeon.quest_title.clone(),
                description: self.dungeon.quest_description.clone(),
                giver: self.contact_resident().entity,
                status: QuestStatus::Active,
                objectives: vec![
                    QuestObjective {
                        description: format!("Recover {}", self.dungeon.relic.name),
                        kind: QuestObjectiveKind::Recover(self.dungeon.relic.id),
                        completed: false,
                    },
                    QuestObjective {
                        description: format!("Defeat {}", self.dungeon.boss_name),
                        kind: QuestObjectiveKind::Defeat(self.dungeon.boss),
                        completed: false,
                    },
                ],
                reward_experience: 120,
            })
            .map_err(SitePlanError::InvalidGameplay)?;
        simulation.apply_command(GameCommand::Equip(HUNTING_BOW));
        Ok(simulation)
    }

    pub fn synchronize_living_projects(
        &self,
        world: &HistoricalWorld,
        simulation: &mut Simulation,
    ) -> usize {
        let mut changed = 0;
        for planned in &self.living_projects {
            let Some(project) = world.projects().get(&planned.project) else {
                continue;
            };
            for (position, cell) in project_cells(planned.position, project.phase) {
                changed += usize::from(simulation.set_terrain_cell(self.map, position, cell));
            }
        }
        changed
    }

    pub fn synchronize_regional_routes(
        &self,
        world: &HistoricalWorld,
        simulation: &mut Simulation,
    ) -> usize {
        self.regional_routes
            .iter()
            .filter_map(|planned| {
                let route = world.routes().get(&planned.route)?;
                simulation
                    .set_terrain_cell(
                        self.regional_map,
                        planned.position,
                        TerrainCell::new(if route.disrupted {
                            TerrainKind::Rubble
                        } else {
                            route_terrain(&self.overworld, planned.position)
                        }),
                    )
                    .then_some(())
            })
            .count()
    }

    pub fn regional_party_entity(party: PartyId) -> EntityId {
        EntityId(REGIONAL_PARTY_BASE | party.0)
    }

    pub fn regional_party_position(
        &self,
        world: &HistoricalWorld,
        party: PartyId,
    ) -> Option<GridPos> {
        let party = world.regional_parties().get(&party)?;
        let route = self
            .regional_routes
            .iter()
            .find(|planned| planned.route == party.route)?;
        let mut path = route.path.clone();
        if party.origin == route.second {
            path.reverse();
        }
        let index =
            usize::from(party.progress).saturating_mul(path.len().saturating_sub(1)) / 1_000;
        path.get(index).copied()
    }

    pub fn synchronize_regional_parties(
        &self,
        world: &HistoricalWorld,
        simulation: &mut Simulation,
    ) -> usize {
        let mut changed = 0;
        for party in world.regional_parties().values() {
            let entity_id = Self::regional_party_entity(party.id);
            if matches!(
                party.status,
                RegionalPartyStatus::Arrived | RegionalPartyStatus::Defeated
            ) {
                changed += usize::from(simulation.remove_entity(entity_id));
                continue;
            }
            let Some(position) = self.regional_party_position(world, party.id) else {
                continue;
            };
            let world_position = WorldPosition {
                map: self.regional_map,
                grid: position,
            };
            if simulation.entity(entity_id).is_none() {
                let hostile = matches!(party.kind, RegionalPartyKind::Raiders { .. });
                if simulation
                    .add_entity(Entity {
                        id: entity_id,
                        kind: if hostile {
                            EntityKind::Creature
                        } else {
                            EntityKind::Character
                        },
                        position: world_position,
                        facing: Direction::South,
                    })
                    .is_ok()
                {
                    changed += 1;
                    if hostile {
                        let strength = match party.kind {
                            RegionalPartyKind::Raiders { strength } => strength,
                            _ => 0,
                        };
                        let _ = simulation.add_combatant(Combatant {
                            entity: entity_id,
                            health: 6 + i16::from(strength / 20),
                            max_health: 6 + i16::from(strength / 20),
                            armor: i16::from(strength / 35),
                            hostile_to_player: true,
                            experience_reward: 30 + u32::from(strength),
                        });
                    }
                }
            } else {
                changed += usize::from(simulation.move_entity(entity_id, world_position));
            }
        }
        changed
    }

    pub fn resident_activity(
        &self,
        person: PersonId,
        tick: u64,
    ) -> Option<(ResidentActivity, GridPos)> {
        let index = self
            .residents
            .iter()
            .position(|resident| resident.person == person)?;
        let resident = &self.residents[index];
        // A campaign opens near the end of the work period. Residents therefore
        // demonstrate the living schedule within a few heartbeat turns rather
        // than appearing static for the first several minutes.
        let local_tick = tick.saturating_add(LOCAL_DAY_PHASE_OFFSET);
        let turn = local_tick % LOCAL_DAY_TURNS;
        let day = (local_tick / LOCAL_DAY_TURNS) % 7;
        let resting = day == 6;
        if !resting && turn < 150 {
            Some((ResidentActivity::Working, resident.position))
        } else if (!resting && turn < 190) || (resting && (60..150).contains(&turn)) {
            Some((
                ResidentActivity::AtLeisure,
                resident_social_position(self.transform, index),
            ))
        } else {
            Some((ResidentActivity::AtHome, resident.home_position))
        }
    }

    pub fn resident_destination(
        &self,
        person: PersonId,
        activity: ResidentActivity,
    ) -> Option<GridPos> {
        let index = self
            .residents
            .iter()
            .position(|resident| resident.person == person)?;
        let resident = &self.residents[index];
        let destination = match activity {
            ResidentActivity::Working => resident.position,
            ResidentActivity::AtLeisure => resident_social_position(self.transform, index),
            ResidentActivity::AtHome => resident.home_position,
            ResidentActivity::SeekingFood => {
                let anchor = self
                    .locations
                    .iter()
                    .find(|location| {
                        matches!(location.kind, LandmarkKind::Granary | LandmarkKind::Shop)
                    })
                    .map(|location| location.position)
                    .unwrap_or(resident.position);
                resident_service_position(anchor, index)
            }
            ResidentActivity::SeekingSafety => {
                let anchor = self
                    .locations
                    .iter()
                    .find(|location| {
                        matches!(
                            location.kind,
                            LandmarkKind::Temple | LandmarkKind::CouncilHall
                        )
                    })
                    .map(|location| location.position)
                    .unwrap_or(resident.home_position);
                resident_service_position(anchor, index)
            }
        };
        Some(destination)
    }

    /// Moves residents toward goals selected by the campaign agent layer. The
    /// site plan owns pathing and semantic destinations, while motives and needs
    /// remain authoritative campaign state.
    pub fn advance_resident_goals(
        &self,
        tick: u64,
        activities: &BTreeMap<PersonId, ResidentActivity>,
        simulation: &mut Simulation,
    ) -> usize {
        let mut occupied = simulation
            .entities()
            .filter(|entity| entity.position.map == self.map && entity.position.grid.z == 0)
            .map(|entity| entity.position.grid)
            .collect::<BTreeSet<_>>();
        let mut moved = 0;
        for (index, resident) in self.residents.iter().enumerate() {
            if tick % 4 != index as u64 % 4 {
                continue;
            }
            let Some(entity) = simulation.entity(resident.entity).cloned() else {
                continue;
            };
            if entity.position.map != self.map || entity.position.grid.z != 0 {
                continue;
            }
            let activity = activities
                .get(&resident.person)
                .copied()
                .or_else(|| {
                    self.resident_activity(resident.person, tick)
                        .map(|(activity, _)| activity)
                })
                .unwrap_or(ResidentActivity::AtHome);
            let Some(destination) = self.resident_destination(resident.person, activity) else {
                continue;
            };
            if entity.position.grid == destination {
                continue;
            }
            occupied.remove(&entity.position.grid);
            let next = next_resident_step(
                simulation,
                self.map,
                entity.position.grid,
                destination,
                &occupied,
            );
            if let Some(next) = next {
                moved += usize::from(simulation.move_entity(
                    resident.entity,
                    WorldPosition {
                        map: self.map,
                        grid: next,
                    },
                ));
                occupied.insert(next);
            } else {
                occupied.insert(entity.position.grid);
            }
        }
        moved
    }

    /// Advances materialized town inhabitants toward their current daily
    /// destination. Their identities and destinations belong to the site plan;
    /// the renderer only observes entity positions.
    pub fn advance_resident_schedules(&self, tick: u64, simulation: &mut Simulation) -> usize {
        let mut occupied = simulation
            .entities()
            .filter(|entity| entity.position.map == self.map && entity.position.grid.z == 0)
            .map(|entity| entity.position.grid)
            .collect::<BTreeSet<_>>();
        let mut moved = 0;
        for (index, resident) in self.residents.iter().enumerate() {
            // Stagger pathfinding and walking while retaining deterministic
            // schedules. Four game turns are a small fraction of an in-world hour.
            if tick % 4 != index as u64 % 4 {
                continue;
            }
            let Some(entity) = simulation.entity(resident.entity).cloned() else {
                continue;
            };
            if entity.position.map != self.map || entity.position.grid.z != 0 {
                continue;
            }
            let Some((_, destination)) = self.resident_activity(resident.person, tick) else {
                continue;
            };
            if entity.position.grid == destination {
                continue;
            }
            occupied.remove(&entity.position.grid);
            let next = next_resident_step(
                simulation,
                self.map,
                entity.position.grid,
                destination,
                &occupied,
            );
            if let Some(next) = next {
                moved += usize::from(simulation.move_entity(
                    resident.entity,
                    WorldPosition {
                        map: self.map,
                        grid: next,
                    },
                ));
                occupied.insert(next);
            } else {
                occupied.insert(entity.position.grid);
            }
        }
        moved
    }

    fn build_map(&self) -> WorldMap {
        let mut map = WorldMap::new(self.map);
        let mut terrain_rng = RandomStream::new(self.campaign_seed, TERRAIN_DETAIL_STREAM);

        for y in -48..=48 {
            for x in -48..=48 {
                let mut terrain = if (-1..=1).contains(&x) || (-1..=1).contains(&y) {
                    TerrainKind::Road
                } else if terrain_rng.one_in(41) {
                    TerrainKind::Dirt
                } else {
                    TerrainKind::Grass
                };
                if (14..=15).contains(&x) {
                    terrain = if (-1..=1).contains(&y) {
                        TerrainKind::Bridge
                    } else {
                        TerrainKind::Water
                    };
                }
                set_transformed(&mut map, self.transform, x, y, TerrainCell::new(terrain));
            }
        }

        paint_rect(
            &mut map,
            self.transform,
            (-3, -3, 3, 3),
            TerrainCell::new(TerrainKind::StoneFloor),
        );
        for bounds in [(-31, -16, -23, -7), (-31, 8, -23, 17)] {
            paint_rect(
                &mut map,
                self.transform,
                bounds,
                TerrainCell::new(TerrainKind::Farmland),
            );
        }
        paint_building(&mut map, self.transform, (-16, 3, -11, 8), (-13, 3));
        match self.evidence_location().kind {
            LandmarkKind::Farm => {
                paint_rect(
                    &mut map,
                    self.transform,
                    (-10, -13, -2, -5),
                    TerrainCell::new(TerrainKind::Farmland),
                );
                paint_building(&mut map, self.transform, (-8, -9, -4, -6), (-6, -6));
            }
            LandmarkKind::Memorial => paint_rect(
                &mut map,
                self.transform,
                (-8, -8, -4, -4),
                TerrainCell::new(TerrainKind::StoneFloor),
            ),
            _ => paint_building(&mut map, self.transform, (-9, -12, -3, -6), (-6, -6)),
        }
        for (bounds, entrance) in [
            ((3, -12, 9, -6), (6, -6)),
            ((-9, 6, -3, 12), (-6, 6)),
            ((3, 6, 9, 12), (6, 6)),
        ] {
            paint_building(&mut map, self.transform, bounds, entrance);
        }
        paint_rect(
            &mut map,
            self.transform,
            (17, -3, 21, 3),
            TerrainCell::new(TerrainKind::StoneFloor),
        );
        for dy in -3_i32..=3 {
            for dx in -3_i32..=3 {
                let boundary_corner = dx.abs() == 3 && dy.abs() == 3;
                map.set_cell(
                    self.dungeon.entrance.offset(dx, dy, 0),
                    TerrainCell::new(if boundary_corner {
                        TerrainKind::Wall
                    } else {
                        TerrainKind::StoneFloor
                    }),
                );
            }
        }
        map.set_cell(
            self.dungeon.entrance,
            TerrainCell::new(TerrainKind::StairsDown),
        );
        paint_dungeon(&mut map, self.campaign_seed, &self.dungeon);
        for project in &self.living_projects {
            for (position, cell) in project_cells(project.position, project.phase) {
                map.set_cell(position, cell);
            }
        }
        for regional_gate in self.regional_gates() {
            map.set_cell(regional_gate, TerrainCell::new(TerrainKind::StairsDown));
        }
        map
    }

    fn build_region_map(&self) -> WorldMap {
        let mut map = WorldMap::new(self.regional_map);
        for (position, cell) in self.overworld.cells() {
            map.set_cell(atlas_grid(position), TerrainCell::new(atlas_terrain(*cell)));
        }
        for route in &self.regional_routes {
            for position in &route.path {
                map.set_cell(
                    *position,
                    TerrainCell::new(route_terrain(&self.overworld, *position)),
                );
            }
        }
        for site in &self.regional_history_sites {
            map.set_cell(site.position, TerrainCell::new(TerrainKind::Dirt));
        }
        for site in &self.regional_sites {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    map.set_cell(
                        site.position.offset(dx, dy, 0),
                        TerrainCell::new(TerrainKind::StoneFloor),
                    );
                }
            }
        }
        if let Some(capital) = self
            .regional_sites
            .iter()
            .find(|planned| planned.site == self.site)
        {
            map.set_cell(capital.position, TerrainCell::new(TerrainKind::StairsUp));
        }
        for route in &self.regional_routes {
            map.set_cell(route.position, TerrainCell::new(TerrainKind::Road));
        }
        map
    }
}

fn plan_region(world: &HistoricalWorld) -> (Vec<PlannedRegionalSite>, Vec<PlannedRegionalRoute>) {
    let sites = world
        .regional_settlements()
        .values()
        .map(|settlement| PlannedRegionalSite {
            site: settlement.site,
            name: world.sites()[&settlement.site].name.clone(),
            position: atlas_grid(settlement.position),
        })
        .collect::<Vec<_>>();
    let routes = world
        .routes()
        .values()
        .map(|route| {
            let path = route
                .path
                .iter()
                .copied()
                .map(atlas_grid)
                .collect::<Vec<_>>();
            PlannedRegionalRoute {
                route: route.id,
                name: route.name.clone(),
                first: route.first,
                second: route.second,
                position: path[path.len() / 2],
                path,
            }
        })
        .collect();
    (sites, routes)
}

fn plan_regional_history_sites(
    world: &HistoricalWorld,
    routes: &[PlannedRegionalRoute],
) -> Vec<PlannedRegionalHistorySite> {
    routes
        .iter()
        .filter_map(|route| {
            let event = world.events().values().rev().find(|event| {
                event.kind == HistoricalEventKind::RouteDisrupted
                    && event.participants.contains(&EntityRef::Route(route.route))
            })?;
            let interior = route.path.len().saturating_sub(4);
            let index = if interior == 0 {
                route.path.len() / 2
            } else {
                2 + event.id.0 as usize % interior
            };
            Some(PlannedRegionalHistorySite {
                event: event.id,
                name: format!("Old raid site: {}", route.name),
                description: format!("In year {}, {}", event.date.year, event.summary),
                position: route.path[index],
                kind: LandmarkKind::Ruin,
            })
        })
        .collect()
}

fn atlas_grid(position: AtlasPosition) -> GridPos {
    GridPos::new(i32::from(position.x), i32::from(position.y), 0)
}

fn atlas_terrain(cell: ultimate_fate_world_atlas::AtlasCell) -> TerrainKind {
    match cell.water {
        WaterBody::Ocean => return TerrainKind::Ocean,
        WaterBody::River | WaterBody::Lake => return TerrainKind::Water,
        WaterBody::None => {}
    }
    match cell.biome {
        Biome::Ocean => TerrainKind::Ocean,
        Biome::Coast | Biome::Desert => TerrainKind::Sand,
        Biome::Grassland => TerrainKind::Grass,
        Biome::Forest => TerrainKind::Forest,
        Biome::Swamp => TerrainKind::Swamp,
        Biome::Tundra => TerrainKind::Snow,
        Biome::Hills => TerrainKind::Hills,
        Biome::Mountains => TerrainKind::Mountain,
    }
}

fn route_terrain(atlas: &WorldAtlas, position: GridPos) -> TerrainKind {
    let position = AtlasPosition::new(position.x as i16, position.y as i16);
    if atlas
        .cell(position)
        .is_some_and(|cell| cell.water == WaterBody::River)
    {
        TerrainKind::Bridge
    } else {
        TerrainKind::Road
    }
}

fn project_landmark_kind(kind: SettlementProjectKind) -> LandmarkKind {
    match kind {
        SettlementProjectKind::PublicGranary => LandmarkKind::Granary,
        SettlementProjectKind::WatchHouse => LandmarkKind::CouncilHall,
        SettlementProjectKind::MarketHall => LandmarkKind::Shop,
        SettlementProjectKind::ReliefHousing => LandmarkKind::Residence,
        SettlementProjectKind::CivicWorkshop => LandmarkKind::Smithy,
    }
}

fn project_cells(center: GridPos, phase: SettlementProjectPhase) -> Vec<(GridPos, TerrainCell)> {
    let mut cells = Vec::new();
    for dy in -2_i32..=2 {
        for dx in -2_i32..=2 {
            let boundary = dx.abs() == 2 || dy.abs() == 2;
            let entrance = dx == 0 && dy == 2;
            let terrain = match phase {
                SettlementProjectPhase::Planned => TerrainKind::Dirt,
                SettlementProjectPhase::Stalled => {
                    if (dx + dy).rem_euclid(3) == 0 {
                        TerrainKind::Rubble
                    } else {
                        TerrainKind::Dirt
                    }
                }
                SettlementProjectPhase::Foundation => {
                    if boundary {
                        TerrainKind::StoneFloor
                    } else {
                        TerrainKind::Dirt
                    }
                }
                SettlementProjectPhase::Structure => {
                    if boundary && !entrance && !(dy == 2 && dx > 0) {
                        TerrainKind::Wall
                    } else {
                        TerrainKind::StoneFloor
                    }
                }
                SettlementProjectPhase::Completed => {
                    if boundary && !entrance {
                        TerrainKind::Wall
                    } else {
                        TerrainKind::StoneFloor
                    }
                }
                SettlementProjectPhase::Damaged => {
                    if (dx, dy) == (-2, -2) || (dx, dy) == (2, -2) {
                        TerrainKind::Wall
                    } else {
                        TerrainKind::Rubble
                    }
                }
            };
            cells.push((center.offset(dx, dy, 0), TerrainCell::new(terrain)));
        }
    }
    cells
}

fn plan_encounter(
    world: &HistoricalWorld,
    related_event: EventId,
    transform: LayoutTransform,
) -> PlannedEncounter {
    let mut rng = RandomStream::new(world.campaign_seed, ENCOUNTER_STREAM);
    let kind = match rng.next_u64() % 3 {
        0 => EncounterKind::StarvingWolf,
        1 => EncounterKind::FeralBoar,
        _ => EncounterKind::AbandonedWarHound,
    };
    let (name, description) = match kind {
        EncounterKind::StarvingWolf => (
            "Starving Wolf",
            "A gaunt wolf has followed empty grain carts toward the settlement.",
        ),
        EncounterKind::FeralBoar => (
            "Feral Boar",
            "A hungry boar driven from neglected fields now charges travelers.",
        ),
        EncounterKind::AbandonedWarHound => (
            "Abandoned War Hound",
            "A scarred hound, left masterless during the emergency, guards the road.",
        ),
    };
    PlannedEncounter {
        entity: ENCOUNTER_ENTITY,
        kind,
        name: name.to_string(),
        description: description.to_string(),
        related_event,
        position: transform.apply(GridPos::new(-16, 0, 0)),
    }
}

fn plan_dungeon(
    world: &HistoricalWorld,
    related_event: EventId,
    law: LawKind,
    contact: PersonId,
    transform: LayoutTransform,
) -> Result<PlannedDungeon, SitePlanError> {
    let mut rng = RandomStream::new(world.campaign_seed, DUNGEON_STREAM);
    let crisis = &world.events()[&related_event];
    let strategic_item = world
        .significant_items()
        .values()
        .find(|item| {
            item.location == crisis.location
                && item
                    .provenance
                    .iter()
                    .any(|entry| entry.event == related_event)
        })
        .or_else(|| {
            world
                .significant_items()
                .values()
                .find(|item| item.location == crisis.location)
        })
        .ok_or(SitePlanError::MissingStrategicItem(related_event))?;
    let relic_name = strategic_item.name.as_str();
    let (name, boss_name, theme) = match law {
        LawKind::PropertySeizure => (
            "Seized Holdings Vault",
            "Forsaken Confiscator",
            "Property taken during the emergency was catalogued below the town and never fully returned.",
        ),
        LawKind::CompulsoryLabor => (
            "Levy Workings",
            "Last Levy Overseer",
            "Emergency laborers reopened older workings and left tools, records, and grievances in the dark.",
        ),
        LawKind::PriceControls => (
            "Buried Customs Archive",
            "Archive Hoarder",
            "Merchants hid stock and price records in older customs tunnels beneath the settlement.",
        ),
        LawKind::OpenGranaries => (
            "First Granary Undercroft",
            "Starved Storekeeper",
            "The public stores were expanded into forgotten foundation chambers during the shortage.",
        ),
        LawKind::Curfew => (
            "Night Watch Cells",
            "Forgotten Watch Captain",
            "The watch used old holding cells below the town while the emergency curfew was enforced.",
        ),
        LawKind::FoodRationing => (
            "Ration Archive Depths",
            "Last Ration Warden",
            "Ration records and reserve stores were moved into older chambers when the shortage was recognized.",
        ),
    };
    let enemy_names = [
        "Cellar Scavenger",
        "Dispossessed Delver",
        "Vault Vermin",
        "Record-Bound Wretch",
        "Feral Tunnel Hound",
    ];
    let mut levels = Vec::new();
    for depth in 1..=3u8 {
        let z = -(i16::from(depth));
        let entry = GridPos::new(-8, -6, z);
        let descent = (depth < 3).then_some(GridPos::new(8, 6, z));
        let mut enemies = Vec::new();
        let ordinary_count = if depth == 3 { 1 } else { 2 };
        for index in 0..ordinary_count {
            let entity = EntityId(DUNGEON_ENEMY_BASE + u64::from(depth) * 16 + index as u64);
            let name = enemy_names[(rng.next_u64() as usize) % enemy_names.len()].to_string();
            let position = match (depth, index) {
                (1, 0) => GridPos::new(-2, -2, z),
                (1, _) => GridPos::new(5, 3, z),
                (2, 0) => GridPos::new(-4, 3, z),
                (2, _) => GridPos::new(4, -3, z),
                _ => GridPos::new(0, 1, z),
            };
            enemies.push(PlannedDungeonEnemy {
                entity,
                name,
                position,
                health: 5 + i16::from(depth) * 2,
                armor: i16::from(depth.saturating_sub(1)),
                experience: 25 + u32::from(depth) * 10,
            });
        }
        if depth == 3 {
            enemies.push(PlannedDungeonEnemy {
                entity: DUNGEON_BOSS,
                name: boss_name.to_string(),
                position: GridPos::new(7, 4, z),
                health: 14,
                armor: 2,
                experience: 90,
            });
        }
        let (level_name, historical_context) = match depth {
            1 => (
                format!("{name}: Emergency Cellars"),
                format!(
                    "Cut and reused during the year {} crisis. {}",
                    crisis.date.year, theme
                ),
            ),
            2 => (
                format!("{name}: Pre-Council Workings"),
                format!(
                    "These tunnels predate the current emergency institutions of {}.",
                    world.sites()[&crisis.location].name
                ),
            ),
            _ => (
                format!("{name}: Founders' Vault"),
                format!(
                    "The deepest masonry belongs to the settlement's first common stores; the year {} emergency records were hidden here later.",
                    crisis.date.year
                ),
            ),
        };
        levels.push(PlannedDungeonLevel {
            depth,
            name: level_name,
            historical_context,
            entry,
            descent,
            enemies,
        });
    }
    let entrance = transform.apply(GridPos::new(-32, -3, 0));
    let formula = strategic_item
        .inscribed_formula
        .and_then(|formula| world.rules().formula(formula));
    let reagents = formula
        .into_iter()
        .flat_map(|formula| formula.reagents.iter().copied())
        .enumerate()
        .map(|(index, material)| Item {
            id: ItemId(DUNGEON_REAGENT_BASE + index as u64),
            name: title_case_material(material),
            kind: ItemKind::Reagent { material },
            quantity: 2,
            weight_grams: reagent_weight(material),
            quality: 55,
        })
        .collect();
    Ok(PlannedDungeon {
        name: name.to_string(),
        description: format!(
            "{} The entrance was sealed after the year {} crisis.",
            theme, crisis.date.year
        ),
        related_event,
        world_item: strategic_item.id,
        entrance,
        levels,
        boss: DUNGEON_BOSS,
        boss_name: boss_name.to_string(),
        relic: Item {
            id: DUNGEON_RELIC,
            name: relic_name.to_string(),
            kind: strategic_item
                .inscribed_formula
                .map_or(ItemKind::Artifact, |formula| ItemKind::InscribedArtifact {
                    object: strategic_item.object,
                    formula,
                }),
            quantity: 1,
            weight_grams: 450,
            quality: 70,
        },
        reagents,
        quest: DUNGEON_QUEST,
        quest_title: format!("The {relic_name}"),
        quest_description: format!(
            "{} believes the {relic_name} can prove what happened during the year {} crisis. Descend through {name}, recover it, and return.",
            person_name(world, contact),
            crisis.date.year
        ),
    })
}

fn title_case_material(material: MaterialKind) -> String {
    let mut name = material.name().to_string();
    if let Some(first) = name.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    name
}

const fn reagent_weight(material: MaterialKind) -> u32 {
    match material.source() {
        ultimate_fate_content::MaterialSource::Mineral => 120,
        ultimate_fate_content::MaterialSource::Timber => 180,
        ultimate_fate_content::MaterialSource::Crafted => 80,
        ultimate_fate_content::MaterialSource::Plant => 35,
        ultimate_fate_content::MaterialSource::River => 60,
    }
}

fn dungeon_transitions(map: MapId, dungeon: &PlannedDungeon) -> Vec<Transition> {
    let mut transitions = Vec::new();
    let entrance = WorldPosition {
        map,
        grid: dungeon.entrance,
    };
    for (index, level) in dungeon.levels.iter().enumerate() {
        let entry = WorldPosition {
            map,
            grid: level.entry,
        };
        let upper = if index == 0 {
            entrance
        } else {
            WorldPosition {
                map,
                grid: dungeon.levels[index - 1]
                    .descent
                    .expect("non-final upper dungeon level has descent"),
            }
        };
        transitions.push(Transition {
            from: upper,
            to: entry,
            kind: TransitionKind::Descend,
            name: format!("Descend into {}", level.name),
        });
        transitions.push(Transition {
            from: entry,
            to: upper,
            kind: TransitionKind::Ascend,
            name: if index == 0 {
                format!("Return to {}", dungeon.name)
            } else {
                format!("Ascend toward {}", dungeon.levels[index - 1].name)
            },
        });
    }
    transitions
}

fn paint_dungeon(map: &mut WorldMap, campaign_seed: u64, dungeon: &PlannedDungeon) {
    let mut rng = RandomStream::new(campaign_seed, DUNGEON_STREAM);
    for level in &dungeon.levels {
        let z = -(i16::from(level.depth));
        for y in -8..=8 {
            for x in -10..=10 {
                let boundary = x == -10 || x == 10 || y == -8 || y == 8;
                let terrain = if boundary {
                    TerrainKind::Wall
                } else {
                    TerrainKind::StoneFloor
                };
                map.set_cell(GridPos::new(x, y, z), TerrainCell::new(terrain));
            }
        }
        let first_gap = -4 + (rng.next_u64() % 5) as i32;
        let second_gap = (rng.next_u64() % 5) as i32;
        for y in -7..=7 {
            if y != first_gap && y != first_gap + 1 {
                map.set_cell(GridPos::new(-2, y, z), TerrainCell::new(TerrainKind::Wall));
            }
            if y != second_gap && y != second_gap + 1 {
                map.set_cell(GridPos::new(3, y, z), TerrainCell::new(TerrainKind::Wall));
            }
        }
        map.set_cell(level.entry, TerrainCell::new(TerrainKind::StairsUp));
        if let Some(descent) = level.descent {
            map.set_cell(descent, TerrainCell::new(TerrainKind::StairsDown));
        }
        for enemy in &level.enemies {
            map.set_cell(enemy.position, TerrainCell::new(TerrainKind::StoneFloor));
        }
    }
}

fn starter_items() -> [Item; 3] {
    [
        Item {
            id: HUNTING_BOW,
            name: "Worn Hunting Bow".to_string(),
            kind: ItemKind::RangedWeapon {
                damage: 3,
                range: 7,
                ammunition: AmmunitionKind::Arrow,
            },
            quantity: 1,
            weight_grams: 900,
            quality: 38,
        },
        Item {
            id: ARROWS,
            name: "Hunting Arrows".to_string(),
            kind: ItemKind::Ammunition {
                kind: AmmunitionKind::Arrow,
            },
            quantity: 5,
            weight_grams: 250,
            quality: 35,
        },
        Item {
            id: FIELD_DRESSING,
            name: "Linen Field Dressing".to_string(),
            kind: ItemKind::Consumable { healing: 5 },
            quantity: 5,
            weight_grams: 100,
            quality: 45,
        },
    ]
}

fn plan_aid_situation(
    world: &HistoricalWorld,
    site: SiteId,
    cause: EventId,
    residents: &[PlannedResident],
) -> Result<PlannedAidSituation, SitePlanError> {
    let custodian = residents
        .iter()
        .filter(|resident| resident.occupation == Occupation::Healer)
        .max_by_key(|resident| {
            (
                drive_value(world, resident.person, Drive::Family)
                    + drive_value(world, resident.person, Drive::Faith),
                std::cmp::Reverse(resident.person),
            )
        })
        .or_else(|| residents.first())
        .ok_or(SitePlanError::InsufficientAidActors)?;
    let patient = residents
        .iter()
        .filter(|resident| resident.person != custodian.person)
        .max_by_key(|resident| {
            (
                drive_value(world, resident.person, Drive::Survival),
                std::cmp::Reverse(resident.person),
            )
        })
        .ok_or(SitePlanError::InsufficientAidActors)?;
    let advocate = residents
        .iter()
        .filter(|resident| resident.person != custodian.person && resident.person != patient.person)
        .max_by_key(|resident| {
            (
                resident.faction != custodian.faction,
                drive_value(world, resident.person, Drive::Justice)
                    + drive_value(world, resident.person, Drive::Loyalty),
                std::cmp::Reverse(resident.person),
            )
        })
        .ok_or(SitePlanError::InsufficientAidActors)?;
    let restricting_law = world.sites()[&site]
        .laws
        .values()
        .filter(|law| law.active)
        .min_by_key(|law| law.id);
    let custodian_person = &world.people()[&custodian.person];
    let surname = &world.families()[&custodian_person.family].surname;
    let medicine = Item {
        id: ACCESS_MEDICINE,
        name: format!("{surname} Fever Tonic"),
        kind: ItemKind::Consumable { healing: 8 },
        quantity: 1,
        weight_grams: 180,
        quality: 58,
    };
    let restriction = restricting_law.map_or_else(
        || "The custodian is unwilling to release the settlement's last prepared dose.".to_string(),
        |law| {
            format!(
                "{} places emergency stores under the authority of {}.",
                law_kind_name(law.kind),
                world.factions()[&law.authority].name
            )
        },
    );
    let historical_cause = world
        .events()
        .get(&cause)
        .map(|event| event.summary.as_str())
        .unwrap_or("An earlier crisis depleted local supplies.");

    Ok(PlannedAidSituation {
        cause,
        patient: patient.person,
        patient_entity: patient.entity,
        patient_name: patient.name.clone(),
        custodian: custodian.person,
        custodian_entity: custodian.entity,
        custodian_name: custodian.name.clone(),
        advocate: advocate.person,
        advocate_entity: advocate.entity,
        advocate_name: advocate.name.clone(),
        restricting_law: restricting_law.map(|law| law.id),
        medicine,
        price: 18,
        title: format!("Medicine for {}", patient.name),
        description: format!(
            "{} needs treatment held by {}. {} {} has offered to argue for access. Cause: {}",
            patient.name, custodian.name, restriction, advocate.name, historical_cause
        ),
    })
}

fn drive_value(world: &HistoricalWorld, person: PersonId, drive: Drive) -> u16 {
    u16::from(
        world.people()[&person]
            .drives
            .get(&drive)
            .copied()
            .unwrap_or_default(),
    )
}

fn law_kind_name(kind: LawKind) -> &'static str {
    match kind {
        LawKind::FoodRationing => "Food rationing",
        LawKind::PriceControls => "Price controls",
        LawKind::CompulsoryLabor => "Compulsory labor",
        LawKind::PropertySeizure => "Property seizure",
        LawKind::Curfew => "The curfew",
        LawKind::OpenGranaries => "The open-granary decree",
    }
}

fn plan_residents(
    world: &HistoricalWorld,
    site: SiteId,
    contact: PersonId,
    contact_anchor: GridPos,
    locations: &[PlannedLocation],
    transform: LayoutTransform,
) -> Vec<PlannedResident> {
    let faction_seats = locations
        .iter()
        .filter_map(|location| match location.source {
            LocationSource::FactionSeat(faction) => Some((faction, location.position)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    for person in std::iter::once(contact)
        .chain(
            world
                .living_people()
                .filter(|person| person.home == site && person.occupation == Occupation::Healer)
                .map(|person| person.id),
        )
        .chain(world.factions().values().map(|faction| faction.leader))
        .chain(
            world
                .living_people()
                .filter(|person| person.home == site)
                .map(|person| person.id),
        )
    {
        if seen.insert(person) {
            selected.push(person);
        }
        if selected.len() >= 12 {
            break;
        }
    }

    let fallback_positions = [
        (-2, 2),
        (2, -2),
        (-1, -4),
        (4, 1),
        (-4, -1),
        (1, 4),
        (-11, 0),
        (9, 0),
        (0, -14),
        (0, 14),
        (18, 1),
        (-24, 0),
    ];
    let home_positions = [
        (-20, -20),
        (-14, -20),
        (-8, -20),
        (-2, -20),
        (4, -20),
        (10, -20),
        (20, -14),
        (20, -8),
        (20, 8),
        (20, 14),
        (10, 20),
        (4, 20),
    ];
    let mut occupied = BTreeSet::new();
    selected
        .into_iter()
        .enumerate()
        .map(|(index, person_id)| {
            let person = &world.people()[&person_id];
            let preferred = if person_id == contact {
                contact_anchor
            } else {
                faction_seats
                    .get(&person.faction)
                    .copied()
                    .filter(|_| world.factions()[&person.faction].leader == person_id)
                    .unwrap_or_else(|| {
                        transform.apply(GridPos::new(
                            fallback_positions[index % fallback_positions.len()].0,
                            fallback_positions[index % fallback_positions.len()].1,
                            0,
                        ))
                    })
            };
            let position = first_free_position(preferred, &mut occupied);
            let home_position = first_free_position(
                transform.apply(GridPos::new(
                    home_positions[index % home_positions.len()].0,
                    home_positions[index % home_positions.len()].1,
                    0,
                )),
                &mut occupied,
            );
            PlannedResident {
                person: person_id,
                entity: EntityId(person_id.0 + 1),
                name: person_name(world, person_id),
                occupation: person.occupation,
                faction: person.faction,
                position,
                home_position,
            }
        })
        .collect()
}

fn resident_social_position(transform: LayoutTransform, index: usize) -> GridPos {
    const SOCIAL_POSITIONS: [(i32, i32); 12] = [
        (-2, -2),
        (0, -2),
        (2, -2),
        (-2, 0),
        (0, 0),
        (2, 0),
        (-2, 2),
        (0, 2),
        (2, 2),
        (-3, 0),
        (3, 0),
        (0, 3),
    ];
    let (x, y) = SOCIAL_POSITIONS[index % SOCIAL_POSITIONS.len()];
    transform.apply(GridPos::new(x, y, 0))
}

fn resident_service_position(anchor: GridPos, index: usize) -> GridPos {
    const OFFSETS: [(i32, i32); 9] = [
        (0, 0),
        (-1, 0),
        (1, 0),
        (0, -1),
        (0, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
        (1, 1),
    ];
    let (x, y) = OFFSETS[index % OFFSETS.len()];
    anchor.offset(x, y, 0)
}

fn first_free_position(preferred: GridPos, occupied: &mut BTreeSet<GridPos>) -> GridPos {
    for radius in 0..=4 {
        for (dx, dy) in [
            (radius, 0),
            (0, radius),
            (-radius, 0),
            (0, -radius),
            (radius, radius),
        ] {
            let candidate = preferred.offset(dx, dy, 0);
            if occupied.insert(candidate) {
                return candidate;
            }
        }
    }
    unreachable!("resident placement search always includes free positions")
}

fn next_resident_step(
    simulation: &Simulation,
    map_id: MapId,
    start: GridPos,
    target: GridPos,
    occupied: &BTreeSet<GridPos>,
) -> Option<GridPos> {
    let map = simulation.map(map_id)?;
    let mut queue = VecDeque::from([start]);
    let mut previous = BTreeMap::<GridPos, GridPos>::new();
    let mut seen = BTreeSet::from([start]);
    while let Some(current) = queue.pop_front() {
        if current == target {
            let mut step = current;
            while previous.get(&step).is_some_and(|parent| *parent != start) {
                step = previous[&step];
            }
            return (step != start).then_some(step);
        }
        for direction in Direction::ALL {
            let (dx, dy) = direction.delta();
            let neighbor = current.offset(dx, dy, 0);
            if seen.contains(&neighbor)
                || occupied.contains(&neighbor)
                || map.cell(neighbor).is_none_or(|cell| cell.movement_blocked)
            {
                continue;
            }
            seen.insert(neighbor);
            previous.insert(neighbor, current);
            queue.push_back(neighbor);
        }
    }
    None
}

fn person_name(world: &HistoricalWorld, person: PersonId) -> String {
    let person = &world.people()[&person];
    format!(
        "{} {}",
        person.given_name,
        world.families()[&person.family].surname
    )
}

fn workplace_name(world: &HistoricalWorld, person: PersonId, town: &str) -> String {
    let person = &world.people()[&person];
    let surname = &world.families()[&person.family].surname;
    match person.occupation {
        Occupation::Merchant => format!("{surname} Trading House"),
        Occupation::Innkeeper => format!("The {town} Wayfarer"),
        Occupation::Healer => format!("{surname} Infirmary"),
        Occupation::Priest => format!("{town} Shrine"),
        Occupation::Smith => format!("{surname} Smithy"),
        Occupation::Farmer => format!("{surname} Farm"),
        Occupation::Miller => format!("{surname} Mill"),
        Occupation::Guard => format!("{town} Watch House"),
        Occupation::Official => format!("{town} Records Office"),
        Occupation::Laborer => format!("{surname} House"),
    }
}

fn workplace_kind(occupation: Occupation) -> LandmarkKind {
    match occupation {
        Occupation::Merchant => LandmarkKind::Shop,
        Occupation::Innkeeper => LandmarkKind::Inn,
        Occupation::Healer => LandmarkKind::Infirmary,
        Occupation::Priest => LandmarkKind::Temple,
        Occupation::Smith => LandmarkKind::Smithy,
        Occupation::Farmer => LandmarkKind::Farm,
        Occupation::Miller => LandmarkKind::Mill,
        Occupation::Guard | Occupation::Official => LandmarkKind::CouncilHall,
        Occupation::Laborer => LandmarkKind::Residence,
    }
}

fn evidence_kind(kind: PhysicalEvidenceKind) -> LandmarkKind {
    match kind {
        PhysicalEvidenceKind::PublicGranary => LandmarkKind::Granary,
        PhysicalEvidenceKind::Fortification => LandmarkKind::Ruin,
        PhysicalEvidenceKind::RefugeeDistrict => LandmarkKind::Residence,
        PhysicalEvidenceKind::AbandonedFarm => LandmarkKind::Farm,
        PhysicalEvidenceKind::Grave | PhysicalEvidenceKind::Memorial => LandmarkKind::Memorial,
        PhysicalEvidenceKind::BurnedBuilding => LandmarkKind::Ruin,
    }
}

fn evidence_title(kind: PhysicalEvidenceKind) -> String {
    let name = physical_evidence_name(kind)
        .strip_prefix("the ")
        .unwrap_or(physical_evidence_name(kind));
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Historical Site".to_string(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LayoutTransform {
    quarter_turns: u8,
    mirror_x: bool,
}

impl LayoutTransform {
    fn from_seed(seed: u64) -> Self {
        let mut rng = RandomStream::new(seed, SITE_LAYOUT_STREAM);
        Self {
            quarter_turns: (rng.next_u64() % 4) as u8,
            mirror_x: rng.next_u64().is_multiple_of(2),
        }
    }

    fn apply(self, mut position: GridPos) -> GridPos {
        if self.mirror_x {
            position.x = -position.x;
        }
        for _ in 0..self.quarter_turns {
            (position.x, position.y) = (-position.y, position.x);
        }
        position
    }
}

fn set_transformed(
    map: &mut WorldMap,
    transform: LayoutTransform,
    x: i32,
    y: i32,
    cell: TerrainCell,
) {
    map.set_cell(transform.apply(GridPos::new(x, y, 0)), cell);
}

fn paint_rect(
    map: &mut WorldMap,
    transform: LayoutTransform,
    bounds: (i32, i32, i32, i32),
    cell: TerrainCell,
) {
    let (min_x, min_y, max_x, max_y) = bounds;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            set_transformed(map, transform, x, y, cell);
        }
    }
}

fn paint_building(
    map: &mut WorldMap,
    transform: LayoutTransform,
    bounds: (i32, i32, i32, i32),
    entrance: (i32, i32),
) {
    let (min_x, min_y, max_x, max_y) = bounds;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let boundary = x == min_x || x == max_x || y == min_y || y == max_y;
            let terrain = if (x, y) == entrance {
                TerrainKind::Road
            } else if boundary {
                TerrainKind::Wall
            } else {
                TerrainKind::StoneFloor
            };
            set_transformed(map, transform, x, y, TerrainCell::new(terrain));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use ultimate_fate_history::HistoryEngine;

    fn plan(seed: u64) -> PlayableSitePlan {
        let mut history = HistoryEngine::seeded_town(seed).expect("history");
        history.simulate_years(20).expect("simulation");
        let start = CampaignStart::for_outsider(history.world(), history.primary_site())
            .expect("campaign start");
        history
            .begin_living_simulation()
            .expect("living simulation");
        PlayableSitePlan::from_history(history.world(), &start).expect("site plan")
    }

    #[test]
    fn plan_binds_contact_and_evidence_from_history() {
        let plan = plan(0x55aa_2026);
        assert_eq!(plan.contact_resident().person, plan.contact);
        assert_eq!(
            plan.evidence_location().source,
            LocationSource::HistoricalEvidence(plan.evidence_event)
        );
        assert!(plan.first_objective().contains(&plan.contact_name));
        assert!(plan.first_objective().contains(&plan.contact_location));
        assert_eq!(plan.encounter.related_event, plan.crisis_event);
        assert!(plan.starter_sword_provenance.contains("year"));
        let simulation = plan.build_simulation().expect("simulation");
        assert!(
            simulation
                .combatant(plan.encounter.entity)
                .is_some_and(|combatant| combatant.hostile_to_player)
        );
        assert_eq!(
            simulation
                .player_inventory()
                .and_then(|inventory| inventory.equipped_ranged),
            Some(plan.hunting_bow)
        );
        assert!(
            simulation
                .inventory(plan.contact_resident().entity)
                .is_some_and(|inventory| inventory.items.contains(&plan.starter_sword.id))
        );
    }

    #[test]
    fn generated_aid_situations_bind_three_people_law_and_real_medicine() {
        for seed in 0..64 {
            let plan = plan(seed);
            let actors = BTreeSet::from([plan.aid.patient, plan.aid.custodian, plan.aid.advocate]);
            assert_eq!(actors.len(), 3, "seed {seed}");
            assert!(plan.aid.description.contains(&plan.aid.patient_name));
            assert!(plan.aid.description.contains(&plan.aid.custodian_name));
            assert!(plan.aid.description.contains(&plan.aid.advocate_name));
            let simulation = plan.build_simulation().expect("simulation");
            assert_eq!(
                simulation.legal_owner(plan.aid.medicine.id),
                Some(plan.aid.custodian_entity),
                "seed {seed}"
            );
            assert!(
                simulation
                    .inventory(plan.aid.custodian_entity)
                    .is_some_and(|inventory| inventory.items.contains(&plan.aid.medicine.id)),
                "seed {seed}"
            );
            assert!(
                simulation
                    .inventory(plan.aid.patient_entity)
                    .is_none_or(|inventory| !inventory.items.contains(&plan.aid.medicine.id)),
                "seed {seed}"
            );
        }
    }

    #[test]
    fn physical_overworld_materializes_biomes_rivers_seas_and_real_roads() {
        let plan = plan(0x55aa_2026);
        let simulation = plan.build_simulation().expect("simulation");
        let map = simulation.map(plan.regional_map).expect("regional map");
        let terrains = map
            .cells()
            .map(|(_, cell)| cell.terrain)
            .collect::<Vec<_>>();

        assert_eq!(map.cells().count(), 65_536);
        assert!(terrains.contains(&TerrainKind::Ocean));
        assert!(terrains.contains(&TerrainKind::Water));
        assert!(terrains.contains(&TerrainKind::Mountain));
        assert!(terrains.contains(&TerrainKind::Forest));
        assert!(plan.regional_routes.iter().all(|route| {
            route
                .path
                .windows(2)
                .all(|step| (step[0].x - step[1].x).abs() + (step[0].y - step[1].y).abs() == 1)
        }));
        assert!(
            plan.regional_routes
                .iter()
                .flat_map(|route| &route.path)
                .all(|position| {
                    map.cell(*position).is_some_and(|cell| {
                        !matches!(cell.terrain, TerrainKind::Ocean | TerrainKind::Mountain)
                    })
                })
        );
        assert!(
            !plan.regional_history_sites.is_empty(),
            "twenty years of route conflict should leave inspectable map scars"
        );
        assert!(plan.regional_history_sites.iter().all(|site| {
            map.cell(site.position)
                .is_some_and(|cell| cell.terrain == TerrainKind::Dirt)
                && simulation.landmarks().any(|landmark| {
                    landmark.position.map == plan.regional_map
                        && landmark.position.grid == site.position
                        && landmark.name == "Old raid site"
                })
        }));
    }

    #[test]
    fn every_visible_road_exit_enters_the_regional_map_on_collision() {
        let plan = plan(0x55aa_2026);
        for gate in plan.regional_gates() {
            let mut simulation = plan.build_simulation().expect("simulation");
            let approach = GridPos::new(gate.x - gate.x.signum(), gate.y - gate.y.signum(), gate.z);
            assert!(simulation.move_entity(
                PLAYER_ENTITY,
                WorldPosition {
                    map: plan.map,
                    grid: approach,
                },
            ));
            let direction = if gate.x > 0 {
                Direction::East
            } else if gate.x < 0 {
                Direction::West
            } else if gate.y > 0 {
                Direction::South
            } else {
                Direction::North
            };

            let outcome = simulation.apply_command(GameCommand::Move(direction));

            assert!(outcome.changed_world);
            assert_eq!(simulation.player().position.map, plan.regional_map);
        }
    }

    #[test]
    fn many_seeds_create_coherent_playable_sites() {
        for seed in 0..64 {
            let plan = plan(seed);
            let simulation = plan.build_simulation().expect("simulation");
            assert_eq!(simulation.player().position.grid, plan.player_spawn);
            assert!(
                plan.residents
                    .iter()
                    .all(|resident| resident.position != plan.player_spawn)
            );
            assert!(simulation.landmarks().count() >= 5);
            let map = simulation.map(plan.map).expect("planned map");
            assert!(plan.residents.iter().all(|resident| {
                map.cell(resident.position)
                    .is_some_and(|cell| !cell.movement_blocked)
                    && map
                        .cell(resident.home_position)
                        .is_some_and(|cell| !cell.movement_blocked)
            }));
            assert!(is_reachable(&simulation, plan.contact_resident().position));
            assert!(is_reachable(&simulation, plan.evidence_location().position));
            assert!(is_reachable(&simulation, plan.encounter.position));
        }
    }

    #[test]
    fn residents_follow_deterministic_work_leisure_and_home_schedules() {
        let plan = plan(3);
        let mut first = plan.build_simulation().expect("simulation");
        let mut replay = plan.build_simulation().expect("simulation");
        let initial = plan
            .residents
            .iter()
            .map(|resident| {
                (
                    resident.entity,
                    first.entity(resident.entity).expect("resident").position,
                )
            })
            .collect::<BTreeMap<_, _>>();

        assert_eq!(
            plan.resident_activity(plan.contact, 0)
                .map(|activity| activity.0),
            Some(ResidentActivity::Working)
        );
        assert_eq!(
            plan.resident_activity(plan.contact, 6)
                .map(|activity| activity.0),
            Some(ResidentActivity::AtLeisure)
        );
        for tick in 6..=86 {
            plan.advance_resident_schedules(tick, &mut first);
            plan.advance_resident_schedules(tick, &mut replay);
        }

        assert!(plan.residents.iter().any(|resident| {
            first.entity(resident.entity).expect("resident").position != initial[&resident.entity]
        }));
        assert!(plan.residents.iter().all(|resident| {
            first.entity(resident.entity).expect("resident").position
                == replay.entity(resident.entity).expect("resident").position
        }));
        let positions = plan
            .residents
            .iter()
            .map(|resident| first.entity(resident.entity).expect("resident").position)
            .collect::<BTreeSet<_>>();
        assert_eq!(positions.len(), plan.residents.len());
        assert_eq!(
            plan.resident_activity(plan.contact, 36)
                .map(|activity| activity.0),
            Some(ResidentActivity::AtLeisure)
        );
        assert_eq!(
            plan.resident_activity(plan.contact, 76)
                .map(|activity| activity.0),
            Some(ResidentActivity::AtHome)
        );
        assert_eq!(
            plan.resident_activity(plan.contact, LOCAL_DAY_TURNS * 6 - LOCAL_DAY_PHASE_OFFSET,)
                .map(|activity| activity.0),
            Some(ResidentActivity::AtHome)
        );
    }

    #[test]
    fn different_seeds_change_history_bound_names_or_layout() {
        let first = plan(1);
        let second = plan(2);
        assert!(
            first.town_name != second.town_name
                || first.contact_name != second.contact_name
                || first.player_spawn != second.player_spawn
        );
    }

    #[test]
    fn persistent_regional_parties_move_on_the_materialized_road() {
        let seed = 0x55aa;
        let mut history = HistoryEngine::seeded_town(seed).expect("history");
        history.simulate_years(20).expect("simulation");
        let start = CampaignStart::for_outsider(history.world(), history.primary_site())
            .expect("campaign start");
        history
            .begin_living_simulation()
            .expect("living simulation");
        let party = (0..96)
            .find_map(|_| {
                history.advance_month().expect("month should advance");
                history
                    .world()
                    .regional_parties()
                    .values()
                    .find(|party| {
                        party.status == RegionalPartyStatus::Traveling
                            && matches!(party.kind, RegionalPartyKind::TradeCaravan { .. })
                    })
                    .map(|party| party.id)
            })
            .expect("regional economy should dispatch a caravan");
        let plan =
            PlayableSitePlan::from_history(history.world(), &start).expect("playable site plan");
        let mut simulation = plan.build_simulation().expect("simulation");
        assert!(plan.synchronize_regional_parties(history.world(), &mut simulation) >= 1);
        let entity = PlayableSitePlan::regional_party_entity(party);
        let before = simulation.entity(entity).expect("party entity").position;

        history
            .advance_regional_parties(400)
            .expect("party movement");
        plan.synchronize_regional_parties(history.world(), &mut simulation);
        let after = simulation.entity(entity).expect("moving party").position;

        assert_eq!(before.map, plan.regional_map);
        assert_eq!(after.map, plan.regional_map);
        assert_ne!(before.grid, after.grid);
        assert!(
            simulation
                .map(plan.regional_map)
                .and_then(|map| map.cell(after.grid))
                .is_some_and(|cell| cell.terrain == TerrainKind::Road)
        );
    }

    #[test]
    fn history_creates_a_connected_multi_level_dungeon_and_relic_quest() {
        let seed = 0x55aa_2026;
        let plan = plan(seed);
        let mut history = HistoryEngine::seeded_town(seed).expect("history");
        history.simulate_years(20).expect("simulation");
        history
            .begin_living_simulation()
            .expect("living simulation");
        let world_item = &history.world().significant_items()[&plan.dungeon.world_item];
        let simulation = plan.build_simulation().expect("simulation");
        let depths = plan
            .dungeon
            .levels
            .iter()
            .map(|level| level.depth)
            .collect::<Vec<_>>();

        assert_eq!(depths, vec![1, 2, 3]);
        assert_eq!(plan.dungeon.related_event, plan.crisis_event);
        assert_eq!(plan.dungeon.relic.name, world_item.name);
        assert!(matches!(
            plan.dungeon.relic.kind,
            ItemKind::InscribedArtifact {
                object,
                formula,
            } if object == world_item.object && Some(formula) == world_item.inscribed_formula
        ));
        let formula = history
            .world()
            .rules()
            .formula(world_item.inscribed_formula.expect("formula"))
            .expect("world formula");
        assert_eq!(plan.dungeon.reagents.len(), formula.reagents.len());
        assert!(formula.reagents.iter().all(|material| {
            plan.dungeon.reagents.iter().any(|item| {
                item.kind
                    == ItemKind::Reagent {
                        material: *material,
                    }
            })
        }));
        assert!(
            world_item
                .provenance
                .iter()
                .any(|entry| entry.event == plan.crisis_event)
        );
        let entrance_distance = (plan.dungeon.entrance.x - plan.player_spawn.x).abs()
            + (plan.dungeon.entrance.y - plan.player_spawn.y).abs();
        assert!(
            entrance_distance <= 16,
            "dungeon entrance should be visible near the opening area"
        );
        assert_eq!(simulation.transitions().count(), 11);
        assert!(simulation.map(plan.regional_map).is_some());
        for gate in plan.regional_gates() {
            assert!(simulation.landmarks().any(|landmark| {
                landmark.kind == LandmarkKind::Gate
                    && landmark.position
                        == WorldPosition {
                            map: plan.map,
                            grid: gate,
                        }
            }));
            assert!(
                simulation
                    .transition_at(WorldPosition {
                        map: plan.map,
                        grid: gate,
                    })
                    .is_some()
            );
        }
        assert_eq!(
            plan.regional_sites.len(),
            history.world().regional_settlements().len()
        );
        assert!(plan.regional_sites.iter().all(|site| {
            simulation
                .map(plan.regional_map)
                .and_then(|map| map.cell(site.position))
                .is_some_and(|cell| !cell.movement_blocked)
        }));
        assert_eq!(
            simulation
                .quest(plan.dungeon.quest)
                .map(|quest| quest.status),
            Some(QuestStatus::Active)
        );
        assert!(
            simulation
                .inventory(plan.dungeon.boss)
                .is_some_and(|inventory| inventory.items.contains(&plan.dungeon.relic.id))
        );

        let map = simulation.map(plan.map).expect("planned map");
        assert_eq!(
            map.cell(plan.dungeon.entrance).map(|cell| cell.terrain),
            Some(TerrainKind::StairsDown)
        );
        assert!(
            (-3_i32..=3)
                .flat_map(|dy| (-3_i32..=3).map(move |dx| (dx, dy)))
                .filter(|(dx, dy)| dx.abs() < 3 || dy.abs() < 3)
                .all(|(dx, dy)| map
                    .cell(plan.dungeon.entrance.offset(dx, dy, 0))
                    .is_some_and(|cell| !cell.movement_blocked)),
            "the visible entrance ruin should remain approachable"
        );
        for transition in simulation.transitions() {
            for position in [transition.from, transition.to] {
                assert!(
                    map.cell(position.grid)
                        .is_some_and(|cell| !cell.movement_blocked),
                    "transition endpoint should be passable: {:?}",
                    position.grid
                );
            }
        }
        for level in &plan.dungeon.levels {
            for target in level
                .enemies
                .iter()
                .map(|enemy| enemy.position)
                .chain(level.descent)
            {
                assert!(
                    is_reachable_from(&simulation, level.entry, target),
                    "{} should connect {:?} to {:?}",
                    level.name,
                    level.entry,
                    target
                );
            }
        }
    }

    #[test]
    fn living_project_phases_rewrite_the_semantic_map() {
        let mut history = HistoryEngine::seeded_town(0x55aa_2026).expect("history");
        history.simulate_years(20).expect("simulation");
        let start = CampaignStart::for_outsider(history.world(), history.primary_site())
            .expect("campaign start");
        let project = history
            .begin_living_simulation()
            .expect("living simulation");
        let plan = PlayableSitePlan::from_history(history.world(), &start).expect("site plan");
        let planned = plan
            .living_projects
            .iter()
            .find(|planned| planned.project == project)
            .expect("project site");
        let center = planned.position;
        let edge = center.offset(-2, 0, 0);
        let mut simulation = plan.build_simulation().expect("simulation");

        assert_eq!(
            simulation
                .map(plan.map)
                .and_then(|map| map.cell(center))
                .map(|cell| cell.terrain),
            Some(TerrainKind::Dirt)
        );
        history.advance_month().expect("construction starts");
        plan.synchronize_living_projects(history.world(), &mut simulation);
        assert_eq!(
            simulation
                .map(plan.map)
                .and_then(|map| map.cell(edge))
                .map(|cell| cell.terrain),
            Some(TerrainKind::StoneFloor)
        );

        while history.world().projects()[&project].phase != SettlementProjectPhase::Completed {
            history.advance_month().expect("construction advances");
        }
        plan.synchronize_living_projects(history.world(), &mut simulation);
        assert_eq!(
            simulation
                .map(plan.map)
                .and_then(|map| map.cell(edge))
                .map(|cell| cell.terrain),
            Some(TerrainKind::Wall)
        );

        for _ in 0..8 {
            if history.world().projects()[&project].phase == SettlementProjectPhase::Damaged {
                break;
            }
            history.advance_month().expect("living world advances");
        }
        assert_eq!(
            history.world().projects()[&project].phase,
            SettlementProjectPhase::Damaged
        );
        plan.synchronize_living_projects(history.world(), &mut simulation);
        assert_eq!(
            simulation
                .map(plan.map)
                .and_then(|map| map.cell(center))
                .map(|cell| cell.terrain),
            Some(TerrainKind::Rubble)
        );
    }

    fn is_reachable(simulation: &Simulation, target: GridPos) -> bool {
        is_reachable_from(simulation, simulation.player().position.grid, target)
    }

    fn is_reachable_from(simulation: &Simulation, start: GridPos, target: GridPos) -> bool {
        let map = simulation
            .map(simulation.player().position.map)
            .expect("player map");
        let mut frontier = VecDeque::from([start]);
        let mut visited = BTreeSet::from([start]);
        while let Some(position) = frontier.pop_front() {
            if position == target {
                return true;
            }
            for direction in Direction::ALL {
                let (dx, dy) = direction.delta();
                let next = position.offset(dx, dy, 0);
                if visited.insert(next) && map.cell(next).is_some_and(|cell| !cell.movement_blocked)
                {
                    frontier.push_back(next);
                }
            }
        }
        false
    }
}
