//! Authoritative campaign orchestration shared by every platform and test host.
//!
//! Local simulation, historical state, player knowledge, and calendar advancement
//! are committed here as one transaction boundary. Platform clients translate
//! device input into semantic commands and render the resulting state; they do not
//! decide what becomes history.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use ultimate_fate_content::FormulaId;
use ultimate_fate_core::{
    CommandOutcome, Direction, EntityId, GameCommand, ItemId, ItemKind, QuestId, Simulation,
    SimulationEvent,
};
use ultimate_fate_history::{
    AidResolutionKind, ClaimId, CrisisResolutionKind, CrisisResolutionOutcome, Drive, EventId,
    FactionId, GoalId, HistoricalEventKind, HistoryEngine, LawId, MonthSummary, PartyId, PersonId,
    RegionalGoalApproach, RegionalGoalOutcome, RegionalGoalStatus, ResourceKind, WorldItemId,
};
use ultimate_fate_text::{
    CampaignStart, Conversation, ConversationContext, ConversationTopic, ConversationTopicKind,
    JournalEntry, JournalSource, PlayerBackground,
};
use ultimate_fate_worldgen::{PlayableSitePlan, ResidentActivity};

pub const DEFAULT_HISTORY_YEARS: u32 = 20;
pub const LIVING_MONTH_TURNS: u64 = 2_400;
pub const SAVE_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CampaignProgress {
    pub met_contact: bool,
    pub inspected_evidence: bool,
    pub questioned_factions: BTreeSet<FactionId>,
    pub learned_topics: BTreeSet<(PersonId, ConversationTopicKind)>,
    pub resolved_encounter: bool,
    pub recovered_history_item: bool,
    pub reconstructed_formulas: BTreeSet<FormulaId>,
    pub recorded_dungeon_clear: bool,
    pub received_starter_sword: bool,
    pub resolved_crisis: Option<CrisisResolutionOutcome>,
    pub resolved_regional_goals: BTreeSet<GoalId>,
    pub faction_standing: BTreeMap<FactionId, i16>,
    pub witnessed_thefts: BTreeSet<ItemId>,
    pub aid_supporters: BTreeSet<PersonId>,
    pub aid_acquisition: Option<AidResolutionKind>,
    pub aid_aftermath_event: Option<EventId>,
    pub player_coin: i64,
    pub aftermath_complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SituationId {
    AidAccess(EventId),
    LocalCrisis(EventId),
    HistoricalRecovery(EventId),
    Regional(GoalId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SituationStatus {
    Available,
    InProgress,
    Resolved,
    ClosedByWorld,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SituationCondition {
    MeetPerson { person: PersonId, satisfied: bool },
    InspectEvent { event: EventId, satisfied: bool },
    HearFactionAccounts { heard: usize, required: usize },
    RecoverHistoricalItem { item: WorldItemId, satisfied: bool },
    DefeatOpponent { entity: EntityId, satisfied: bool },
    ReconstructInscription { satisfied: bool },
    ReachRegionalTarget { goal: GoalId, satisfied: bool },
    ReceiveAid { person: PersonId, satisfied: bool },
}

impl SituationCondition {
    pub fn satisfied(&self) -> bool {
        match self {
            Self::MeetPerson { satisfied, .. }
            | Self::InspectEvent { satisfied, .. }
            | Self::RecoverHistoricalItem { satisfied, .. }
            | Self::DefeatOpponent { satisfied, .. }
            | Self::ReconstructInscription { satisfied }
            | Self::ReachRegionalTarget { satisfied, .. }
            | Self::ReceiveAid { satisfied, .. } => *satisfied,
            Self::HearFactionAccounts {
                heard, required, ..
            } => heard >= required,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignSituation {
    pub id: SituationId,
    pub title: String,
    pub description: String,
    pub cause: EventId,
    pub sponsor: Option<FactionId>,
    pub actors: Vec<PersonId>,
    pub status: SituationStatus,
    pub conditions: Vec<SituationCondition>,
    pub approaches: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResidentGoal {
    Work,
    Socialize,
    Rest,
    SeekFood,
    SeekSafety,
}

impl ResidentGoal {
    fn activity(self) -> ResidentActivity {
        match self {
            Self::Work => ResidentActivity::Working,
            Self::Socialize => ResidentActivity::AtLeisure,
            Self::Rest => ResidentActivity::AtHome,
            Self::SeekFood => ResidentActivity::SeekingFood,
            Self::SeekSafety => ResidentActivity::SeekingSafety,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResidentAgentState {
    pub person: PersonId,
    pub goal: ResidentGoal,
    pub hunger: u8,
    pub fatigue: u8,
    pub fear: u8,
    pub isolation: u8,
    pub goal_since: u64,
    pub completed_actions: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignCommand {
    Game(GameCommand),
    Interact,
    InspectHere,
    Talk {
        person: PersonId,
        topic: ConversationTopicKind,
    },
    InspectEvidence(EventId),
    InspectHistoricalSite(EventId),
    ResolveCrisis(CrisisResolutionKind),
    ResolveRegionalGoal {
        goal: GoalId,
        approach: RegionalGoalApproach,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CampaignEvent {
    Conversation {
        speaker: PersonId,
        topic: ConversationTopic,
    },
    EvidenceInspected {
        event: EventId,
        name: String,
        description: String,
        newly_discovered: bool,
    },
    ItemGifted {
        item: ItemId,
        from: EntityId,
        to: EntityId,
    },
    AidSupported {
        advocate: PersonId,
        patient: PersonId,
    },
    ItemAcquired {
        item: ItemId,
        from: PersonId,
        method: AidResolutionKind,
    },
    AidDelivered {
        patient: PersonId,
        item: ItemId,
        method: AidResolutionKind,
        event: EventId,
    },
    CrisisResolved(CrisisResolutionOutcome),
    RegionalGoalResolved(RegionalGoalOutcome),
    StandingChanged {
        faction: FactionId,
        amount: i16,
        reason: String,
    },
    ActionRejected(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CampaignOutcome {
    pub simulation: CommandOutcome,
    pub campaign_events: Vec<CampaignEvent>,
    pub historical_events: Vec<EventId>,
    pub month_summaries: Vec<MonthSummary>,
    pub resident_moves: usize,
    pub journal_entries_added: usize,
    pub errors: Vec<String>,
}

impl CampaignOutcome {
    pub fn advanced_time(&self) -> bool {
        self.simulation.advanced_time
    }

    pub fn changed_world(&self) -> bool {
        self.simulation.changed_world
            || !self.historical_events.is_empty()
            || !self.month_summaries.is_empty()
    }

    fn absorb(&mut self, mut other: Self) {
        self.simulation.advanced_time |= other.simulation.advanced_time;
        self.simulation.changed_world |= other.simulation.changed_world;
        self.simulation.events.append(&mut other.simulation.events);
        self.campaign_events.append(&mut other.campaign_events);
        self.historical_events.append(&mut other.historical_events);
        self.month_summaries.append(&mut other.month_summaries);
        self.resident_moves = self.resident_moves.saturating_add(other.resident_moves);
        self.journal_entries_added = self
            .journal_entries_added
            .saturating_add(other.journal_entries_added);
        self.errors.append(&mut other.errors);
    }
}

pub struct CampaignSession {
    history: HistoryEngine,
    campaign_start: CampaignStart,
    site_plan: PlayableSitePlan,
    simulation: Simulation,
    progress: CampaignProgress,
    resident_agents: BTreeMap<PersonId, ResidentAgentState>,
    history_years: u32,
    command_log: Vec<CampaignCommand>,
    next_living_month_turn: u64,
    last_regional_party_turn: u64,
}

impl CampaignSession {
    pub fn new(seed: u64) -> Result<Self, String> {
        Self::with_history_years(seed, DEFAULT_HISTORY_YEARS)
    }

    pub fn with_history_years(seed: u64, years: u32) -> Result<Self, String> {
        let mut history = HistoryEngine::seeded_town(seed).map_err(|error| error.to_string())?;
        history
            .simulate_years(years)
            .map_err(|error| error.to_string())?;
        let start = CampaignStart::for_outsider(history.world(), history.primary_site())
            .map_err(|error| error.to_string())?;
        history
            .begin_living_simulation()
            .map_err(|error| error.to_string())?;
        let site_plan = PlayableSitePlan::from_history(history.world(), &start)
            .map_err(|error| error.to_string())?;
        let mut simulation = site_plan
            .build_simulation()
            .map_err(|error| error.to_string())?;
        site_plan.synchronize_regional_parties(history.world(), &mut simulation);
        debug_assert_eq!(start.briefing.background, PlayerBackground::Outsider);
        debug_assert_eq!(simulation.rules(), history.world().rules());
        let resident_agents = site_plan
            .residents
            .iter()
            .map(|resident| {
                let variation = ((seed ^ resident.person.0.wrapping_mul(0x9e37_79b9)) % 17) as u8;
                (
                    resident.person,
                    ResidentAgentState {
                        person: resident.person,
                        goal: ResidentGoal::Work,
                        hunger: 8 + variation,
                        fatigue: 5 + variation / 2,
                        fear: 0,
                        isolation: 6 + variation,
                        goal_since: 0,
                        completed_actions: 0,
                    },
                )
            })
            .collect();

        Ok(Self {
            history,
            campaign_start: start,
            site_plan,
            simulation,
            progress: CampaignProgress {
                player_coin: 30,
                ..CampaignProgress::default()
            },
            resident_agents,
            history_years: years,
            command_log: Vec::new(),
            next_living_month_turn: LIVING_MONTH_TURNS,
            last_regional_party_turn: 0,
        })
    }

    pub fn history(&self) -> &HistoryEngine {
        &self.history
    }

    pub fn start(&self) -> &CampaignStart {
        &self.campaign_start
    }

    pub fn site_plan(&self) -> &PlayableSitePlan {
        &self.site_plan
    }

    pub fn simulation(&self) -> &Simulation {
        &self.simulation
    }

    pub fn progress(&self) -> &CampaignProgress {
        &self.progress
    }

    /// Projects player-facing commitments from authoritative world state.
    ///
    /// These records never own completion flags. They observe people, evidence,
    /// inventories, combatants, historical goals, and recorded consequences, so
    /// a client cannot complete a situation by editing presentation state.
    pub fn situations(&self) -> Vec<CampaignSituation> {
        let mut situations = Vec::new();
        let aid = &self.site_plan.aid;
        let aid_delivery = self.aid_delivery();
        let aid_in_player_custody = self
            .simulation
            .player_inventory()
            .is_some_and(|inventory| inventory.items.contains(&aid.medicine.id));
        situations.push(CampaignSituation {
            id: SituationId::AidAccess(aid.cause),
            title: aid.title.clone(),
            description: aid.description.clone(),
            cause: aid.cause,
            sponsor: Some(self.history.world().people()[&aid.advocate].faction),
            actors: vec![aid.patient, aid.custodian, aid.advocate],
            status: if aid_delivery.is_some() {
                SituationStatus::Resolved
            } else if aid_in_player_custody || self.progress.aid_supporters.contains(&aid.advocate)
            {
                SituationStatus::InProgress
            } else {
                SituationStatus::Available
            },
            conditions: vec![SituationCondition::ReceiveAid {
                person: aid.patient,
                satisfied: aid_delivery.is_some(),
            }],
            approaches: vec![
                format!("ask {} to support an appeal", aid.advocate_name),
                format!("persuade {} to release the medicine", aid.custodian_name),
                format!("purchase it for {} coin", aid.price),
                "take it and accept the consequences".to_string(),
                "supply another effective treatment".to_string(),
            ],
        });
        let local_conditions = vec![
            SituationCondition::MeetPerson {
                person: self.site_plan.contact,
                satisfied: self.progress.met_contact,
            },
            SituationCondition::InspectEvent {
                event: self.site_plan.evidence_event,
                satisfied: self.progress.inspected_evidence,
            },
            SituationCondition::HearFactionAccounts {
                heard: self.progress.questioned_factions.len(),
                required: 2,
            },
        ];
        situations.push(CampaignSituation {
            id: SituationId::LocalCrisis(self.site_plan.crisis_event),
            title: format!("The crisis in {}", self.site_plan.town_name),
            description: self
                .history
                .world()
                .events()
                .get(&self.site_plan.crisis_event)
                .map(|event| event.summary.clone())
                .unwrap_or_else(|| "A local crisis demands investigation.".to_string()),
            cause: self.site_plan.crisis_event,
            sponsor: Some(
                self.history.world().sites()[&self.site_plan.site]
                    .laws
                    .values()
                    .find(|law| law.active)
                    .map(|law| law.authority)
                    .unwrap_or(self.history.world().people()[&self.site_plan.contact].faction),
            ),
            actors: vec![self.site_plan.contact],
            status: if self.progress.resolved_crisis.is_some() {
                SituationStatus::Resolved
            } else if local_conditions.iter().any(SituationCondition::satisfied) {
                SituationStatus::InProgress
            } else {
                SituationStatus::Available
            },
            conditions: local_conditions,
            approaches: vec![
                "uphold the emergency law".to_string(),
                "open the public stores".to_string(),
                "broker a compromise".to_string(),
            ],
        });

        let recovery_conditions = vec![
            SituationCondition::RecoverHistoricalItem {
                item: self.site_plan.dungeon.world_item,
                satisfied: self.progress.recovered_history_item,
            },
            SituationCondition::DefeatOpponent {
                entity: self.site_plan.dungeon.boss,
                satisfied: self
                    .simulation
                    .combatant(self.site_plan.dungeon.boss)
                    .is_some_and(|combatant| !combatant.is_alive()),
            },
            SituationCondition::ReconstructInscription {
                satisfied: !self.progress.reconstructed_formulas.is_empty(),
            },
        ];
        situations.push(CampaignSituation {
            id: SituationId::HistoricalRecovery(self.site_plan.dungeon.related_event),
            title: self.site_plan.dungeon.quest_title.clone(),
            description: self.site_plan.dungeon.quest_description.clone(),
            cause: self.site_plan.dungeon.related_event,
            sponsor: Some(self.history.world().people()[&self.site_plan.contact].faction),
            actors: vec![self.site_plan.contact],
            status: if self.progress.recorded_dungeon_clear {
                SituationStatus::Resolved
            } else if recovery_conditions
                .iter()
                .any(SituationCondition::satisfied)
            {
                SituationStatus::InProgress
            } else {
                SituationStatus::Available
            },
            conditions: recovery_conditions,
            approaches: vec![
                "recover the object".to_string(),
                "reconstruct its lost formula".to_string(),
                "decide who should hold it".to_string(),
            ],
        });

        for goal in self
            .history
            .world()
            .regional_goals()
            .values()
            .filter(|goal| goal.status == RegionalGoalStatus::Open)
        {
            let at_target =
                self.site_plan
                    .regional_goal_target(goal.kind)
                    .is_some_and(|(_, target)| {
                        let player = self.simulation.player().position;
                        player.map == self.site_plan.regional_map
                            && manhattan(player.grid.x, player.grid.y, target.x, target.y) <= 2
                    });
            let approaches = self
                .history
                .regional_goal_options(goal.id)
                .unwrap_or_default()
                .into_iter()
                .map(|option| option.title)
                .collect();
            situations.push(CampaignSituation {
                id: SituationId::Regional(goal.id),
                title: goal.title.clone(),
                description: goal.description.clone(),
                cause: goal.cause,
                sponsor: Some(goal.sponsor),
                actors: Vec::new(),
                status: if at_target {
                    SituationStatus::InProgress
                } else {
                    SituationStatus::Available
                },
                conditions: vec![SituationCondition::ReachRegionalTarget {
                    goal: goal.id,
                    satisfied: at_target,
                }],
                approaches,
            });
        }
        situations.sort_by_key(|situation| situation.id);
        situations
    }

    /// Returns the material treatment currently held by the generated patient.
    ///
    /// Situation completion is deliberately derived from inventory, ownership,
    /// and provenance state. `aid_aftermath_event` only prevents duplicate
    /// historical records; it does not decide whether care was delivered.
    pub fn aid_delivery(&self) -> Option<(ItemId, AidResolutionKind)> {
        let aid = &self.site_plan.aid;
        let inventory = self.simulation.inventory(aid.patient_entity)?;
        inventory.items.iter().find_map(|item_id| {
            let item = self.simulation.item(*item_id)?;
            let ItemKind::Consumable { healing } = item.kind else {
                return None;
            };
            if item.quantity == 0 || healing < 5 {
                return None;
            }
            let method = if *item_id != aid.medicine.id {
                AidResolutionKind::AlternativeTreatment
            } else if self.simulation.is_stolen(*item_id) {
                AidResolutionKind::TakenWithoutConsent
            } else {
                self.progress
                    .aid_acquisition
                    .unwrap_or(AidResolutionKind::ReleasedByConsent)
            };
            Some((*item_id, method))
        })
    }

    /// Projects conversation plus context-sensitive social actions. The text
    /// layer still generates ordinary topics; campaign state contributes only
    /// actions that are currently meaningful for the nearby generated actors.
    pub fn conversation_for_person(
        &self,
        person: PersonId,
        context: &ConversationContext,
    ) -> Result<Conversation, String> {
        let mut conversation =
            Conversation::for_person(self.history.world(), &self.campaign_start, person, context)
                .map_err(|error| error.to_string())?;
        if self.aid_delivery().is_some() {
            return Ok(conversation);
        }

        let aid = &self.site_plan.aid;
        let mut actions = Vec::new();
        if person == aid.advocate && !self.progress.aid_supporters.contains(&person) {
            actions.push(ConversationTopic {
                kind: ConversationTopicKind::SupportAid(aid.patient),
                prompt: format!("Will you support {}'s claim to care?", aid.patient_name),
                response: format!(
                    "{} agrees to put their name and faction standing behind the request.",
                    aid.advocate_name
                ),
                reveals_events: vec![aid.cause],
                reveals_claims: Vec::new(),
            });
        }
        if person == aid.custodian
            && self
                .simulation
                .inventory(aid.custodian_entity)
                .is_some_and(|inventory| inventory.items.contains(&aid.medicine.id))
        {
            let release_allowed = self.aid_release_allowed();
            actions.extend([
                ConversationTopic {
                    kind: ConversationTopicKind::RequestAid(aid.patient),
                    prompt: format!("Release the medicine for {}.", aid.patient_name),
                    response: if release_allowed {
                        format!(
                            "{} accepts the appeal and releases the medicine to you.",
                            aid.custodian_name
                        )
                    } else {
                        format!(
                            "{} refuses while the emergency restriction stands. {} could give the appeal political weight.",
                            aid.custodian_name, aid.advocate_name
                        )
                    },
                    reveals_events: vec![aid.cause],
                    reveals_claims: Vec::new(),
                },
                ConversationTopic {
                    kind: ConversationTopicKind::OfferPayment(aid.patient),
                    prompt: format!("Offer {} coin for the medicine.", aid.price),
                    response: if self.progress.player_coin >= aid.price {
                        format!(
                            "{} accepts payment and transfers lawful title to the medicine.",
                            aid.custodian_name
                        )
                    } else {
                        format!(
                            "{} will not sell it for less than {} coin; you have {}.",
                            aid.custodian_name, aid.price, self.progress.player_coin
                        )
                    },
                    reveals_events: Vec::new(),
                    reveals_claims: Vec::new(),
                },
                ConversationTopic {
                    kind: ConversationTopicKind::TakeAid(aid.patient),
                    prompt: "Take the medicine without consent.".to_string(),
                    response: format!(
                        "{} witnesses the taking and calls it theft, whatever your purpose.",
                        aid.custodian_name
                    ),
                    reveals_events: Vec::new(),
                    reveals_claims: Vec::new(),
                },
            ]);
        }
        if person == aid.patient {
            let carried_treatment = self
                .simulation
                .player_inventory()
                .into_iter()
                .flat_map(|inventory| inventory.items.iter())
                .any(|item_id| {
                    self.simulation.item(*item_id).is_some_and(|item| {
                        item.quantity > 0
                            && matches!(item.kind, ItemKind::Consumable { healing } if healing >= 5)
                    })
                });
            if carried_treatment {
                actions.push(ConversationTopic {
                    kind: ConversationTopicKind::OfferAid(aid.patient),
                    prompt: "Offer the treatment you are carrying.".to_string(),
                    response: format!(
                        "{} accepts the material treatment; its origin and ownership will shape the aftermath.",
                        aid.patient_name
                    ),
                    reveals_events: Vec::new(),
                    reveals_claims: Vec::new(),
                });
            }
        }

        let ordinary_limit = 7_usize.saturating_sub(actions.len());
        conversation.topics.sort_by_key(|topic| match topic.kind {
            ConversationTopicKind::Evidence(_) => 0,
            ConversationTopicKind::Orientation => 1,
            ConversationTopicKind::Aftermath(_) => 2,
            ConversationTopicKind::PresentCrisis => 3,
            ConversationTopicKind::Law(_) => 4,
            ConversationTopicKind::FactionView => 5,
            ConversationTopicKind::Claim(_) => 6,
            ConversationTopicKind::RequestAid(_)
            | ConversationTopicKind::OfferPayment(_)
            | ConversationTopicKind::OfferAid(_)
            | ConversationTopicKind::SupportAid(_)
            | ConversationTopicKind::TakeAid(_) => 7,
        });
        conversation.topics.truncate(ordinary_limit);
        conversation.topics.extend(actions);
        Ok(conversation)
    }

    fn aid_release_allowed(&self) -> bool {
        let aid = &self.site_plan.aid;
        let restriction_active = aid.restricting_law.is_some_and(|law| {
            self.history.world().sites()[&self.site_plan.site]
                .laws
                .get(&law)
                .is_some_and(|law| law.active)
        });
        let custodian_faction = self.history.world().people()[&aid.custodian].faction;
        !restriction_active
            || self.progress.aid_supporters.contains(&aid.advocate)
            || self
                .progress
                .faction_standing
                .get(&custodian_faction)
                .copied()
                .unwrap_or_default()
                >= 10
    }

    pub fn resident_agents(&self) -> &BTreeMap<PersonId, ResidentAgentState> {
        &self.resident_agents
    }

    pub fn command_log(&self) -> &[CampaignCommand] {
        &self.command_log
    }

    pub fn save_to_string(&self) -> String {
        let mut save = format!(
            "ULTIMATE_FATE|{}|{}|{}|{}\n",
            SAVE_FORMAT_VERSION,
            self.simulation.campaign_seed,
            self.history_years,
            self.command_log.len()
        );
        for command in &self.command_log {
            let _ = writeln!(save, "{}", encode_command(*command));
        }
        save
    }

    pub fn load_from_str(save: &str) -> Result<Self, String> {
        let mut lines = save.lines();
        let header = lines
            .next()
            .ok_or_else(|| "save is empty".to_string())?
            .split('|')
            .collect::<Vec<_>>();
        if header.len() != 5 || header[0] != "ULTIMATE_FATE" {
            return Err("save header is invalid".to_string());
        }
        let version = parse_u32(header[1], "save version")?;
        if version != SAVE_FORMAT_VERSION {
            return Err(format!(
                "save version {version} is unsupported; expected {SAVE_FORMAT_VERSION}"
            ));
        }
        let seed = parse_u64(header[2], "campaign seed")?;
        let history_years = parse_u32(header[3], "history years")?;
        let command_count = parse_usize(header[4], "command count")?;
        let command_lines = lines.collect::<Vec<_>>();
        if command_lines.len() != command_count {
            return Err(format!(
                "save declares {command_count} commands but contains {}",
                command_lines.len()
            ));
        }
        let commands = command_lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                decode_command(line)
                    .map_err(|error| format!("invalid command {}: {error}", index + 1))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut session = Self::with_history_years(seed, history_years)?;
        for command in commands {
            session.apply_command(command);
        }
        Ok(session)
    }

    pub fn apply_game_command(&mut self, command: GameCommand) -> CampaignOutcome {
        self.apply_command(CampaignCommand::Game(command))
    }

    fn apply_game_command_unlogged(&mut self, command: GameCommand) -> CampaignOutcome {
        let journal_before = self.campaign_start.journal.entries.len();
        let simulation = self.simulation.apply_command(command);
        let mut outcome = CampaignOutcome {
            simulation,
            ..CampaignOutcome::default()
        };
        self.commit_simulation_consequences(&mut outcome);
        self.commit_social_consequences(command, &mut outcome);
        self.commit_aid_consequences(&mut outcome);
        if outcome.simulation.advanced_time {
            outcome.resident_moves = self.advance_resident_agents();
            self.advance_regional_parties(&mut outcome);
        }
        self.advance_calendar(&mut outcome);
        outcome.journal_entries_added = self
            .campaign_start
            .journal
            .entries
            .len()
            .saturating_sub(journal_before);
        outcome
    }

    pub fn apply_command(&mut self, command: CampaignCommand) -> CampaignOutcome {
        self.command_log.push(command);
        match command {
            CampaignCommand::Game(command) => self.apply_game_command_unlogged(command),
            CampaignCommand::Interact => self.interact(),
            CampaignCommand::InspectHere => self.inspect_here(),
            CampaignCommand::Talk { person, topic } => self.resolve_conversation(person, topic),
            CampaignCommand::InspectEvidence(event) => self.inspect_evidence(event),
            CampaignCommand::InspectHistoricalSite(event) => self.inspect_historical_site(event),
            CampaignCommand::ResolveCrisis(kind) => self.resolve_crisis(kind),
            CampaignCommand::ResolveRegionalGoal { goal, approach } => {
                self.resolve_regional_goal(goal, approach)
            }
        }
    }

    fn interact(&mut self) -> CampaignOutcome {
        if let Some(target) = self.simulation.hostile_in_melee_range() {
            return self.apply_game_command_unlogged(GameCommand::Attack(target));
        }
        let player = self.simulation.player().position;
        if self.simulation.transition_at(player).is_some() {
            return self.apply_game_command_unlogged(GameCommand::Traverse);
        }
        let ready_quest = self.simulation.quests().find_map(|quest| {
            let giver = self.simulation.entity(quest.giver)?;
            (quest.status == ultimate_fate_core::QuestStatus::ReadyToTurnIn
                && giver.position.map == player.map
                && giver.position.grid.z == player.grid.z
                && manhattan(
                    giver.position.grid.x,
                    giver.position.grid.y,
                    player.grid.x,
                    player.grid.y,
                ) <= 1)
                .then_some(quest.id)
        });
        if let Some(quest) = ready_quest {
            return self.apply_game_command_unlogged(GameCommand::TurnInQuest(quest));
        }
        let resident = self
            .site_plan
            .residents
            .iter()
            .filter_map(|resident| {
                let position = self.simulation.entity(resident.entity)?.position;
                (position.map == player.map
                    && position.grid.z == player.grid.z
                    && manhattan(
                        position.grid.x,
                        position.grid.y,
                        player.grid.x,
                        player.grid.y,
                    ) <= 1)
                    .then_some(resident.clone())
            })
            .min_by_key(|resident| (resident.person != self.site_plan.contact, resident.person));
        let Some(resident) = resident else {
            return rejected("there is nothing here to interact with".to_string());
        };
        let mut context = ConversationContext::default();
        if self.progress.inspected_evidence {
            context
                .examined_evidence
                .insert(self.site_plan.evidence_event);
        }
        let conversation = match self.conversation_for_person(resident.person, &context) {
            Ok(conversation) => conversation,
            Err(error) => return rejected(error),
        };
        let topic = conversation
            .topics
            .iter()
            .find(|topic| {
                !self
                    .progress
                    .learned_topics
                    .contains(&(resident.person, topic.kind))
            })
            .or_else(|| conversation.topics.first())
            .map(|topic| topic.kind);
        match topic {
            Some(topic) => self.resolve_conversation(resident.person, topic),
            None => rejected(format!("{} has nothing to discuss", resident.name)),
        }
    }

    fn inspect_here(&mut self) -> CampaignOutcome {
        let player = self.simulation.player().position;
        let evidence = self.site_plan.evidence_location();
        if player.map == self.site_plan.map
            && player.grid.z == evidence.position.z
            && manhattan(
                player.grid.x,
                player.grid.y,
                evidence.position.x,
                evidence.position.y,
            ) <= 1
        {
            return self.inspect_evidence(self.site_plan.evidence_event);
        }
        let historical_site = self
            .site_plan
            .regional_history_sites
            .iter()
            .filter(|site| {
                player.map == self.site_plan.regional_map
                    && manhattan(
                        player.grid.x,
                        player.grid.y,
                        site.position.x,
                        site.position.y,
                    ) <= 2
            })
            .min_by_key(|site| site.event)
            .map(|site| site.event);
        match historical_site {
            Some(event) => self.inspect_historical_site(event),
            None => rejected("nothing nearby reveals additional history".to_string()),
        }
    }

    fn resolve_conversation(
        &mut self,
        person: PersonId,
        topic_kind: ConversationTopicKind,
    ) -> CampaignOutcome {
        let Some(resident) = self
            .site_plan
            .residents
            .iter()
            .find(|resident| resident.person == person)
            .cloned()
        else {
            return rejected(format!("person {} is not materialized here", person.0));
        };
        let Some(resident_position) = self
            .simulation
            .entity(resident.entity)
            .map(|entity| entity.position)
        else {
            return rejected(format!("person {} has no local entity", person.0));
        };
        let player_position = self.simulation.player().position;
        if player_position.map != resident_position.map
            || player_position.grid.z != resident_position.grid.z
            || manhattan(
                player_position.grid.x,
                player_position.grid.y,
                resident_position.grid.x,
                resident_position.grid.y,
            ) > 1
        {
            return rejected(format!(
                "{} is not close enough to speak with",
                resident.name
            ));
        }

        let mut context = ConversationContext::default();
        if self.progress.inspected_evidence {
            context
                .examined_evidence
                .insert(self.site_plan.evidence_event);
        }
        let conversation = match self.conversation_for_person(person, &context) {
            Ok(conversation) => conversation,
            Err(error) => return rejected(error),
        };
        let Some(topic) = conversation
            .topics
            .iter()
            .find(|topic| topic.kind == topic_kind)
            .cloned()
        else {
            return rejected(format!("{} cannot discuss that topic", resident.name));
        };

        let journal_before = self.campaign_start.journal.entries.len();
        self.campaign_start
            .journal
            .knowledge
            .known_events
            .extend(topic.reveals_events.iter().copied());
        self.campaign_start
            .journal
            .knowledge
            .known_claims
            .extend(topic.reveals_claims.iter().copied());
        if self.progress.learned_topics.insert((person, topic.kind)) {
            self.simulation.record_social_practice();
            self.campaign_start.journal.entries.push(JournalEntry {
                title: format!("{}: {}", conversation.speaker_name, topic.prompt),
                body: topic.response.clone(),
                learned_at: self.history.world().date,
                source: JournalSource::Conversation(person),
                events: topic.reveals_events.clone(),
                claims: topic.reveals_claims.clone(),
            });
        }

        let mut outcome = CampaignOutcome {
            campaign_events: vec![CampaignEvent::Conversation {
                speaker: person,
                topic: topic.clone(),
            }],
            ..CampaignOutcome::default()
        };
        if topic.kind == ConversationTopicKind::Orientation
            && person == self.site_plan.contact
            && !self.progress.met_contact
        {
            self.progress.met_contact = true;
            self.give_starter_sword(resident.entity, &mut outcome);
        }
        if matches!(topic.kind, ConversationTopicKind::Evidence(_)) {
            self.progress.questioned_factions.insert(resident.faction);
        }
        if let ConversationTopicKind::Aftermath(event) = topic.kind
            && self
                .progress
                .resolved_crisis
                .as_ref()
                .is_some_and(|resolution| {
                    resolution.event == event && resolution.reaction_faction == resident.faction
                })
        {
            self.progress.aftermath_complete = true;
        }
        match topic.kind {
            ConversationTopicKind::SupportAid(patient)
                if person == self.site_plan.aid.advocate
                    && patient == self.site_plan.aid.patient =>
            {
                if self.progress.aid_supporters.insert(person) {
                    self.simulation.record_social_practice();
                    outcome.campaign_events.push(CampaignEvent::AidSupported {
                        advocate: person,
                        patient,
                    });
                }
            }
            ConversationTopicKind::RequestAid(patient)
                if person == self.site_plan.aid.custodian
                    && patient == self.site_plan.aid.patient =>
            {
                if self.aid_release_allowed() {
                    self.acquire_aid_medicine(AidResolutionKind::ReleasedByConsent, &mut outcome);
                }
            }
            ConversationTopicKind::OfferPayment(patient)
                if person == self.site_plan.aid.custodian
                    && patient == self.site_plan.aid.patient =>
            {
                if self.progress.player_coin >= self.site_plan.aid.price
                    && self.acquire_aid_medicine(AidResolutionKind::Purchased, &mut outcome)
                {
                    self.progress.player_coin -= self.site_plan.aid.price;
                }
            }
            ConversationTopicKind::TakeAid(patient)
                if person == self.site_plan.aid.custodian
                    && patient == self.site_plan.aid.patient =>
            {
                let nested = self.apply_game_command_unlogged(GameCommand::Take {
                    item: self.site_plan.aid.medicine.id,
                    from: self.site_plan.aid.custodian_entity,
                });
                outcome.absorb(nested);
            }
            ConversationTopicKind::OfferAid(patient)
                if person == self.site_plan.aid.patient
                    && patient == self.site_plan.aid.patient =>
            {
                if let Some(item) = self.best_carried_treatment() {
                    let nested = self.apply_game_command_unlogged(GameCommand::Give {
                        item,
                        to: self.site_plan.aid.patient_entity,
                    });
                    outcome.absorb(nested);
                }
            }
            _ => {}
        }
        outcome.journal_entries_added = self
            .campaign_start
            .journal
            .entries
            .len()
            .saturating_sub(journal_before);
        outcome
    }

    fn give_starter_sword(&mut self, from: EntityId, outcome: &mut CampaignOutcome) {
        if self.progress.received_starter_sword {
            return;
        }
        let player = self.simulation.player_id();
        let item = self.site_plan.starter_sword.id;
        match self.simulation.transfer_item(item, from, player) {
            Ok(event) => {
                outcome.simulation.changed_world = true;
                outcome.simulation.events.push(event);
                let equipped = self.simulation.apply_command(GameCommand::Equip(item));
                outcome.simulation.changed_world |= equipped.changed_world;
                outcome.simulation.events.extend(equipped.events);
                self.progress.received_starter_sword = true;
                self.campaign_start.journal.entries.push(JournalEntry {
                    title: format!("The {}", self.site_plan.starter_sword.name),
                    body: self.site_plan.starter_sword_provenance.clone(),
                    learned_at: self.history.world().date,
                    source: JournalSource::Conversation(self.site_plan.contact),
                    events: vec![self.site_plan.crisis_event],
                    claims: Vec::new(),
                });
                outcome.campaign_events.push(CampaignEvent::ItemGifted {
                    item,
                    from,
                    to: player,
                });
            }
            Err(error) => outcome.errors.push(format!("{error:?}")),
        }
    }

    fn acquire_aid_medicine(
        &mut self,
        method: AidResolutionKind,
        outcome: &mut CampaignOutcome,
    ) -> bool {
        let aid = &self.site_plan.aid;
        let player = self.simulation.player_id();
        let custodian_has_item = self
            .simulation
            .inventory(aid.custodian_entity)
            .is_some_and(|inventory| inventory.items.contains(&aid.medicine.id));
        if !custodian_has_item
            || self.simulation.legal_owner(aid.medicine.id) != Some(aid.custodian_entity)
        {
            return false;
        }
        let Ok(event) =
            self.simulation
                .transfer_item(aid.medicine.id, aid.custodian_entity, player)
        else {
            return false;
        };
        if !self
            .simulation
            .transfer_legal_ownership(aid.medicine.id, aid.custodian_entity, player)
        {
            outcome
                .errors
                .push("medicine custody changed without a valid title transfer".to_string());
            return false;
        }
        self.progress.aid_acquisition = Some(method);
        self.simulation.record_social_practice();
        outcome.simulation.changed_world = true;
        outcome.simulation.events.push(event);
        outcome.campaign_events.push(CampaignEvent::ItemAcquired {
            item: aid.medicine.id,
            from: aid.custodian,
            method,
        });
        true
    }

    fn best_carried_treatment(&self) -> Option<ItemId> {
        let medicine = self.site_plan.aid.medicine.id;
        self.simulation
            .player_inventory()?
            .items
            .iter()
            .filter_map(|item_id| {
                let item = self.simulation.item(*item_id)?;
                let ItemKind::Consumable { healing } = item.kind else {
                    return None;
                };
                (item.quantity > 0 && healing >= 5).then_some((
                    *item_id == medicine,
                    healing,
                    std::cmp::Reverse(*item_id),
                    *item_id,
                ))
            })
            .max()
            .map(|(_, _, _, item)| item)
    }

    fn inspect_evidence(&mut self, event: EventId) -> CampaignOutcome {
        if event != self.site_plan.evidence_event {
            return rejected(format!(
                "event {} has no inspectable local evidence",
                event.0
            ));
        }
        let evidence = self.site_plan.evidence_location().clone();
        let player = self.simulation.player().position;
        if player.map != self.site_plan.map
            || player.grid.z != evidence.position.z
            || manhattan(
                player.grid.x,
                player.grid.y,
                evidence.position.x,
                evidence.position.y,
            ) > 1
        {
            return rejected(format!("{} is not close enough to inspect", evidence.name));
        }
        let newly_discovered = !self.progress.inspected_evidence;
        let journal_before = self.campaign_start.journal.entries.len();
        if newly_discovered {
            self.progress.inspected_evidence = true;
            self.campaign_start
                .journal
                .knowledge
                .known_events
                .insert(event);
            self.campaign_start.journal.entries.push(JournalEntry {
                title: format!("Evidence at {}", evidence.name),
                body: self.site_plan.evidence_description.clone(),
                learned_at: self.history.world().date,
                source: JournalSource::PhysicalEvidence(event),
                events: vec![event],
                claims: Vec::new(),
            });
        }
        CampaignOutcome {
            campaign_events: vec![CampaignEvent::EvidenceInspected {
                event,
                name: evidence.name,
                description: self.site_plan.evidence_description.clone(),
                newly_discovered,
            }],
            journal_entries_added: self
                .campaign_start
                .journal
                .entries
                .len()
                .saturating_sub(journal_before),
            ..CampaignOutcome::default()
        }
    }

    fn inspect_historical_site(&mut self, event: EventId) -> CampaignOutcome {
        let Some(site) = self
            .site_plan
            .regional_history_sites
            .iter()
            .find(|site| site.event == event)
            .cloned()
        else {
            return rejected(format!("event {} has no regional historical site", event.0));
        };
        let player = self.simulation.player().position;
        if player.map != self.site_plan.regional_map
            || manhattan(
                player.grid.x,
                player.grid.y,
                site.position.x,
                site.position.y,
            ) > 2
        {
            return rejected(format!("{} is not close enough to inspect", site.name));
        }
        let journal_before = self.campaign_start.journal.entries.len();
        let newly_discovered = self
            .campaign_start
            .journal
            .knowledge
            .known_events
            .insert(event);
        if newly_discovered {
            self.campaign_start.journal.entries.push(JournalEntry {
                title: site.name.clone(),
                body: site.description.clone(),
                learned_at: self.history.world().date,
                source: JournalSource::PhysicalEvidence(event),
                events: vec![event],
                claims: Vec::new(),
            });
        }
        CampaignOutcome {
            campaign_events: vec![CampaignEvent::EvidenceInspected {
                event,
                name: site.name,
                description: site.description,
                newly_discovered,
            }],
            journal_entries_added: self
                .campaign_start
                .journal
                .entries
                .len()
                .saturating_sub(journal_before),
            ..CampaignOutcome::default()
        }
    }

    fn resolve_crisis(&mut self, kind: CrisisResolutionKind) -> CampaignOutcome {
        if self.progress.resolved_crisis.is_some() {
            return rejected("the local crisis is already resolved".to_string());
        }
        if !self.progress.inspected_evidence || self.progress.questioned_factions.len() < 2 {
            return rejected(
                "inspect the evidence and hear two faction accounts before judging the crisis"
                    .to_string(),
            );
        }
        let journal_before = self.campaign_start.journal.entries.len();
        let resolution = match self
            .history
            .resolve_crisis(self.site_plan.crisis_event, kind)
        {
            Ok(resolution) => resolution,
            Err(error) => return rejected(error.to_string()),
        };
        self.campaign_start
            .journal
            .knowledge
            .known_events
            .insert(resolution.event);
        self.campaign_start.journal.entries.push(JournalEntry {
            title: "Resolution of the shortage".to_string(),
            body: format!(
                "{}. The town now holds {} food and {} coin under {} active law(s).",
                resolution.summary,
                resolution.food_after,
                resolution.coin_after,
                resolution.active_laws
            ),
            learned_at: self.history.world().date,
            source: JournalSource::Resolution(resolution.event),
            events: vec![resolution.event, self.site_plan.crisis_event],
            claims: Vec::new(),
        });
        self.progress.resolved_crisis = Some(resolution.clone());
        self.simulation.record_world_change();
        CampaignOutcome {
            historical_events: vec![resolution.event],
            campaign_events: vec![CampaignEvent::CrisisResolved(resolution)],
            journal_entries_added: self
                .campaign_start
                .journal
                .entries
                .len()
                .saturating_sub(journal_before),
            ..CampaignOutcome::default()
        }
    }

    fn resolve_regional_goal(
        &mut self,
        goal: GoalId,
        approach: RegionalGoalApproach,
    ) -> CampaignOutcome {
        let Some(goal_record) = self.history.world().regional_goals().get(&goal).cloned() else {
            return rejected(format!("regional goal {} no longer exists", goal.0));
        };
        let Some((target_name, target)) = self.site_plan.regional_goal_target(goal_record.kind)
        else {
            return rejected(format!("{} has no material target", goal_record.title));
        };
        let player = self.simulation.player().position;
        if player.map != self.site_plan.regional_map
            || manhattan(player.grid.x, player.grid.y, target.x, target.y) > 2
        {
            return rejected(format!("arrive at {target_name} before intervening"));
        }
        let journal_before = self.campaign_start.journal.entries.len();
        let resolution = match self.history.resolve_regional_goal(goal, approach) {
            Ok(resolution) => resolution,
            Err(error) => return rejected(error.to_string()),
        };
        self.campaign_start
            .journal
            .knowledge
            .known_events
            .insert(resolution.event);
        self.campaign_start.journal.entries.push(JournalEntry {
            title: "Regional intervention".to_string(),
            body: resolution.summary.clone(),
            learned_at: self.history.world().date,
            source: JournalSource::Resolution(resolution.event),
            events: vec![resolution.event],
            claims: Vec::new(),
        });
        self.site_plan
            .synchronize_regional_routes(self.history.world(), &mut self.simulation);
        self.site_plan
            .synchronize_regional_parties(self.history.world(), &mut self.simulation);
        self.progress.resolved_regional_goals.insert(goal);
        self.simulation.record_world_change();
        CampaignOutcome {
            historical_events: vec![resolution.event],
            campaign_events: vec![CampaignEvent::RegionalGoalResolved(resolution)],
            journal_entries_added: self
                .campaign_start
                .journal
                .entries
                .len()
                .saturating_sub(journal_before),
            ..CampaignOutcome::default()
        }
    }

    fn commit_simulation_consequences(&mut self, outcome: &mut CampaignOutcome) {
        let player = self.simulation.player_id();
        let defeated_parties = outcome
            .simulation
            .events
            .iter()
            .filter_map(|event| match event {
                SimulationEvent::Defeated { entity, by } if *by == player => {
                    self.regional_party_for_entity(*entity)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let completed_dungeon = outcome.simulation.events.iter().any(|event| {
            matches!(
                event,
                SimulationEvent::QuestCompleted { quest }
                    if *quest == self.site_plan.dungeon.quest
            )
        });
        let recovered_world_item = outcome.simulation.events.iter().any(|event| {
            let SimulationEvent::ItemTransferred { item, to, .. } = event else {
                return false;
            };
            *to == player
                && self.simulation.item(*item).is_some_and(|item| {
                    matches!(
                        item.kind,
                        ItemKind::InscribedArtifact { object, .. }
                            if object
                                == self.history.world().significant_items()
                                    [&self.site_plan.dungeon.world_item]
                                    .object
                    )
                })
        });
        let learned_formulas = outcome
            .simulation
            .events
            .iter()
            .filter_map(|event| match event {
                SimulationEvent::FormulaLearned { formula, .. } => Some(*formula),
                _ => None,
            })
            .collect::<Vec<_>>();

        if outcome.simulation.events.iter().any(|event| {
            matches!(
                event,
                SimulationEvent::Defeated { entity, by }
                    if *entity == self.site_plan.encounter.entity && *by == player
            )
        }) && !self.progress.resolved_encounter
        {
            self.progress.resolved_encounter = true;
            let related_event = self.site_plan.encounter.related_event;
            self.campaign_start
                .journal
                .knowledge
                .known_events
                .insert(related_event);
            self.campaign_start.journal.entries.push(JournalEntry {
                title: format!("Road encounter: {}", self.site_plan.encounter.name),
                body: format!(
                    "{} Its presence near town was another material consequence of the crisis.",
                    self.site_plan.encounter.description
                ),
                learned_at: self.history.world().date,
                source: JournalSource::Combat(related_event),
                events: vec![related_event],
                claims: Vec::new(),
            });
        }

        if recovered_world_item && !self.progress.recovered_history_item {
            match self.history.record_item_recovered_by_player(
                self.site_plan.dungeon.world_item,
                &self.site_plan.dungeon.name,
            ) {
                Ok(event) => {
                    self.progress.recovered_history_item = true;
                    self.record_history_event(event);
                    self.campaign_start.journal.entries.push(JournalEntry {
                        title: format!(
                            "Recovered {}",
                            self.history.world().significant_items()
                                [&self.site_plan.dungeon.world_item]
                                .name
                        ),
                        body: self.history.world().events()[&event].summary.clone(),
                        learned_at: self.history.world().date,
                        source: JournalSource::PhysicalEvidence(event),
                        events: vec![event, self.site_plan.dungeon.related_event],
                        claims: Vec::new(),
                    });
                    outcome.historical_events.push(event);
                }
                Err(error) => outcome.errors.push(error.to_string()),
            }
        }

        for formula_id in learned_formulas {
            if !self.progress.reconstructed_formulas.insert(formula_id) {
                continue;
            }
            let Some(formula) = self.simulation.rules().formula(formula_id).cloned() else {
                continue;
            };
            match self
                .history
                .record_formula_reconstructed_by_player(self.site_plan.dungeon.world_item)
            {
                Ok(event) => {
                    self.record_history_event(event);
                    let source_event = self
                        .history
                        .world()
                        .significant_items()
                        .values()
                        .find(|item| item.inscribed_formula == Some(formula.id))
                        .and_then(|item| item.provenance.last())
                        .map(|entry| entry.event)
                        .unwrap_or(event);
                    self.campaign_start.journal.entries.push(JournalEntry {
                        title: format!("Magical formula: {}", formula.name),
                        body: format!(
                            "Confirmed effect: {}. Reagents: {}. Condition: {}.",
                            formula.effect.name(),
                            formula
                                .reagents
                                .iter()
                                .map(|material| material.name())
                                .collect::<Vec<_>>()
                                .join(", "),
                            formula.condition.name()
                        ),
                        learned_at: self.history.world().date,
                        source: JournalSource::PhysicalEvidence(source_event),
                        events: vec![source_event, event],
                        claims: Vec::new(),
                    });
                    outcome.historical_events.push(event);
                }
                Err(error) => outcome.errors.push(error.to_string()),
            }
        }

        for party in defeated_parties {
            match self.history.defeat_regional_party(party) {
                Ok(event) => {
                    self.record_history_event(event);
                    self.campaign_start.journal.entries.push(JournalEntry {
                        title: "Regional encounter".to_string(),
                        body: self.history.world().events()[&event].summary.clone(),
                        learned_at: self.history.world().date,
                        source: JournalSource::Combat(event),
                        events: vec![event],
                        claims: Vec::new(),
                    });
                    outcome.historical_events.push(event);
                }
                Err(error) => outcome.errors.push(error.to_string()),
            }
        }
        if !outcome.historical_events.is_empty() {
            self.site_plan
                .synchronize_regional_parties(self.history.world(), &mut self.simulation);
        }

        if completed_dungeon && !self.progress.recorded_dungeon_clear {
            match self.history.record_dungeon_cleared_with_item(
                self.site_plan.dungeon.related_event,
                &self.site_plan.dungeon.name,
                self.site_plan.dungeon.world_item,
            ) {
                Ok(event) => {
                    self.progress.recorded_dungeon_clear = true;
                    self.record_history_event(event);
                    self.campaign_start.journal.entries.push(JournalEntry {
                        title: format!("Cleared {}", self.site_plan.dungeon.name),
                        body: self.history.world().events()[&event].summary.clone(),
                        learned_at: self.history.world().date,
                        source: JournalSource::Quest(event),
                        events: vec![event, self.site_plan.dungeon.related_event],
                        claims: Vec::new(),
                    });
                    outcome.historical_events.push(event);
                }
                Err(error) => outcome.errors.push(error.to_string()),
            }
        }
    }

    fn commit_social_consequences(&mut self, command: GameCommand, outcome: &mut CampaignOutcome) {
        let player = self.simulation.player_id();
        let transferred = |item: ItemId, from: EntityId, to: EntityId| {
            outcome.simulation.events.iter().any(|event| {
                matches!(
                    event,
                    SimulationEvent::ItemTransferred {
                        item: moved,
                        from: source,
                        to: destination,
                    } if *moved == item && *source == from && *destination == to
                )
            })
        };
        let (person_entity, amount, reason) = match command {
            GameCommand::Give { item, to } if transferred(item, player, to) => {
                (to, 1, "received a voluntary gift")
            }
            GameCommand::Take { item, from }
                if transferred(item, from, player) && self.simulation.is_stolen(item) =>
            {
                if !self.progress.witnessed_thefts.insert(item) {
                    return;
                }
                (from, -10, "witnessed the outsider take another's property")
            }
            _ => return,
        };
        let Some(resident) = self
            .site_plan
            .residents
            .iter()
            .find(|resident| resident.entity == person_entity)
        else {
            return;
        };
        let standing = self
            .progress
            .faction_standing
            .entry(resident.faction)
            .or_default();
        *standing = standing.saturating_add(amount).clamp(-100, 100);
        if amount > 0 {
            self.simulation.record_social_practice();
        }
        outcome
            .campaign_events
            .push(CampaignEvent::StandingChanged {
                faction: resident.faction,
                amount,
                reason: reason.to_string(),
            });
    }

    fn commit_aid_consequences(&mut self, outcome: &mut CampaignOutcome) {
        if self.progress.aid_aftermath_event.is_some() {
            return;
        }
        let Some((item, method)) = self.aid_delivery() else {
            return;
        };
        let aid = self.site_plan.aid.clone();
        match self.history.record_aid_delivered(
            aid.cause,
            aid.patient,
            aid.custodian,
            aid.advocate,
            aid.restricting_law,
            method,
        ) {
            Ok(event) => {
                self.progress.aid_aftermath_event = Some(event);
                self.record_history_event(event);
                let summary = self.history.world().events()[&event].summary.clone();
                self.campaign_start
                    .journal
                    .knowledge
                    .known_events
                    .insert(event);
                self.campaign_start.journal.entries.push(JournalEntry {
                    title: aid.title,
                    body: summary,
                    learned_at: self.history.world().date,
                    source: JournalSource::Resolution(event),
                    events: vec![event, aid.cause],
                    claims: Vec::new(),
                });
                outcome.historical_events.push(event);
                outcome.campaign_events.push(CampaignEvent::AidDelivered {
                    patient: aid.patient,
                    item,
                    method,
                    event,
                });
            }
            Err(error) => outcome.errors.push(error.to_string()),
        }
    }

    fn advance_resident_agents(&mut self) -> usize {
        let tick = self.simulation.tick;
        let settlement = self
            .history
            .world()
            .regional_settlements()
            .get(&self.site_plan.site);
        let shortage = settlement.is_some_and(|settlement| settlement.shortage);
        let unrest = settlement.map_or(0, |settlement| settlement.unrest);
        let food_available =
            self.history.world().sites()[&self.site_plan.site].resources[&ResourceKind::Food] > 0;
        let nearby_hostiles = self
            .simulation
            .entities()
            .filter(|entity| entity.position.map == self.site_plan.map)
            .filter(|entity| {
                self.simulation
                    .combatant(entity.id)
                    .is_some_and(|combatant| combatant.hostile_to_player && combatant.is_alive())
            })
            .count() as u8;
        let mut activities = BTreeMap::new();

        for resident in &self.site_plan.residents {
            let scheduled = self
                .site_plan
                .resident_activity(resident.person, tick)
                .map(|(activity, _)| activity)
                .unwrap_or(ResidentActivity::AtHome);
            let drives = self.history.world().people()[&resident.person]
                .drives
                .clone();
            let Some(agent) = self.resident_agents.get_mut(&resident.person) else {
                continue;
            };
            if tick.is_multiple_of(8) {
                agent.hunger = agent
                    .hunger
                    .saturating_add(if shortage { 2 } else { 1 })
                    .min(100);
            }
            if tick.is_multiple_of(6) && agent.goal != ResidentGoal::Rest {
                agent.fatigue = agent.fatigue.saturating_add(1).min(100);
            }
            if tick.is_multiple_of(12) && agent.goal != ResidentGoal::Socialize {
                agent.isolation = agent.isolation.saturating_add(1).min(100);
            }
            if tick.is_multiple_of(24) {
                let pressure = unrest / 4 + nearby_hostiles.saturating_mul(8);
                if agent.fear < pressure {
                    agent.fear = agent.fear.saturating_add(2).min(pressure);
                } else {
                    agent.fear = agent.fear.saturating_sub(1);
                }
            }

            let selected = choose_resident_goal(agent, &drives, scheduled, shortage);
            if selected != agent.goal {
                agent.goal = selected;
                agent.goal_since = tick;
            }
            activities.insert(resident.person, agent.goal.activity());
        }

        let moved = self
            .site_plan
            .advance_resident_goals(tick, &activities, &mut self.simulation);

        for resident in &self.site_plan.residents {
            let Some(agent) = self.resident_agents.get_mut(&resident.person) else {
                continue;
            };
            let Some(destination) = self
                .site_plan
                .resident_destination(resident.person, agent.goal.activity())
            else {
                continue;
            };
            let arrived = self
                .simulation
                .entity(resident.entity)
                .is_some_and(|entity| entity.position.grid == destination);
            if !arrived {
                continue;
            }
            let acted = match agent.goal {
                ResidentGoal::SeekFood if food_available => {
                    agent.hunger = agent.hunger.saturating_sub(5);
                    true
                }
                ResidentGoal::SeekSafety => {
                    agent.fear = agent.fear.saturating_sub(4);
                    true
                }
                ResidentGoal::Rest => {
                    agent.fatigue = agent.fatigue.saturating_sub(4);
                    true
                }
                ResidentGoal::Socialize => {
                    agent.isolation = agent.isolation.saturating_sub(4);
                    true
                }
                ResidentGoal::Work => agent.fatigue < 95,
                ResidentGoal::SeekFood => {
                    agent.fear = agent.fear.saturating_add(1).min(100);
                    false
                }
            };
            if acted && tick.is_multiple_of(4) {
                agent.completed_actions = agent.completed_actions.saturating_add(1);
            }
        }
        moved
    }

    fn advance_regional_parties(&mut self, outcome: &mut CampaignOutcome) {
        while self.last_regional_party_turn < self.simulation.tick {
            self.last_regional_party_turn = self.last_regional_party_turn.saturating_add(1);
            match self.history.advance_regional_parties_one_tile() {
                Ok(events) => outcome.historical_events.extend(events),
                Err(error) => outcome.errors.push(error.to_string()),
            }
            match self.history.pulse_living_simulation() {
                Ok(events) => outcome.historical_events.extend(events),
                Err(error) => outcome.errors.push(error.to_string()),
            }
        }
        self.site_plan
            .synchronize_regional_parties(self.history.world(), &mut self.simulation);
    }

    fn advance_calendar(&mut self, outcome: &mut CampaignOutcome) {
        while self.simulation.tick >= self.next_living_month_turn {
            match self.history.advance_month() {
                Ok(month) => {
                    self.site_plan
                        .synchronize_living_projects(self.history.world(), &mut self.simulation);
                    self.site_plan
                        .synchronize_regional_routes(self.history.world(), &mut self.simulation);
                    self.site_plan
                        .synchronize_regional_parties(self.history.world(), &mut self.simulation);
                    self.record_public_month(&month);
                    outcome
                        .historical_events
                        .extend(month.events.iter().copied());
                    outcome.month_summaries.push(month);
                }
                Err(error) => outcome.errors.push(error.to_string()),
            }
            self.next_living_month_turn = self
                .next_living_month_turn
                .saturating_add(LIVING_MONTH_TURNS);
        }
    }

    fn record_public_month(&mut self, month: &MonthSummary) {
        let public_events = month
            .events
            .iter()
            .copied()
            .filter(|event| direct_world_alert(self.history.world().events()[event].kind))
            .collect::<Vec<_>>();
        let Some(first_event) = public_events.first().copied() else {
            return;
        };
        self.campaign_start
            .journal
            .knowledge
            .known_events
            .extend(public_events.iter().copied());
        let summaries = public_events
            .iter()
            .map(|event| self.history.world().events()[event].summary.clone())
            .collect::<Vec<_>>();
        let mut body = summaries
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let omitted = summaries.len().saturating_sub(3);
        if omitted > 0 {
            body.push_str(&format!("\n{omitted} other development(s) recorded."));
        }
        self.campaign_start.journal.entries.push(JournalEntry {
            title: format!(
                "World developments — Year {}, Month {}",
                month.date.year, month.date.month
            ),
            body,
            learned_at: month.date,
            source: JournalSource::WorldChange(first_event),
            events: public_events,
            claims: Vec::new(),
        });
    }

    fn record_history_event(&mut self, event: EventId) {
        self.campaign_start
            .journal
            .knowledge
            .known_events
            .insert(event);
        self.simulation.record_world_change();
    }

    fn regional_party_for_entity(&self, entity: EntityId) -> Option<PartyId> {
        self.history
            .world()
            .regional_parties()
            .keys()
            .copied()
            .find(|party| PlayableSitePlan::regional_party_entity(*party) == entity)
    }
}

fn direct_world_alert(kind: HistoricalEventKind) -> bool {
    matches!(
        kind,
        HistoricalEventKind::RegionalShortage
            | HistoricalEventKind::RegionalRecovery
            | HistoricalEventKind::RouteDisrupted
            | HistoricalEventKind::RouteReopened
            | HistoricalEventKind::RegionalGoalProposed
            | HistoricalEventKind::RegionalGoalResolved
            | HistoricalEventKind::RegionalPartyDefeated
            | HistoricalEventKind::ProjectCompleted
            | HistoricalEventKind::ProjectDamaged
            | HistoricalEventKind::ProjectRepaired
    )
}

fn choose_resident_goal(
    agent: &ResidentAgentState,
    drives: &BTreeMap<Drive, u8>,
    scheduled: ResidentActivity,
    shortage: bool,
) -> ResidentGoal {
    let survival = drive_weight(drives, Drive::Survival);
    let wealth = drive_weight(drives, Drive::Wealth);
    let status = drive_weight(drives, Drive::Status);
    let family = drive_weight(drives, Drive::Family);
    let loyalty = drive_weight(drives, Drive::Loyalty);
    let freedom = drive_weight(drives, Drive::Freedom);
    let work_schedule = i32::from(scheduled == ResidentActivity::Working) * 35;
    let rest_schedule = i32::from(scheduled == ResidentActivity::AtHome) * 28;
    let social_schedule = i32::from(scheduled == ResidentActivity::AtLeisure) * 28;
    let shortage_pressure = i32::from(shortage) * 18;
    let mut scores = [
        (
            ResidentGoal::Work,
            20 + (wealth + status) / 2 + work_schedule,
        ),
        (
            ResidentGoal::Socialize,
            i32::from(agent.isolation) * (50 + family + loyalty) / 100 + social_schedule,
        ),
        (
            ResidentGoal::Rest,
            i32::from(agent.fatigue) * (60 + survival) / 100 + rest_schedule,
        ),
        (
            ResidentGoal::SeekFood,
            i32::from(agent.hunger) * (70 + survival) / 100 + shortage_pressure,
        ),
        (
            ResidentGoal::SeekSafety,
            i32::from(agent.fear) * (65 + survival + loyalty / 2) / 100
                + i32::from(shortage) * (20 - freedom / 8).max(0),
        ),
    ];
    for (goal, score) in &mut scores {
        if *goal == agent.goal {
            *score += 10;
        }
    }
    scores
        .into_iter()
        .max_by_key(|(goal, score)| (*score, std::cmp::Reverse(*goal)))
        .map(|(goal, _)| goal)
        .unwrap_or(ResidentGoal::Work)
}

fn drive_weight(drives: &BTreeMap<Drive, u8>, drive: Drive) -> i32 {
    i32::from(drives.get(&drive).copied().unwrap_or(25))
}

fn rejected(reason: String) -> CampaignOutcome {
    CampaignOutcome {
        campaign_events: vec![CampaignEvent::ActionRejected(reason)],
        ..CampaignOutcome::default()
    }
}

fn encode_command(command: CampaignCommand) -> String {
    match command {
        CampaignCommand::Game(command) => match command {
            GameCommand::Move(direction) => {
                format!("GAME|MOVE|{}", encode_direction(direction))
            }
            GameCommand::Attack(entity) => format!("GAME|ATTACK|{}", entity.0),
            GameCommand::FireAt(entity) => format!("GAME|FIRE|{}", entity.0),
            GameCommand::Equip(item) => format!("GAME|EQUIP|{}", item.0),
            GameCommand::UseItem(item) => format!("GAME|USE|{}", item.0),
            GameCommand::Study(item) => format!("GAME|STUDY|{}", item.0),
            GameCommand::Give { item, to } => {
                format!("GAME|GIVE|{}|{}", item.0, to.0)
            }
            GameCommand::Take { item, from } => {
                format!("GAME|TAKE|{}|{}", item.0, from.0)
            }
            GameCommand::Experiment { first, second } => {
                format!("GAME|EXPERIMENT|{}|{}", first.0, second.0)
            }
            GameCommand::Cast { formula, target } => format!(
                "GAME|CAST|{}|{}",
                formula.0,
                target
                    .map(|entity| entity.0.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            GameCommand::Traverse => "GAME|TRAVERSE".to_string(),
            GameCommand::TurnInQuest(quest) => format!("GAME|TURN_IN|{}", quest.0),
            GameCommand::Wait => "GAME|WAIT".to_string(),
            GameCommand::Pause => "GAME|PAUSE".to_string(),
        },
        CampaignCommand::Interact => "INTERACT".to_string(),
        CampaignCommand::InspectHere => "INSPECT_HERE".to_string(),
        CampaignCommand::Talk { person, topic } => {
            format!("TALK|{}|{}", person.0, encode_topic(topic))
        }
        CampaignCommand::InspectEvidence(event) => {
            format!("INSPECT_EVIDENCE|{}", event.0)
        }
        CampaignCommand::InspectHistoricalSite(event) => {
            format!("INSPECT_SITE|{}", event.0)
        }
        CampaignCommand::ResolveCrisis(kind) => {
            format!("CRISIS|{}", encode_crisis_resolution(kind))
        }
        CampaignCommand::ResolveRegionalGoal { goal, approach } => {
            format!("REGIONAL|{}|{}", goal.0, encode_regional_approach(approach))
        }
    }
}

fn decode_command(line: &str) -> Result<CampaignCommand, String> {
    let fields = line.split('|').collect::<Vec<_>>();
    match fields.as_slice() {
        ["GAME", "MOVE", direction] => Ok(CampaignCommand::Game(GameCommand::Move(
            decode_direction(direction)?,
        ))),
        ["GAME", "ATTACK", entity] => Ok(CampaignCommand::Game(GameCommand::Attack(EntityId(
            parse_u64(entity, "entity id")?,
        )))),
        ["GAME", "FIRE", entity] => Ok(CampaignCommand::Game(GameCommand::FireAt(EntityId(
            parse_u64(entity, "entity id")?,
        )))),
        ["GAME", "EQUIP", item] => Ok(CampaignCommand::Game(GameCommand::Equip(ItemId(
            parse_u64(item, "item id")?,
        )))),
        ["GAME", "USE", item] => Ok(CampaignCommand::Game(GameCommand::UseItem(ItemId(
            parse_u64(item, "item id")?,
        )))),
        ["GAME", "STUDY", item] => Ok(CampaignCommand::Game(GameCommand::Study(ItemId(
            parse_u64(item, "item id")?,
        )))),
        ["GAME", "GIVE", item, to] => Ok(CampaignCommand::Game(GameCommand::Give {
            item: ItemId(parse_u64(item, "item id")?),
            to: EntityId(parse_u64(to, "recipient entity id")?),
        })),
        ["GAME", "TAKE", item, from] => Ok(CampaignCommand::Game(GameCommand::Take {
            item: ItemId(parse_u64(item, "item id")?),
            from: EntityId(parse_u64(from, "source entity id")?),
        })),
        ["GAME", "EXPERIMENT", first, second] => {
            Ok(CampaignCommand::Game(GameCommand::Experiment {
                first: ItemId(parse_u64(first, "first reagent item id")?),
                second: ItemId(parse_u64(second, "second reagent item id")?),
            }))
        }
        ["GAME", "CAST", formula, target] => {
            let target = if *target == "-" {
                None
            } else {
                Some(EntityId(parse_u64(target, "target entity id")?))
            };
            Ok(CampaignCommand::Game(GameCommand::Cast {
                formula: FormulaId(parse_u64(formula, "formula id")?),
                target,
            }))
        }
        ["GAME", "TRAVERSE"] => Ok(CampaignCommand::Game(GameCommand::Traverse)),
        ["GAME", "TURN_IN", quest] => Ok(CampaignCommand::Game(GameCommand::TurnInQuest(QuestId(
            parse_u64(quest, "quest id")?,
        )))),
        ["GAME", "WAIT"] => Ok(CampaignCommand::Game(GameCommand::Wait)),
        ["GAME", "PAUSE"] => Ok(CampaignCommand::Game(GameCommand::Pause)),
        ["INTERACT"] => Ok(CampaignCommand::Interact),
        ["INSPECT_HERE"] => Ok(CampaignCommand::InspectHere),
        ["TALK", person, topic] => Ok(CampaignCommand::Talk {
            person: PersonId(parse_u64(person, "person id")?),
            topic: decode_topic(topic)?,
        }),
        ["INSPECT_EVIDENCE", event] => Ok(CampaignCommand::InspectEvidence(EventId(parse_u64(
            event, "event id",
        )?))),
        ["INSPECT_SITE", event] => Ok(CampaignCommand::InspectHistoricalSite(EventId(parse_u64(
            event, "event id",
        )?))),
        ["CRISIS", kind] => Ok(CampaignCommand::ResolveCrisis(decode_crisis_resolution(
            kind,
        )?)),
        ["REGIONAL", goal, approach] => Ok(CampaignCommand::ResolveRegionalGoal {
            goal: GoalId(parse_u64(goal, "goal id")?),
            approach: decode_regional_approach(approach)?,
        }),
        _ => Err(format!("unrecognized command `{line}`")),
    }
}

fn encode_direction(direction: Direction) -> &'static str {
    match direction {
        Direction::North => "N",
        Direction::East => "E",
        Direction::South => "S",
        Direction::West => "W",
    }
}

fn decode_direction(value: &str) -> Result<Direction, String> {
    match value {
        "N" => Ok(Direction::North),
        "E" => Ok(Direction::East),
        "S" => Ok(Direction::South),
        "W" => Ok(Direction::West),
        _ => Err(format!("invalid direction `{value}`")),
    }
}

fn encode_topic(topic: ConversationTopicKind) -> String {
    match topic {
        ConversationTopicKind::Orientation => "ORIENTATION".to_string(),
        ConversationTopicKind::PresentCrisis => "CRISIS".to_string(),
        ConversationTopicKind::FactionView => "FACTION".to_string(),
        ConversationTopicKind::Claim(claim) => format!("CLAIM:{}", claim.0),
        ConversationTopicKind::Law(law) => format!("LAW:{}", law.0),
        ConversationTopicKind::Evidence(event) => format!("EVIDENCE:{}", event.0),
        ConversationTopicKind::Aftermath(event) => format!("AFTERMATH:{}", event.0),
        ConversationTopicKind::RequestAid(person) => format!("REQUEST_AID:{}", person.0),
        ConversationTopicKind::OfferPayment(person) => format!("PAY_AID:{}", person.0),
        ConversationTopicKind::OfferAid(person) => format!("OFFER_AID:{}", person.0),
        ConversationTopicKind::SupportAid(person) => format!("SUPPORT_AID:{}", person.0),
        ConversationTopicKind::TakeAid(person) => format!("TAKE_AID:{}", person.0),
    }
}

fn decode_topic(value: &str) -> Result<ConversationTopicKind, String> {
    match value {
        "ORIENTATION" => Ok(ConversationTopicKind::Orientation),
        "CRISIS" => Ok(ConversationTopicKind::PresentCrisis),
        "FACTION" => Ok(ConversationTopicKind::FactionView),
        _ => {
            let Some((kind, id)) = value.split_once(':') else {
                return Err(format!("invalid conversation topic `{value}`"));
            };
            let id = parse_u64(id, "conversation topic id")?;
            match kind {
                "CLAIM" => Ok(ConversationTopicKind::Claim(ClaimId(id))),
                "LAW" => Ok(ConversationTopicKind::Law(LawId(id))),
                "EVIDENCE" => Ok(ConversationTopicKind::Evidence(EventId(id))),
                "AFTERMATH" => Ok(ConversationTopicKind::Aftermath(EventId(id))),
                "REQUEST_AID" => Ok(ConversationTopicKind::RequestAid(PersonId(id))),
                "PAY_AID" => Ok(ConversationTopicKind::OfferPayment(PersonId(id))),
                "OFFER_AID" => Ok(ConversationTopicKind::OfferAid(PersonId(id))),
                "SUPPORT_AID" => Ok(ConversationTopicKind::SupportAid(PersonId(id))),
                "TAKE_AID" => Ok(ConversationTopicKind::TakeAid(PersonId(id))),
                _ => Err(format!("invalid conversation topic `{value}`")),
            }
        }
    }
}

fn encode_crisis_resolution(kind: CrisisResolutionKind) -> &'static str {
    match kind {
        CrisisResolutionKind::EnforceEmergencyLaw => "ENFORCE",
        CrisisResolutionKind::OpenPublicStores => "OPEN",
        CrisisResolutionKind::BrokerCompromise => "COMPROMISE",
    }
}

fn decode_crisis_resolution(value: &str) -> Result<CrisisResolutionKind, String> {
    match value {
        "ENFORCE" => Ok(CrisisResolutionKind::EnforceEmergencyLaw),
        "OPEN" => Ok(CrisisResolutionKind::OpenPublicStores),
        "COMPROMISE" => Ok(CrisisResolutionKind::BrokerCompromise),
        _ => Err(format!("invalid crisis resolution `{value}`")),
    }
}

fn encode_regional_approach(approach: RegionalGoalApproach) -> &'static str {
    match approach {
        RegionalGoalApproach::RestoreByForce => "FORCE",
        RegionalGoalApproach::NegotiatePassage => "NEGOTIATE",
        RegionalGoalApproach::ExploitDisruption => "EXPLOIT",
        RegionalGoalApproach::DeliverRelief => "RELIEF",
        RegionalGoalApproach::DivertShipment => "DIVERT",
        RegionalGoalApproach::EnforceRationing => "RATION",
    }
}

fn decode_regional_approach(value: &str) -> Result<RegionalGoalApproach, String> {
    match value {
        "FORCE" => Ok(RegionalGoalApproach::RestoreByForce),
        "NEGOTIATE" => Ok(RegionalGoalApproach::NegotiatePassage),
        "EXPLOIT" => Ok(RegionalGoalApproach::ExploitDisruption),
        "RELIEF" => Ok(RegionalGoalApproach::DeliverRelief),
        "DIVERT" => Ok(RegionalGoalApproach::DivertShipment),
        "RATION" => Ok(RegionalGoalApproach::EnforceRationing),
        _ => Err(format!("invalid regional goal approach `{value}`")),
    }
}

fn parse_u32(value: &str, label: &str) -> Result<u32, String> {
    value
        .parse()
        .map_err(|_| format!("{label} `{value}` is not a valid unsigned integer"))
}

fn parse_u64(value: &str, label: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("{label} `{value}` is not a valid unsigned integer"))
}

fn parse_usize(value: &str, label: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|_| format!("{label} `{value}` is not a valid unsigned integer"))
}

fn manhattan(first_x: i32, first_y: i32, second_x: i32, second_y: i32) -> i32 {
    (first_x - second_x).abs() + (first_y - second_y).abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ultimate_fate_core::{Direction, GameCommand};

    #[test]
    fn campaign_owns_one_ruleset_and_advances_all_world_layers() {
        let mut session = CampaignSession::new(0x55aa_2026).expect("campaign");
        assert_eq!(
            session.simulation().rules(),
            session.history().world().rules()
        );
        let date = session.history().world().date;
        for _ in 0..LIVING_MONTH_TURNS {
            session.apply_game_command(GameCommand::Wait);
        }
        assert_eq!(session.history().world().date, date.next_month());
        assert_eq!(session.simulation().tick, LIVING_MONTH_TURNS);
    }

    #[test]
    fn identical_semantic_commands_replay_the_complete_campaign() {
        let mut first = CampaignSession::new(77).expect("first");
        let mut second = CampaignSession::new(77).expect("second");
        let commands = [
            GameCommand::Move(Direction::North),
            GameCommand::Wait,
            GameCommand::Move(Direction::East),
            GameCommand::Wait,
        ];
        for command in commands {
            assert_eq!(
                first.apply_game_command(command),
                second.apply_game_command(command)
            );
        }
        assert_eq!(first.simulation(), second.simulation());
        assert_eq!(first.history().world(), second.history().world());
        assert_eq!(first.start(), second.start());
    }

    #[test]
    fn platform_host_cannot_advance_history_without_the_session() {
        let session = CampaignSession::with_history_years(12, 0).expect("campaign");
        assert_eq!(session.next_living_month_turn, LIVING_MONTH_TURNS);
        assert_eq!(session.last_regional_party_turn, 0);
    }

    #[test]
    fn versioned_save_replays_the_authoritative_campaign_exactly() {
        let mut original = CampaignSession::with_history_years(0x77_2026, 3).expect("campaign");
        let commands = [
            CampaignCommand::Game(GameCommand::Move(Direction::North)),
            CampaignCommand::Game(GameCommand::Wait),
            CampaignCommand::Game(GameCommand::Pause),
            CampaignCommand::InspectHere,
            CampaignCommand::Talk {
                person: original.site_plan.contact,
                topic: ConversationTopicKind::Orientation,
            },
        ];
        for command in commands {
            original.apply_command(command);
        }

        let save = original.save_to_string();
        let loaded = CampaignSession::load_from_str(&save).expect("load");

        assert_eq!(loaded.command_log(), original.command_log());
        assert_eq!(loaded.simulation(), original.simulation());
        assert_eq!(loaded.history().world(), original.history().world());
        assert_eq!(loaded.start(), original.start());
        assert_eq!(loaded.progress(), original.progress());
        assert_eq!(loaded.resident_agents(), original.resident_agents());
        assert_eq!(
            loaded.next_living_month_turn,
            original.next_living_month_turn
        );
        assert_eq!(
            loaded.last_regional_party_turn,
            original.last_regional_party_turn
        );
    }

    #[test]
    fn save_codec_covers_every_semantic_command_shape() {
        let commands = [
            CampaignCommand::Game(GameCommand::Move(Direction::West)),
            CampaignCommand::Game(GameCommand::Attack(EntityId(1))),
            CampaignCommand::Game(GameCommand::FireAt(EntityId(2))),
            CampaignCommand::Game(GameCommand::Equip(ItemId(3))),
            CampaignCommand::Game(GameCommand::UseItem(ItemId(4))),
            CampaignCommand::Game(GameCommand::Study(ItemId(5))),
            CampaignCommand::Game(GameCommand::Give {
                item: ItemId(21),
                to: EntityId(22),
            }),
            CampaignCommand::Game(GameCommand::Take {
                item: ItemId(23),
                from: EntityId(24),
            }),
            CampaignCommand::Game(GameCommand::Experiment {
                first: ItemId(25),
                second: ItemId(26),
            }),
            CampaignCommand::Game(GameCommand::Cast {
                formula: FormulaId(6),
                target: Some(EntityId(7)),
            }),
            CampaignCommand::Game(GameCommand::Cast {
                formula: FormulaId(8),
                target: None,
            }),
            CampaignCommand::Game(GameCommand::Traverse),
            CampaignCommand::Game(GameCommand::TurnInQuest(QuestId(9))),
            CampaignCommand::Game(GameCommand::Wait),
            CampaignCommand::Game(GameCommand::Pause),
            CampaignCommand::Interact,
            CampaignCommand::InspectHere,
            CampaignCommand::Talk {
                person: PersonId(10),
                topic: ConversationTopicKind::Claim(ClaimId(11)),
            },
            CampaignCommand::Talk {
                person: PersonId(12),
                topic: ConversationTopicKind::Law(LawId(13)),
            },
            CampaignCommand::Talk {
                person: PersonId(14),
                topic: ConversationTopicKind::Evidence(EventId(15)),
            },
            CampaignCommand::Talk {
                person: PersonId(16),
                topic: ConversationTopicKind::Aftermath(EventId(17)),
            },
            CampaignCommand::Talk {
                person: PersonId(18),
                topic: ConversationTopicKind::RequestAid(PersonId(19)),
            },
            CampaignCommand::Talk {
                person: PersonId(20),
                topic: ConversationTopicKind::OfferPayment(PersonId(21)),
            },
            CampaignCommand::Talk {
                person: PersonId(22),
                topic: ConversationTopicKind::OfferAid(PersonId(23)),
            },
            CampaignCommand::Talk {
                person: PersonId(24),
                topic: ConversationTopicKind::SupportAid(PersonId(25)),
            },
            CampaignCommand::Talk {
                person: PersonId(26),
                topic: ConversationTopicKind::TakeAid(PersonId(27)),
            },
            CampaignCommand::InspectEvidence(EventId(28)),
            CampaignCommand::InspectHistoricalSite(EventId(29)),
            CampaignCommand::ResolveCrisis(CrisisResolutionKind::BrokerCompromise),
            CampaignCommand::ResolveRegionalGoal {
                goal: GoalId(30),
                approach: RegionalGoalApproach::DeliverRelief,
            },
        ];

        for command in commands {
            let encoded = encode_command(command);
            assert_eq!(decode_command(&encoded), Ok(command), "{encoded}");
        }
    }

    #[test]
    fn save_loader_rejects_unknown_versions_and_truncated_logs() {
        let version_error = CampaignSession::load_from_str("ULTIMATE_FATE|999|1|0|0\n")
            .err()
            .expect("version");
        assert!(version_error.contains("unsupported"));

        let truncated = CampaignSession::load_from_str("ULTIMATE_FATE|1|1|0|1\n")
            .err()
            .expect("truncated");
        assert!(truncated.contains("declares 1 commands"));
    }

    #[test]
    fn situations_are_projections_of_authoritative_state() {
        let mut session = CampaignSession::with_history_years(91, 0).expect("campaign");
        let initial = session.situations();
        let crisis = initial
            .iter()
            .find(|situation| {
                situation.id == SituationId::LocalCrisis(session.site_plan.crisis_event)
            })
            .expect("local crisis");
        assert_eq!(crisis.status, SituationStatus::Available);
        assert!(
            crisis
                .conditions
                .iter()
                .all(|condition| !condition.satisfied())
        );

        session.progress.met_contact = true;
        let changed = session
            .situations()
            .into_iter()
            .find(|situation| {
                situation.id == SituationId::LocalCrisis(session.site_plan.crisis_event)
            })
            .expect("local crisis");
        assert_eq!(changed.status, SituationStatus::InProgress);
        assert!(changed.conditions[0].satisfied());
    }

    #[test]
    fn current_situations_do_not_dump_closed_historical_contracts() {
        let session = CampaignSession::new(0x55aa_2026).expect("campaign");
        let situations = session.situations();
        assert!(
            situations
                .iter()
                .all(|situation| situation.status != SituationStatus::ClosedByWorld)
        );
        let open_regional = session
            .history()
            .world()
            .regional_goals()
            .values()
            .filter(|goal| goal.status == RegionalGoalStatus::Open)
            .count();
        assert_eq!(situations.len(), open_regional + 3);
    }

    fn stand_with(session: &mut CampaignSession, entity: EntityId) {
        let target = session.simulation.entity(entity).expect("actor").position;
        let player = session.simulation.player_id();
        assert!(session.simulation.move_entity(player, target));
    }

    fn talk(
        session: &mut CampaignSession,
        person: PersonId,
        topic: ConversationTopicKind,
    ) -> CampaignOutcome {
        session.apply_command(CampaignCommand::Talk { person, topic })
    }

    #[test]
    fn aid_resolution_is_observed_from_material_state_across_distinct_routes() {
        let seed = 0x55aa_2026;
        for expected in [
            AidResolutionKind::ReleasedByConsent,
            AidResolutionKind::Purchased,
            AidResolutionKind::TakenWithoutConsent,
            AidResolutionKind::AlternativeTreatment,
        ] {
            let mut session = CampaignSession::new(seed).expect("campaign");
            let aid = session.site_plan.aid.clone();
            if expected == AidResolutionKind::ReleasedByConsent {
                stand_with(&mut session, aid.advocate_entity);
                talk(
                    &mut session,
                    aid.advocate,
                    ConversationTopicKind::SupportAid(aid.patient),
                );
            }
            if expected != AidResolutionKind::AlternativeTreatment {
                stand_with(&mut session, aid.custodian_entity);
                let topic = match expected {
                    AidResolutionKind::ReleasedByConsent => {
                        ConversationTopicKind::RequestAid(aid.patient)
                    }
                    AidResolutionKind::Purchased => {
                        ConversationTopicKind::OfferPayment(aid.patient)
                    }
                    AidResolutionKind::TakenWithoutConsent => {
                        ConversationTopicKind::TakeAid(aid.patient)
                    }
                    AidResolutionKind::AlternativeTreatment => unreachable!(),
                };
                talk(&mut session, aid.custodian, topic);
                assert!(
                    session
                        .simulation
                        .player_inventory()
                        .is_some_and(|inventory| inventory.items.contains(&aid.medicine.id))
                );
            }

            stand_with(&mut session, aid.patient_entity);
            let outcome = talk(
                &mut session,
                aid.patient,
                ConversationTopicKind::OfferAid(aid.patient),
            );
            let (delivered_item, actual) = session.aid_delivery().expect("material aid");
            assert_eq!(actual, expected);
            assert!(session.progress.aid_aftermath_event.is_some());
            assert!(outcome.campaign_events.iter().any(|event| {
                matches!(
                    event,
                    CampaignEvent::AidDelivered { method, .. } if *method == expected
                )
            }));
            assert_eq!(
                session
                    .situations()
                    .into_iter()
                    .find(|situation| situation.id == SituationId::AidAccess(aid.cause))
                    .map(|situation| situation.status),
                Some(SituationStatus::Resolved)
            );
            if expected == AidResolutionKind::TakenWithoutConsent {
                assert!(session.simulation.is_stolen(delivered_item));
                assert_eq!(
                    session.simulation.legal_owner(delivered_item),
                    Some(aid.custodian_entity)
                );
            } else {
                assert!(!session.simulation.is_stolen(delivered_item));
                assert_eq!(
                    session.simulation.legal_owner(delivered_item),
                    Some(aid.patient_entity)
                );
            }
        }
    }

    #[test]
    fn resident_agents_deviate_from_timetables_for_persistent_needs() {
        let mut session = CampaignSession::with_history_years(123, 0).expect("campaign");
        let resident = session
            .site_plan
            .residents
            .iter()
            .find(|resident| {
                session
                    .site_plan
                    .resident_destination(resident.person, ResidentActivity::SeekingFood)
                    != Some(resident.position)
            })
            .expect("resident away from food")
            .clone();
        let person = resident.person;
        let entity = resident.entity;
        let starting_position = session
            .simulation
            .entity(entity)
            .expect("resident")
            .position;
        let agent = session.resident_agents.get_mut(&person).expect("agent");
        agent.hunger = 100;
        agent.fatigue = 0;
        agent.fear = 0;
        agent.isolation = 0;

        session.apply_game_command(GameCommand::Wait);
        assert_eq!(
            session.resident_agents[&person].goal,
            ResidentGoal::SeekFood
        );
        for _ in 1..24 {
            session.apply_game_command(GameCommand::Wait);
        }
        assert_ne!(
            session
                .simulation
                .entity(entity)
                .expect("resident")
                .position,
            starting_position
        );
    }
}
