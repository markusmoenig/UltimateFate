//! Development-only observation, control, tracing, and automated play tools.
//!
//! The line protocol deliberately accepts both compact shell commands and a
//! small JSON command shape. Every response is one JSON object, making a
//! persistent process easy to drive from Codex, scripts, or a human terminal.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::Write as _,
    ops::{Deref, DerefMut},
};

use ultimate_fate_content::{FormulaCondition, FormulaId};
use ultimate_fate_core::{
    CommandOutcome, Direction, EntityId, EntityKind, GameCommand, GridPos, ItemKind, QuestStatus,
    Simulation, SimulationEvent, TerrainKind, WorldPosition,
};
use ultimate_fate_history::{
    AidResolutionKind, CrisisResolutionOutcome, FactionId, GoalId, HistoricalEventKind,
    HistoryEngine, RegionalGoalApproach, RegionalGoalKind, RegionalGoalStatus, RegionalPartyStatus,
    StrategicFront,
};
use ultimate_fate_session::{CampaignCommand, CampaignEvent, CampaignOutcome, CampaignSession};
use ultimate_fate_text::{ConversationContext, ConversationTopic, ConversationTopicKind};
use ultimate_fate_worldgen::PlayableSitePlan;

pub const DEFAULT_SEED: u64 = 0x55aa_2026;
pub const DEFAULT_HISTORY_YEARS: u32 = 20;
pub use ultimate_fate_session::LIVING_MONTH_TURNS;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LocalObjectiveProgress {
    pub met_contact: bool,
    pub inspected_evidence: bool,
    pub questioned_factions: usize,
    pub crisis_resolved: bool,
    pub aftermath_complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LabCommand {
    Help,
    Reset {
        seed: u64,
    },
    Observe {
        radius: i32,
    },
    Metrics,
    World,
    Goals,
    Objectives,
    Move {
        direction: Direction,
        count: u32,
    },
    Wait {
        turns: u32,
    },
    Interact,
    Inspect,
    Study,
    Experiment {
        first: ultimate_fate_core::ItemId,
        second: ultimate_fate_core::ItemId,
    },
    Cast {
        formula: FormulaId,
    },
    Explore {
        turns: u32,
    },
    Goto {
        target: String,
        maximum_turns: u32,
    },
    PursueGoal {
        goal_index: usize,
        option_index: usize,
        maximum_turns: u32,
    },
    ResolveAid {
        approach: AidResolutionKind,
        maximum_turns: u32,
    },
    PlaySlice {
        resolution_index: usize,
        maximum_turns: u32,
    },
    Quit,
}

pub fn parse_command(line: &str) -> Result<LabCommand, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err("empty command".to_string());
    }
    if trimmed.starts_with('{') {
        return parse_json_command(trimmed);
    }
    let words = trimmed.split_whitespace().collect::<Vec<_>>();
    let number = |index: usize, default: u32| {
        words
            .get(index)
            .and_then(|word| word.parse::<u32>().ok())
            .unwrap_or(default)
    };
    match words[0].to_ascii_lowercase().as_str() {
        "help" => Ok(LabCommand::Help),
        "reset" => Ok(LabCommand::Reset {
            seed: words
                .get(1)
                .map_or(Ok(DEFAULT_SEED), |seed| parse_u64(seed))?,
        }),
        "observe" | "look" => Ok(LabCommand::Observe {
            radius: number(1, 12).clamp(2, 64) as i32,
        }),
        "metrics" => Ok(LabCommand::Metrics),
        "world" => Ok(LabCommand::World),
        "goals" => Ok(LabCommand::Goals),
        "objectives" | "quests" => Ok(LabCommand::Objectives),
        "move" => Ok(LabCommand::Move {
            direction: parse_direction(
                words
                    .get(1)
                    .ok_or_else(|| "move requires a direction".to_string())?,
            )?,
            count: number(2, 1).clamp(1, 10_000),
        }),
        "wait" => Ok(LabCommand::Wait {
            turns: number(1, 1).clamp(1, 100_000),
        }),
        "interact" | "act" => Ok(LabCommand::Interact),
        "inspect" => Ok(LabCommand::Inspect),
        "study" => Ok(LabCommand::Study),
        "experiment" => Ok(LabCommand::Experiment {
            first: ultimate_fate_core::ItemId(parse_u64(
                words
                    .get(1)
                    .ok_or_else(|| "experiment requires two item ids".to_string())?,
            )?),
            second: ultimate_fate_core::ItemId(parse_u64(
                words
                    .get(2)
                    .ok_or_else(|| "experiment requires two item ids".to_string())?,
            )?),
        }),
        "cast" => Ok(LabCommand::Cast {
            formula: FormulaId(number(1, 1) as u64),
        }),
        "explore" | "autoplay" => Ok(LabCommand::Explore {
            turns: number(1, 500).clamp(1, 1_000_000),
        }),
        "goto" => Ok(LabCommand::Goto {
            target: words
                .get(1..)
                .filter(|words| !words.is_empty())
                .ok_or_else(|| "goto requires a landmark name".to_string())?
                .join(" "),
            maximum_turns: 10_000,
        }),
        "pursue" => Ok(LabCommand::PursueGoal {
            goal_index: number(1, 0) as usize,
            option_index: number(2, 0) as usize,
            maximum_turns: 10_000,
        }),
        "aid" => Ok(LabCommand::ResolveAid {
            approach: parse_aid_approach(words.get(1).ok_or_else(|| {
                "aid requires consent, purchase, theft, or alternative".to_string()
            })?)?,
            maximum_turns: 10_000,
        }),
        "slice" | "campaign" => Ok(LabCommand::PlaySlice {
            resolution_index: number(1, 0) as usize,
            maximum_turns: 10_000,
        }),
        "quit" | "exit" => Ok(LabCommand::Quit),
        command => Err(format!("unknown command {command:?}")),
    }
}

fn parse_json_command(line: &str) -> Result<LabCommand, String> {
    let command = json_string_field(line, "command")
        .ok_or_else(|| "JSON command requires a string \"command\" field".to_string())?;
    let direction = json_string_field(line, "direction");
    let count = json_u64_field(line, "count").unwrap_or(1).clamp(1, 10_000) as u32;
    let turns = json_u64_field(line, "turns")
        .unwrap_or(500)
        .clamp(1, 1_000_000) as u32;
    match command.to_ascii_lowercase().as_str() {
        "help" => Ok(LabCommand::Help),
        "reset" => Ok(LabCommand::Reset {
            seed: json_u64_field(line, "seed").unwrap_or(DEFAULT_SEED),
        }),
        "observe" => Ok(LabCommand::Observe {
            radius: json_u64_field(line, "radius").unwrap_or(12).clamp(2, 64) as i32,
        }),
        "metrics" => Ok(LabCommand::Metrics),
        "world" => Ok(LabCommand::World),
        "goals" => Ok(LabCommand::Goals),
        "objectives" | "quests" => Ok(LabCommand::Objectives),
        "move" => Ok(LabCommand::Move {
            direction: parse_direction(
                direction
                    .as_deref()
                    .ok_or_else(|| "move requires \"direction\"".to_string())?,
            )?,
            count,
        }),
        "wait" => Ok(LabCommand::Wait { turns }),
        "interact" | "act" => Ok(LabCommand::Interact),
        "inspect" => Ok(LabCommand::Inspect),
        "study" => Ok(LabCommand::Study),
        "experiment" => Ok(LabCommand::Experiment {
            first: ultimate_fate_core::ItemId(
                json_u64_field(line, "first")
                    .ok_or_else(|| "experiment requires \"first\" item id".to_string())?,
            ),
            second: ultimate_fate_core::ItemId(
                json_u64_field(line, "second")
                    .ok_or_else(|| "experiment requires \"second\" item id".to_string())?,
            ),
        }),
        "cast" => Ok(LabCommand::Cast {
            formula: FormulaId(json_u64_field(line, "formula").unwrap_or(1)),
        }),
        "explore" | "autoplay" => Ok(LabCommand::Explore { turns }),
        "goto" => Ok(LabCommand::Goto {
            target: json_string_field(line, "target")
                .ok_or_else(|| "goto requires \"target\"".to_string())?,
            maximum_turns: json_u64_field(line, "maximum_turns")
                .unwrap_or(10_000)
                .clamp(1, 100_000) as u32,
        }),
        "pursue" => Ok(LabCommand::PursueGoal {
            goal_index: json_u64_field(line, "goal_index").unwrap_or(0) as usize,
            option_index: json_u64_field(line, "option_index").unwrap_or(0) as usize,
            maximum_turns: json_u64_field(line, "maximum_turns")
                .unwrap_or(10_000)
                .clamp(1, 100_000) as u32,
        }),
        "aid" => Ok(LabCommand::ResolveAid {
            approach: parse_aid_approach(
                json_string_field(line, "approach")
                    .as_deref()
                    .ok_or_else(|| "aid requires string \"approach\"".to_string())?,
            )?,
            maximum_turns: json_u64_field(line, "maximum_turns")
                .unwrap_or(10_000)
                .clamp(1, 100_000) as u32,
        }),
        "slice" | "campaign" => Ok(LabCommand::PlaySlice {
            resolution_index: json_u64_field(line, "resolution_index").unwrap_or(0) as usize,
            maximum_turns: json_u64_field(line, "maximum_turns")
                .unwrap_or(10_000)
                .clamp(1, 100_000) as u32,
        }),
        "quit" => Ok(LabCommand::Quit),
        unknown => Err(format!("unknown JSON command {unknown:?}")),
    }
}

fn json_string_field(input: &str, field: &str) -> Option<String> {
    let marker = format!("\"{field}\"");
    let rest = input.split_once(&marker)?.1;
    let rest = rest.split_once(':')?.1.trim_start();
    let value = rest.strip_prefix('"')?;
    let end = value.find('"')?;
    Some(value[..end].to_string())
}

fn json_u64_field(input: &str, field: &str) -> Option<u64> {
    let marker = format!("\"{field}\"");
    let rest = input.split_once(&marker)?.1;
    let rest = rest.split_once(':')?.1.trim_start();
    let digits = rest
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

fn parse_u64(input: &str) -> Result<u64, String> {
    if let Some(hex) = input.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).map_err(|error| error.to_string())
    } else {
        input.parse::<u64>().map_err(|error| error.to_string())
    }
}

fn parse_direction(input: &str) -> Result<Direction, String> {
    match input.to_ascii_lowercase().as_str() {
        "n" | "north" | "up" => Ok(Direction::North),
        "e" | "east" | "right" => Ok(Direction::East),
        "s" | "south" | "down" => Ok(Direction::South),
        "w" | "west" | "left" => Ok(Direction::West),
        _ => Err(format!("invalid direction {input:?}")),
    }
}

fn parse_aid_approach(input: &str) -> Result<AidResolutionKind, String> {
    match input.to_ascii_lowercase().as_str() {
        "consent" | "appeal" | "persuade" => Ok(AidResolutionKind::ReleasedByConsent),
        "purchase" | "buy" => Ok(AidResolutionKind::Purchased),
        "theft" | "steal" | "take" => Ok(AidResolutionKind::TakenWithoutConsent),
        "alternative" | "treatment" => Ok(AidResolutionKind::AlternativeTreatment),
        _ => Err(format!(
            "invalid aid approach {input:?}; expected consent, purchase, theft, or alternative"
        )),
    }
}

#[derive(Clone, Debug)]
pub struct ExperienceTracker {
    commands: u64,
    time_advancing_turns: u64,
    successful_moves: u64,
    blocked_moves: u64,
    meaningful_events: u64,
    landmark_discoveries: u64,
    combat_events: u64,
    transitions: u64,
    local_npc_moves: u64,
    quiet_turns: u64,
    longest_quiet_stretch: u64,
    historical_events_at_start: usize,
    visited: BTreeSet<WorldPosition>,
    terrain_seen: BTreeSet<TerrainKind>,
    discovered_landmarks: BTreeSet<(WorldPosition, String)>,
    last_position: WorldPosition,
}

impl ExperienceTracker {
    pub fn new(simulation: &Simulation, historical_events: usize) -> Self {
        let last_position = simulation.player().position;
        let mut visited = BTreeSet::new();
        visited.insert(last_position);
        let mut tracker = Self {
            commands: 0,
            time_advancing_turns: 0,
            successful_moves: 0,
            blocked_moves: 0,
            meaningful_events: 0,
            landmark_discoveries: 0,
            combat_events: 0,
            transitions: 0,
            local_npc_moves: 0,
            quiet_turns: 0,
            longest_quiet_stretch: 0,
            historical_events_at_start: historical_events,
            visited,
            terrain_seen: BTreeSet::new(),
            discovered_landmarks: simulation
                .landmarks()
                .filter(|landmark| landmark.position == simulation.player().position)
                .map(|landmark| (landmark.position, landmark.name.clone()))
                .collect(),
            last_position,
        };
        tracker.observe_terrain(simulation);
        tracker
    }

    pub fn record(&mut self, simulation: &Simulation, outcome: &CommandOutcome) {
        self.commands += 1;
        self.time_advancing_turns += u64::from(outcome.advanced_time);
        let position = simulation.player().position;
        if position != self.last_position {
            self.successful_moves += 1;
            self.visited.insert(position);
        } else if !outcome.changed_world && !outcome.advanced_time && outcome.events.is_empty() {
            self.blocked_moves += 1;
        }
        let discovered = simulation
            .landmarks()
            .filter(|landmark| landmark.position == position)
            .filter(|landmark| {
                self.discovered_landmarks
                    .insert((landmark.position, landmark.name.clone()))
            })
            .count() as u64;
        self.landmark_discoveries += discovered;
        let meaningful = !outcome.events.is_empty() || discovered > 0;
        if meaningful {
            self.meaningful_events += 1;
            self.quiet_turns = 0;
        } else if outcome.advanced_time {
            self.quiet_turns += 1;
            self.longest_quiet_stretch = self.longest_quiet_stretch.max(self.quiet_turns);
        }
        for event in &outcome.events {
            match event {
                SimulationEvent::Damaged { .. } | SimulationEvent::Defeated { .. } => {
                    self.combat_events += 1;
                }
                SimulationEvent::Traversed { .. } => self.transitions += 1,
                _ => {}
            }
        }
        self.last_position = position;
        self.observe_terrain(simulation);
    }

    pub fn has_visited(&self, position: WorldPosition) -> bool {
        self.visited.contains(&position)
    }

    pub fn has_discovered_landmark(&self, position: WorldPosition, name: &str) -> bool {
        self.discovered_landmarks
            .contains(&(position, name.to_string()))
    }

    pub fn record_local_npc_moves(&mut self, count: usize) {
        self.local_npc_moves = self.local_npc_moves.saturating_add(count as u64);
    }

    pub fn metrics_json(
        &self,
        simulation: &Simulation,
        history: &HistoryEngine,
        journal_entries: usize,
    ) -> String {
        let decision_density = if self.time_advancing_turns == 0 {
            0.0
        } else {
            self.meaningful_events as f64 * 100.0 / self.time_advancing_turns as f64
        };
        let blocked_ratio = if self.commands == 0 {
            0.0
        } else {
            self.blocked_moves as f64 / self.commands as f64
        };
        let player = simulation.player();
        let nearby_entities = simulation
            .entities()
            .filter(|entity| {
                entity.id != player.id
                    && entity.position.map == player.position.map
                    && entity.position.grid.z == player.position.grid.z
                    && grid_distance(entity.position.grid, player.position.grid) <= 10
            })
            .count();
        let historical_growth = history
            .world()
            .events()
            .len()
            .saturating_sub(self.historical_events_at_start);
        let traveling_parties = history
            .world()
            .regional_parties()
            .values()
            .filter(|party| party.status == RegionalPartyStatus::Traveling)
            .count();
        let active_parties = history
            .world()
            .regional_parties()
            .values()
            .filter(|party| {
                matches!(
                    party.status,
                    RegionalPartyStatus::Traveling | RegionalPartyStatus::Stationed
                )
            })
            .count();
        format!(
            concat!(
                "{{\"ok\":true,\"type\":\"metrics\",\"commands\":{},",
                "\"turns\":{},\"successful_moves\":{},\"blocked_moves\":{},",
                "\"blocked_ratio\":{:.3},\"meaningful_events\":{},",
                "\"decision_events_per_100_turns\":{:.2},\"landmark_discoveries\":{},",
                "\"combat_events\":{},",
                "\"map_transitions\":{},\"local_npc_moves\":{},\"longest_quiet_stretch\":{},",
                "\"unique_cells_visited\":{},\"terrain_types_seen\":{},",
                "\"nearby_entities\":{},\"traveling_parties\":{},\"active_parties\":{},",
                "\"historical_events_added\":{},",
                "\"journal_entries\":{}}}"
            ),
            self.commands,
            self.time_advancing_turns,
            self.successful_moves,
            self.blocked_moves,
            blocked_ratio,
            self.meaningful_events,
            decision_density,
            self.landmark_discoveries,
            self.combat_events,
            self.transitions,
            self.local_npc_moves,
            self.longest_quiet_stretch,
            self.visited.len(),
            self.terrain_seen.len(),
            nearby_entities,
            traveling_parties,
            active_parties,
            historical_growth,
            journal_entries,
        )
    }

    fn observe_terrain(&mut self, simulation: &Simulation) {
        let player = simulation.player().position;
        let Some(map) = simulation.map(player.map) else {
            return;
        };
        for dy in -8..=8 {
            for dx in -8..=8 {
                if let Some(cell) = map.cell(player.grid.offset(dx, dy, 0)) {
                    self.terrain_seen.insert(cell.terrain);
                }
            }
        }
    }
}

pub struct LabSession {
    campaign: CampaignSession,
    pub messages: Vec<String>,
    pub tracker: ExperienceTracker,
    exploration_cursor: usize,
    exploration_path: VecDeque<Direction>,
    met_contact: bool,
    inspected_evidence: bool,
    questioned_factions: BTreeSet<FactionId>,
    resolved_crisis: Option<CrisisResolutionOutcome>,
    aftermath_complete: bool,
}

impl Deref for LabSession {
    type Target = CampaignSession;

    fn deref(&self) -> &Self::Target {
        &self.campaign
    }
}

impl DerefMut for LabSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.campaign
    }
}

impl LabSession {
    pub fn new(seed: u64) -> Result<Self, String> {
        let campaign = CampaignSession::with_history_years(seed, DEFAULT_HISTORY_YEARS)?;
        let tracker = ExperienceTracker::new(
            campaign.simulation(),
            campaign.history().world().events().len(),
        );
        Ok(Self {
            campaign,
            messages: vec!["Headless campaign ready.".to_string()],
            tracker,
            exploration_cursor: 0,
            exploration_path: VecDeque::new(),
            met_contact: false,
            inspected_evidence: false,
            questioned_factions: BTreeSet::new(),
            resolved_crisis: None,
            aftermath_complete: false,
        })
    }

    pub fn execute(&mut self, command: LabCommand) -> String {
        match command {
            LabCommand::Help => help_json(),
            LabCommand::Reset { seed } => match Self::new(seed) {
                Ok(session) => {
                    *self = session;
                    format!("{{\"ok\":true,\"type\":\"reset\",\"seed\":{seed}}}")
                }
                Err(error) => error_json(&error),
            },
            LabCommand::Observe { radius } => observation_json(
                self.campaign.simulation(),
                self.campaign.history(),
                self.campaign.site_plan(),
                &self.messages,
                radius,
            ),
            LabCommand::Metrics => self.tracker.metrics_json(
                self.campaign.simulation(),
                self.campaign.history(),
                self.campaign.start().journal.entries.len(),
            ),
            LabCommand::World => world_json(self.campaign.history(), self.campaign.site_plan()),
            LabCommand::Goals => goals_json(
                self.campaign.history(),
                self.campaign.site_plan(),
                self.campaign.simulation(),
            ),
            LabCommand::Objectives => self.objectives_json(),
            LabCommand::Move { direction, count } => {
                for _ in 0..count {
                    self.apply(GameCommand::Move(direction));
                }
                observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.messages,
                    12,
                )
            }
            LabCommand::Wait { turns } => {
                for _ in 0..turns {
                    self.apply(GameCommand::Wait);
                }
                self.tracker.metrics_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.start().journal.entries.len(),
                )
            }
            LabCommand::Interact => {
                self.apply_campaign(CampaignCommand::Interact);
                observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.messages,
                    12,
                )
            }
            LabCommand::Inspect => {
                self.inspect_context();
                observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.messages,
                    12,
                )
            }
            LabCommand::Study => {
                let artifact =
                    self.campaign
                        .simulation()
                        .player_inventory()
                        .and_then(|inventory| {
                            inventory.items.iter().copied().find(|item| {
                                self.campaign.simulation().item(*item).is_some_and(|item| {
                                    matches!(item.kind, ItemKind::InscribedArtifact { .. })
                                })
                            })
                        });
                if let Some(artifact) = artifact {
                    self.apply(GameCommand::Study(artifact));
                } else {
                    self.push_message("No carried inscription can be studied.");
                }
                observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.messages,
                    12,
                )
            }
            LabCommand::Experiment { first, second } => {
                self.apply(GameCommand::Experiment { first, second });
                observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.messages,
                    12,
                )
            }
            LabCommand::Cast { formula } => {
                self.apply(GameCommand::Cast {
                    formula,
                    target: None,
                });
                observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.messages,
                    12,
                )
            }
            LabCommand::Explore { turns } => {
                for _ in 0..turns {
                    if let Some(target) = self.campaign.simulation().hostile_in_melee_range() {
                        self.apply(GameCommand::Attack(target));
                    } else if let Some(target) = self.campaign.simulation().hostile_in_ranged_line()
                    {
                        self.apply(GameCommand::FireAt(target));
                    } else {
                        let direction = self.exploration_direction();
                        self.apply(GameCommand::Move(direction));
                    }
                }
                self.tracker.metrics_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.start().journal.entries.len(),
                )
            }
            LabCommand::Goto {
                target,
                maximum_turns,
            } => {
                let Some(path) = path_to_landmark(self.campaign.simulation(), &target) else {
                    return error_json(&format!("no reachable landmark matching {target:?}"));
                };
                for direction in path.into_iter().take(maximum_turns as usize) {
                    self.apply(GameCommand::Move(direction));
                }
                observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.messages,
                    12,
                )
            }
            LabCommand::PursueGoal {
                goal_index,
                option_index,
                maximum_turns,
            } => match self.pursue_goal(goal_index, option_index, maximum_turns) {
                Ok(()) => observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.messages,
                    12,
                ),
                Err(error) => error_json(&error),
            },
            LabCommand::ResolveAid {
                approach,
                maximum_turns,
            } => match self.resolve_aid(approach, maximum_turns) {
                Ok(summary) => summary,
                Err(error) => error_json(&error),
            },
            LabCommand::PlaySlice {
                resolution_index,
                maximum_turns,
            } => match self.play_slice(resolution_index, maximum_turns) {
                Ok(summary) => summary,
                Err(error) => error_json(&error),
            },
            LabCommand::Quit => "{\"ok\":true,\"type\":\"quit\"}".to_string(),
        }
    }

    fn exploration_direction(&mut self) -> Direction {
        if let Some(direction) = self.exploration_path.pop_front() {
            return direction;
        }
        self.exploration_path = path_to_unvisited(self.campaign.simulation(), &self.tracker).into();
        if let Some(direction) = self.exploration_path.pop_front() {
            return direction;
        }
        let directions = Direction::ALL;
        let direction = directions[self.exploration_cursor % directions.len()];
        self.exploration_cursor = (self.exploration_cursor + 1) % directions.len();
        direction
    }

    fn apply(&mut self, command: GameCommand) {
        self.apply_campaign(CampaignCommand::Game(command));
    }

    fn apply_campaign(&mut self, command: CampaignCommand) -> CampaignOutcome {
        let outcome = self.campaign.apply_command(command);
        self.tracker
            .record(self.campaign.simulation(), &outcome.simulation);
        self.tracker.record_local_npc_moves(outcome.resident_moves);
        for event in &outcome.simulation.events {
            self.push_message(event_summary(event));
        }
        for event in &outcome.historical_events {
            let record = &self.campaign.history().world().events()[event];
            let summary = matches!(
                record.kind,
                HistoricalEventKind::ArtifactRecovered
                    | HistoricalEventKind::FormulaReconstructed
                    | HistoricalEventKind::DungeonCleared
                    | HistoricalEventKind::CareDelivered
                    | HistoricalEventKind::RegionalPartyDefeated
            )
            .then(|| record.summary.clone());
            if let Some(summary) = summary {
                self.push_message(summary);
            }
        }
        for error in &outcome.errors {
            self.push_message(format!("Campaign error: {error}"));
        }
        for event in &outcome.campaign_events {
            match event {
                CampaignEvent::Conversation { topic, .. } => {
                    self.push_message(topic.response.clone());
                }
                CampaignEvent::EvidenceInspected {
                    name, description, ..
                } => self.push_message(format!("Examined {name}: {description}.")),
                CampaignEvent::CrisisResolved(resolution) => {
                    self.push_message(format!("Crisis resolution: {}.", resolution.summary));
                }
                CampaignEvent::RegionalGoalResolved(resolution) => {
                    self.push_message(format!("Regional outcome: {}.", resolution.summary));
                }
                CampaignEvent::ActionRejected(reason) => {
                    self.push_message(format!("Action rejected: {reason}."));
                }
                CampaignEvent::StandingChanged { amount, reason, .. } => {
                    self.push_message(format!("Local standing {:+}: {reason}.", amount))
                }
                CampaignEvent::AidSupported { advocate, patient } => self.push_message(format!(
                    "Person {} supported aid for person {}.",
                    advocate.0, patient.0
                )),
                CampaignEvent::ItemAcquired { item, from, method } => self.push_message(format!(
                    "Item {} acquired from person {} by {method:?}.",
                    item.0, from.0
                )),
                CampaignEvent::AidDelivered {
                    patient, method, ..
                } => self.push_message(format!(
                    "Care delivered to person {} by {method:?}.",
                    patient.0
                )),
                CampaignEvent::ItemGifted { .. } => {}
            }
        }
        self.met_contact = self.campaign.progress().met_contact;
        self.inspected_evidence = self.campaign.progress().inspected_evidence;
        self.questioned_factions = self.campaign.progress().questioned_factions.clone();
        self.resolved_crisis = self.campaign.progress().resolved_crisis.clone();
        self.aftermath_complete = self.campaign.progress().aftermath_complete;
        outcome
    }

    fn pursue_goal(
        &mut self,
        goal_index: usize,
        option_index: usize,
        maximum_turns: u32,
    ) -> Result<(), String> {
        let goal_id = open_goal_ids(self.campaign.history())
            .get(goal_index)
            .copied()
            .ok_or_else(|| format!("no open regional goal at index {goal_index}"))?;
        let goal = self.campaign.history().world().regional_goals()[&goal_id].clone();
        let option = self.campaign.history()
            .regional_goal_options(goal_id)
            .map_err(|error| error.to_string())?
            .get(option_index)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "goal {goal_index} has no option at index {option_index}; use goals to inspect it"
                )
            })?;
        let mut turns = 0_u32;

        if self.campaign.simulation().player().position.map
            != self.campaign.site_plan().regional_map
        {
            let path = path_to_landmark(self.campaign.simulation(), "world road gate")
                .ok_or_else(|| "the world road gate is not reachable from town".to_string())?;
            for direction in path {
                self.apply(GameCommand::Move(direction));
                turns += 1;
                if turns >= maximum_turns {
                    return Err(format!(
                        "turn limit reached while leaving town for {}",
                        goal.title
                    ));
                }
            }
            if self.campaign.simulation().player().position.map
                != self.campaign.site_plan().regional_map
            {
                self.apply(GameCommand::Traverse);
                turns += 1;
            }
        }

        if let RegionalGoalKind::SecureRoute(route) = goal.kind
            && option.approach == RegionalGoalApproach::RestoreByForce
        {
            while let Some(party) = self
                .campaign
                .history()
                .active_route_raiders(route)
                .first()
                .copied()
            {
                if turns >= maximum_turns {
                    return Err(format!(
                        "turn limit reached while pursuing raiders for {}",
                        goal.title
                    ));
                }
                let entity = PlayableSitePlan::regional_party_entity(party);
                let target = self
                    .campaign
                    .simulation()
                    .entity(entity)
                    .map(|entity| entity.position)
                    .ok_or_else(|| {
                        "an active raider party was missing from the regional simulation"
                            .to_string()
                    })?;
                let player = self.campaign.simulation().player().position;
                let command = if player.map == target.map
                    && taxicab_distance(player.grid, target.grid) <= 1
                {
                    GameCommand::Attack(entity)
                } else if self.campaign.simulation().hostile_in_ranged_line() == Some(entity) {
                    GameCommand::FireAt(entity)
                } else {
                    let targets = BTreeSet::from([target.grid]);
                    let direction = path_to_targets(self.campaign.simulation(), &targets)
                        .and_then(|path| path.first().copied())
                        .ok_or_else(|| {
                            format!(
                                "no route to raider party {}",
                                self.campaign.history().world().regional_parties()[&party].name
                            )
                        })?;
                    GameCommand::Move(direction)
                };
                self.apply(command);
                turns += 1;
                if self
                    .campaign
                    .simulation()
                    .player_combatant()
                    .is_some_and(|combatant| combatant.health <= 0)
                {
                    return Err(format!(
                        "the player was defeated while pursuing {}",
                        goal.title
                    ));
                }
            }
        }

        let (target_name, target) = self
            .campaign
            .site_plan()
            .regional_goal_target(goal.kind)
            .map(|(name, target)| (name.to_string(), target))
            .ok_or_else(|| format!("{} has no physical regional target", goal.title))?;
        while taxicab_distance(self.campaign.simulation().player().position.grid, target) > 2 {
            if turns >= maximum_turns {
                return Err(format!(
                    "turn limit reached before arriving at {target_name} for {}",
                    goal.title
                ));
            }
            let targets = BTreeSet::from([target]);
            let direction = path_to_targets(self.campaign.simulation(), &targets)
                .and_then(|path| path.first().copied())
                .ok_or_else(|| format!("{target_name} is not reachable"))?;
            self.apply(GameCommand::Move(direction));
            turns += 1;
        }

        let campaign_outcome = self.apply_campaign(CampaignCommand::ResolveRegionalGoal {
            goal: goal_id,
            approach: option.approach,
        });
        let outcome = campaign_outcome
            .campaign_events
            .iter()
            .find_map(|event| match event {
                CampaignEvent::RegionalGoalResolved(outcome) => Some(outcome),
                _ => None,
            })
            .ok_or_else(|| campaign_rejection(&campaign_outcome))?;
        self.push_message(format!(
            "Regional outcome after {turns} turns: {}.",
            outcome.summary
        ));
        Ok(())
    }

    fn resolve_aid(
        &mut self,
        approach: AidResolutionKind,
        maximum_turns: u32,
    ) -> Result<String, String> {
        if let Some((item, method)) = self.campaign.aid_delivery() {
            return Ok(format!(
                "{{\"ok\":true,\"type\":\"aid\",\"already_resolved\":true,\"item\":{},\"approach\":\"{:?}\"}}",
                item.0, method
            ));
        }
        let starting_tick = self.campaign.simulation().tick;
        let starting_events = self.campaign.history().world().events().len();
        let starting_journal = self.campaign.start().journal.entries.len();
        let aid = self.campaign.site_plan().aid.clone();

        if approach == AidResolutionKind::ReleasedByConsent {
            self.navigate_adjacent_to_entity(aid.advocate_entity, starting_tick, maximum_turns)?;
            let support = self.apply_campaign(CampaignCommand::Talk {
                person: aid.advocate,
                topic: ConversationTopicKind::SupportAid(aid.patient),
            });
            if !support.campaign_events.iter().any(|event| {
                matches!(event, CampaignEvent::AidSupported { advocate, .. } if *advocate == aid.advocate)
            }) {
                return Err(campaign_rejection(&support));
            }
        }

        if approach != AidResolutionKind::AlternativeTreatment {
            self.navigate_adjacent_to_entity(aid.custodian_entity, starting_tick, maximum_turns)?;
            let topic = match approach {
                AidResolutionKind::ReleasedByConsent => {
                    ConversationTopicKind::RequestAid(aid.patient)
                }
                AidResolutionKind::Purchased => ConversationTopicKind::OfferPayment(aid.patient),
                AidResolutionKind::TakenWithoutConsent => {
                    ConversationTopicKind::TakeAid(aid.patient)
                }
                AidResolutionKind::AlternativeTreatment => unreachable!(),
            };
            let acquisition = self.apply_campaign(CampaignCommand::Talk {
                person: aid.custodian,
                topic,
            });
            let acquired = self
                .campaign
                .simulation()
                .player_inventory()
                .is_some_and(|inventory| inventory.items.contains(&aid.medicine.id));
            if !acquired {
                return Err(campaign_rejection(&acquisition));
            }
        }

        self.navigate_adjacent_to_entity(aid.patient_entity, starting_tick, maximum_turns)?;
        let delivery = self.apply_campaign(CampaignCommand::Talk {
            person: aid.patient,
            topic: ConversationTopicKind::OfferAid(aid.patient),
        });
        let Some((item, actual_method)) = self.campaign.aid_delivery() else {
            return Err(campaign_rejection(&delivery));
        };
        if actual_method != approach {
            return Err(format!(
                "requested {approach:?} but authoritative state recorded {actual_method:?}"
            ));
        }
        let aftermath = self
            .campaign
            .progress()
            .aid_aftermath_event
            .ok_or_else(|| "care reached the patient without a historical aftermath".to_string())?;
        Ok(format!(
            concat!(
                "{{\"ok\":true,\"type\":\"aid\",\"approach\":\"{:?}\",\"item\":{},",
                "\"turns\":{},\"historical_event\":{},\"events_added\":{},",
                "\"journal_entries_added\":{},\"stolen\":{},\"player_coin\":{}}}"
            ),
            actual_method,
            item.0,
            self.campaign
                .simulation()
                .tick
                .saturating_sub(starting_tick),
            aftermath.0,
            self.campaign
                .history()
                .world()
                .events()
                .len()
                .saturating_sub(starting_events),
            self.campaign
                .start()
                .journal
                .entries
                .len()
                .saturating_sub(starting_journal),
            self.campaign.simulation().is_stolen(item),
            self.campaign.progress().player_coin
        ))
    }

    fn objectives_json(&self) -> String {
        let legacy = local_objectives_json(
            self.campaign.simulation(),
            self.campaign.history(),
            self.campaign.site_plan(),
            LocalObjectiveProgress {
                met_contact: self.met_contact,
                inspected_evidence: self.inspected_evidence,
                questioned_factions: self.questioned_factions.len(),
                crisis_resolved: self.resolved_crisis.is_some(),
                aftermath_complete: self.aftermath_complete,
            },
        );
        let situations = self
            .campaign
            .situations()
            .into_iter()
            .map(|situation| {
                let conditions = situation
                    .conditions
                    .iter()
                    .map(|condition| {
                        format!(
                            "{{\"kind\":\"{}\",\"satisfied\":{}}}",
                            json_escape(&format!("{condition:?}")),
                            condition.satisfied()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let approaches = situation
                    .approaches
                    .iter()
                    .map(|approach| format!("\"{}\"", json_escape(approach)))
                    .collect::<Vec<_>>()
                    .join(",");
                let actors = situation
                    .actors
                    .iter()
                    .map(|person| person.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    concat!(
                        "{{\"id\":\"{:?}\",\"title\":\"{}\",\"status\":\"{:?}\",",
                        "\"cause\":{},\"actors\":[{}],\"conditions\":[{}],\"approaches\":[{}]}}"
                    ),
                    situation.id,
                    json_escape(&situation.title),
                    situation.status,
                    situation.cause.0,
                    actors,
                    conditions,
                    approaches
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let resident_agents = self
            .campaign
            .resident_agents()
            .values()
            .map(|agent| {
                format!(
                    concat!(
                        "{{\"person\":{},\"goal\":\"{:?}\",\"hunger\":{},",
                        "\"fatigue\":{},\"fear\":{},\"isolation\":{},\"actions\":{}}}"
                    ),
                    agent.person.0,
                    agent.goal,
                    agent.hunger,
                    agent.fatigue,
                    agent.fear,
                    agent.isolation,
                    agent.completed_actions
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{},\"situations\":[{}],\"resident_agents\":[{}]}}",
            legacy.strip_suffix('}').unwrap_or(&legacy),
            situations,
            resident_agents
        )
    }

    fn play_slice(
        &mut self,
        resolution_index: usize,
        maximum_turns: u32,
    ) -> Result<String, String> {
        let starting_tick = self.campaign.simulation().tick;
        let starting_events = self.campaign.history().world().events().len();
        let starting_journal = self.campaign.start().journal.entries.len();

        self.meet_arrival_contact(starting_tick, maximum_turns)
            .map_err(|error| format!("arrival: {error}"))?;
        self.examine_local_evidence(starting_tick, maximum_turns)
            .map_err(|error| format!("evidence: {error}"))?;
        self.question_two_factions(starting_tick, maximum_turns)
            .map_err(|error| format!("faction accounts: {error}"))?;
        self.clear_history_dungeon(starting_tick, maximum_turns)
            .map_err(|error| format!("dungeon: {error}"))?;
        self.study_and_use_recovered_formula(starting_tick, maximum_turns)
            .map_err(|error| format!("magic discovery: {error}"))?;
        self.turn_in_dungeon_quest(starting_tick, maximum_turns)
            .map_err(|error| format!("turn in: {error}"))?;
        self.resolve_local_crisis(resolution_index)?;
        self.record_crisis_aftermath(starting_tick, maximum_turns)
            .map_err(|error| format!("aftermath: {error}"))?;

        let quest = self
            .campaign
            .simulation()
            .quest(self.campaign.site_plan().dungeon.quest)
            .expect("dungeon quest");
        if quest.status != QuestStatus::Completed
            || self.resolved_crisis.is_none()
            || !self.aftermath_complete
        {
            return Err(
                "the campaign slice stopped before its authoritative aftermath".to_string(),
            );
        }
        let outcome = self.resolved_crisis.as_ref().expect("checked above");
        let combatant = self
            .campaign
            .simulation()
            .player_combatant()
            .expect("player combatant");
        let progression = self.campaign.simulation().progression();
        Ok(format!(
            concat!(
                "{{\"ok\":true,\"type\":\"slice\",\"seed\":{},\"turns\":{},",
                "\"quest\":\"Completed\",\"resolution\":\"{:?}\",",
                "\"aftermath_complete\":true,\"health\":{},\"max_health\":{},",
                "\"level\":{},\"experience\":{},\"journal_entries_added\":{},",
                "\"historical_events_added\":{},\"summary\":\"{}\"}}"
            ),
            self.campaign.simulation().campaign_seed,
            self.campaign
                .simulation()
                .tick
                .saturating_sub(starting_tick),
            outcome.kind,
            combatant.health,
            combatant.max_health,
            progression.level,
            progression.experience,
            self.campaign
                .start()
                .journal
                .entries
                .len()
                .saturating_sub(starting_journal),
            self.campaign
                .history()
                .world()
                .events()
                .len()
                .saturating_sub(starting_events),
            json_escape(&outcome.summary)
        ))
    }

    fn meet_arrival_contact(
        &mut self,
        starting_tick: u64,
        maximum_turns: u32,
    ) -> Result<(), String> {
        if self.met_contact {
            return Ok(());
        }
        let resident = self.campaign.site_plan().contact_resident().clone();
        self.navigate_adjacent_to_entity(resident.entity, starting_tick, maximum_turns)?;
        let conversation = self
            .campaign
            .conversation_for_person(resident.person, &ConversationContext::default())?;
        let topic = conversation
            .topics
            .iter()
            .find(|topic| topic.kind == ConversationTopicKind::Orientation)
            .cloned()
            .ok_or_else(|| "arrival contact had no orientation topic".to_string())?;
        let outcome = self.apply_campaign(CampaignCommand::Talk {
            person: resident.person,
            topic: topic.kind,
        });
        if !outcome.campaign_events.iter().any(|event| {
            matches!(
                event,
                CampaignEvent::Conversation { speaker, .. } if *speaker == resident.person
            )
        }) {
            return Err(campaign_rejection(&outcome));
        }
        Ok(())
    }

    fn examine_local_evidence(
        &mut self,
        starting_tick: u64,
        maximum_turns: u32,
    ) -> Result<(), String> {
        if self.inspected_evidence {
            return Ok(());
        }
        let evidence = self.campaign.site_plan().evidence_location().clone();
        self.navigate_to_position(evidence.position, 1, starting_tick, maximum_turns)?;
        let outcome = self.apply_campaign(CampaignCommand::InspectEvidence(
            self.campaign.site_plan().evidence_event,
        ));
        if !outcome
            .campaign_events
            .iter()
            .any(|event| matches!(event, CampaignEvent::EvidenceInspected { .. }))
        {
            return Err(campaign_rejection(&outcome));
        }
        Ok(())
    }

    fn question_two_factions(
        &mut self,
        starting_tick: u64,
        maximum_turns: u32,
    ) -> Result<(), String> {
        let candidates = self
            .campaign
            .site_plan()
            .residents
            .iter()
            .filter(|resident| !self.questioned_factions.contains(&resident.faction))
            .fold(
                BTreeMap::<FactionId, _>::new(),
                |mut candidates, resident| {
                    candidates
                        .entry(resident.faction)
                        .or_insert_with(|| resident.clone());
                    candidates
                },
            )
            .into_values()
            .collect::<Vec<_>>();
        for resident in candidates {
            if self.questioned_factions.len() >= 2 {
                break;
            }
            self.navigate_adjacent_to_entity(resident.entity, starting_tick, maximum_turns)?;
            let mut context = ConversationContext::default();
            context
                .examined_evidence
                .insert(self.campaign.site_plan().evidence_event);
            let conversation = self
                .campaign
                .conversation_for_person(resident.person, &context)?;
            let topic = conversation
                .topics
                .iter()
                .find(|topic| {
                    topic.kind
                        == ConversationTopicKind::Evidence(self.campaign.site_plan().evidence_event)
                })
                .cloned()
                .ok_or_else(|| format!("{} had no evidence account", resident.name))?;
            self.record_conversation_topic(resident.person, &conversation.speaker_name, &topic);
            self.questioned_factions.insert(resident.faction);
        }
        if self.questioned_factions.len() < 2 {
            return Err("fewer than two generated factions could be questioned".to_string());
        }
        Ok(())
    }

    fn clear_history_dungeon(
        &mut self,
        starting_tick: u64,
        maximum_turns: u32,
    ) -> Result<(), String> {
        if self
            .campaign
            .simulation()
            .quest(self.campaign.site_plan().dungeon.quest)
            .is_some_and(|quest| quest.status != QuestStatus::Active)
        {
            return Ok(());
        }
        self.navigate_to_position(
            self.campaign.site_plan().dungeon.entrance,
            0,
            starting_tick,
            maximum_turns,
        )?;
        self.apply_counted(GameCommand::Traverse, starting_tick, maximum_turns)?;

        let levels = self.campaign.site_plan().dungeon.levels.clone();
        for level in &levels {
            if level.depth == 3 {
                self.defeat_entity(
                    self.campaign.site_plan().dungeon.boss,
                    true,
                    starting_tick,
                    maximum_turns,
                )?;
            } else {
                let descent = level
                    .descent
                    .ok_or_else(|| format!("{} has no descent", level.name))?;
                self.navigate_to_position(descent, 0, starting_tick, maximum_turns)?;
                self.apply_counted(GameCommand::Traverse, starting_tick, maximum_turns)?;
            }
        }
        if self
            .campaign
            .simulation()
            .quest(self.campaign.site_plan().dungeon.quest)
            .is_none_or(|quest| quest.status != QuestStatus::ReadyToTurnIn)
        {
            return Err(format!(
                "{} was defeated but its artifact quest is not ready",
                self.campaign.site_plan().dungeon.boss_name
            ));
        }

        for level in levels.iter().rev() {
            self.navigate_to_position(level.entry, 0, starting_tick, maximum_turns)?;
            self.apply_counted(GameCommand::Traverse, starting_tick, maximum_turns)?;
        }
        Ok(())
    }

    fn turn_in_dungeon_quest(
        &mut self,
        starting_tick: u64,
        maximum_turns: u32,
    ) -> Result<(), String> {
        if self
            .campaign
            .simulation()
            .quest(self.campaign.site_plan().dungeon.quest)
            .is_some_and(|quest| quest.status == QuestStatus::Completed)
        {
            return Ok(());
        }
        let giver = self.campaign.site_plan().contact_resident().entity;
        self.navigate_adjacent_to_entity(giver, starting_tick, maximum_turns)?;
        self.apply(GameCommand::TurnInQuest(
            self.campaign.site_plan().dungeon.quest,
        ));
        if self
            .campaign
            .simulation()
            .quest(self.campaign.site_plan().dungeon.quest)
            .is_none_or(|quest| quest.status != QuestStatus::Completed)
        {
            return Err("the recovered artifact could not be returned to its giver".to_string());
        }
        Ok(())
    }

    fn study_and_use_recovered_formula(
        &mut self,
        starting_tick: u64,
        maximum_turns: u32,
    ) -> Result<(), String> {
        let (artifact, formula_id) = self
            .campaign
            .simulation()
            .player_inventory()
            .and_then(|inventory| {
                inventory.items.iter().find_map(|item_id| {
                    self.campaign
                        .simulation()
                        .item(*item_id)
                        .and_then(|item| match item.kind {
                            ItemKind::InscribedArtifact { formula, .. } => {
                                Some((*item_id, formula))
                            }
                            _ => None,
                        })
                })
            })
            .ok_or_else(|| "the recovered object was not in player custody".to_string())?;
        self.apply(GameCommand::Study(artifact));
        if !self
            .campaign
            .simulation()
            .known_formulas()
            .contains(&formula_id)
        {
            return Err("studying the inscription did not reconstruct its formula".to_string());
        }

        let formula = self
            .campaign
            .simulation()
            .rules()
            .formula(formula_id)
            .cloned()
            .ok_or_else(|| "the inscription referenced no world formula".to_string())?;
        match formula.condition {
            FormulaCondition::CleanWater => {
                let player = self.campaign.simulation().player().position;
                let targets = self
                    .campaign
                    .simulation()
                    .map(player.map)
                    .into_iter()
                    .flat_map(|map| map.cells())
                    .filter(|(position, cell)| {
                        position.z == player.grid.z
                            && !cell.movement_blocked
                            && Direction::ALL.into_iter().any(|direction| {
                                let (dx, dy) = direction.delta();
                                self.campaign
                                    .simulation()
                                    .map(player.map)
                                    .and_then(|map| map.cell(position.offset(dx, dy, 0)))
                                    .is_some_and(|cell| {
                                        matches!(
                                            cell.terrain,
                                            TerrainKind::Water | TerrainKind::Ocean
                                        )
                                    })
                            })
                    })
                    .map(|(position, _)| position)
                    .collect::<BTreeSet<_>>();
                let path = path_to_targets(self.campaign.simulation(), &targets)
                    .ok_or_else(|| "no reachable clean water exists on this map".to_string())?;
                for direction in path {
                    self.apply_counted(GameCommand::Move(direction), starting_tick, maximum_turns)?;
                }
            }
            FormulaCondition::NightSky => {
                while self.campaign.simulation().tick % 24 < 12 {
                    self.apply_counted(GameCommand::Wait, starting_tick, maximum_turns)?;
                }
            }
            FormulaCondition::DirectSunlight => {
                while self.campaign.simulation().tick % 24 >= 12 {
                    self.apply_counted(GameCommand::Wait, starting_tick, maximum_turns)?;
                }
            }
            FormulaCondition::ExistingFlame => {}
        }

        let reagents_before = self.carried_reagent_quantity();
        self.apply(GameCommand::Cast {
            formula: formula_id,
            target: None,
        });
        let reagents_after = self.carried_reagent_quantity();
        if reagents_before.saturating_sub(reagents_after) != formula.reagents.len() as u32 {
            return Err(format!(
                "{} was learned but could not be performed under its world condition",
                formula.name
            ));
        }
        Ok(())
    }

    fn carried_reagent_quantity(&self) -> u32 {
        self.campaign
            .simulation()
            .player_inventory()
            .into_iter()
            .flat_map(|inventory| inventory.items.iter())
            .filter_map(|item| self.campaign.simulation().item(*item))
            .filter(|item| matches!(item.kind, ItemKind::Reagent { .. }))
            .map(|item| u32::from(item.quantity))
            .sum()
    }

    fn resolve_local_crisis(&mut self, resolution_index: usize) -> Result<(), String> {
        if self.resolved_crisis.is_some() {
            return Ok(());
        }
        if !self.inspected_evidence || self.questioned_factions.len() < 2 {
            return Err(
                "the crisis cannot be judged before evidence and competing accounts".into(),
            );
        }
        let option = self
            .campaign
            .history()
            .crisis_resolution_options(self.campaign.site_plan().crisis_event)
            .map_err(|error| error.to_string())?
            .get(resolution_index)
            .cloned()
            .ok_or_else(|| format!("no crisis resolution at index {resolution_index}"))?;
        let campaign_outcome = self.apply_campaign(CampaignCommand::ResolveCrisis(option.kind));
        if !campaign_outcome
            .campaign_events
            .iter()
            .any(|event| matches!(event, CampaignEvent::CrisisResolved(_)))
        {
            return Err(campaign_rejection(&campaign_outcome));
        }
        Ok(())
    }

    fn record_crisis_aftermath(
        &mut self,
        starting_tick: u64,
        maximum_turns: u32,
    ) -> Result<(), String> {
        if self.aftermath_complete {
            return Ok(());
        }
        let outcome = self
            .resolved_crisis
            .clone()
            .ok_or_else(|| "the crisis has no resolution to react to".to_string())?;
        let resident = self
            .campaign
            .site_plan()
            .residents
            .iter()
            .find(|resident| resident.faction == outcome.reaction_faction)
            .cloned()
            .ok_or_else(|| "the reacting faction has no materialized resident".to_string())?;
        self.navigate_adjacent_to_entity(resident.entity, starting_tick, maximum_turns)?;
        let conversation = self
            .campaign
            .conversation_for_person(resident.person, &ConversationContext::default())?;
        let topic = conversation
            .topics
            .iter()
            .find(|topic| topic.kind == ConversationTopicKind::Aftermath(outcome.event))
            .cloned()
            .ok_or_else(|| format!("{} had no aftermath response", resident.name))?;
        self.record_conversation_topic(resident.person, &conversation.speaker_name, &topic);
        self.aftermath_complete = true;
        self.push_message(format!(
            "Recorded {}'s faction response to the intervention.",
            resident.name
        ));
        Ok(())
    }

    fn record_conversation_topic(
        &mut self,
        speaker: ultimate_fate_history::PersonId,
        _speaker_name: &str,
        topic: &ConversationTopic,
    ) {
        self.apply_campaign(CampaignCommand::Talk {
            person: speaker,
            topic: topic.kind,
        });
    }

    fn navigate_adjacent_to_entity(
        &mut self,
        entity: EntityId,
        starting_tick: u64,
        maximum_turns: u32,
    ) -> Result<(), String> {
        loop {
            let target = self
                .campaign
                .simulation()
                .entity(entity)
                .map(|entity| entity.position)
                .ok_or_else(|| format!("entity {} is not materialized", entity.0))?;
            let player = self.campaign.simulation().player().position;
            if player.map == target.map
                && player.grid.z == target.grid.z
                && taxicab_distance(player.grid, target.grid) <= 1
            {
                return Ok(());
            }
            let pursuing_hostile = {
                self.campaign
                    .simulation()
                    .entities()
                    .filter(|candidate| {
                        candidate.position.map == player.map
                            && candidate.position.grid.z == player.grid.z
                            && taxicab_distance(player.grid, candidate.position.grid) <= 2
                            && self
                                .campaign
                                .simulation()
                                .combatant(candidate.id)
                                .is_some_and(|combatant| {
                                    combatant.hostile_to_player && combatant.is_alive()
                                })
                    })
                    .map(|candidate| candidate.id)
                    .next()
            };
            if let Some(pursuing_hostile) = pursuing_hostile {
                self.defeat_entity(pursuing_hostile, false, starting_tick, maximum_turns)?;
                continue;
            }
            let direction = path_to_position_avoiding_hostiles(
                self.campaign.simulation(),
                target.grid,
                1,
                None,
            )
            .and_then(|path| path.first().copied())
            .ok_or_else(|| format!("no safe path to entity {}", entity.0))?;
            self.apply_counted(GameCommand::Move(direction), starting_tick, maximum_turns)?;
        }
    }

    fn navigate_to_position(
        &mut self,
        target: GridPos,
        threshold: i32,
        starting_tick: u64,
        maximum_turns: u32,
    ) -> Result<(), String> {
        loop {
            let player = self.campaign.simulation().player().position;
            if player.grid.z == target.z && taxicab_distance(player.grid, target) <= threshold {
                return Ok(());
            }
            let adjacent_hostile = {
                self.campaign
                    .simulation()
                    .entities()
                    .filter(|entity| {
                        entity.position.map == player.map
                            && entity.position.grid.z == player.grid.z
                            && taxicab_distance(player.grid, entity.position.grid) <= 2
                            && self.campaign.simulation().combatant(entity.id).is_some_and(
                                |combatant| combatant.hostile_to_player && combatant.is_alive(),
                            )
                    })
                    .map(|entity| entity.id)
                    .next()
            };
            if let Some(hostile) = adjacent_hostile {
                self.defeat_entity(hostile, false, starting_tick, maximum_turns)?;
                continue;
            }
            let path = path_to_position_avoiding_hostiles(
                self.campaign.simulation(),
                target,
                threshold,
                None,
            );
            if let Some(direction) = path.and_then(|path| path.first().copied()) {
                self.apply_counted(GameCommand::Move(direction), starting_tick, maximum_turns)?;
                continue;
            }
            let hostile = self
                .campaign
                .simulation()
                .entities()
                .filter(|entity| {
                    entity.position.map == player.map
                        && entity.position.grid.z == player.grid.z
                        && self.campaign.simulation().combatant(entity.id).is_some_and(
                            |combatant| combatant.hostile_to_player && combatant.is_alive(),
                        )
                })
                .min_by_key(|entity| (grid_distance(player.grid, entity.position.grid), entity.id))
                .map(|entity| entity.id)
                .ok_or_else(|| format!("no path to {},{}", target.x, target.y))?;
            self.defeat_entity(hostile, false, starting_tick, maximum_turns)?;
        }
    }

    fn defeat_entity(
        &mut self,
        entity: EntityId,
        preserve_ranged_for_target: bool,
        starting_tick: u64,
        maximum_turns: u32,
    ) -> Result<(), String> {
        while self
            .campaign
            .simulation()
            .combatant(entity)
            .is_some_and(|combatant| combatant.is_alive())
        {
            self.heal_if_needed();
            if self
                .campaign
                .simulation()
                .player_combatant()
                .is_some_and(|combatant| !combatant.is_alive())
            {
                return Err(format!("the player was defeated by entity {}", entity.0));
            }
            let player = self.campaign.simulation().player().position;
            let interfering_hostile = {
                self.campaign
                    .simulation()
                    .entities()
                    .filter(|candidate| candidate.id != entity)
                    .filter(|candidate| {
                        candidate.position.map == player.map
                            && candidate.position.grid.z == player.grid.z
                            && taxicab_distance(player.grid, candidate.position.grid) <= 2
                            && self
                                .campaign
                                .simulation()
                                .combatant(candidate.id)
                                .is_some_and(|combatant| {
                                    combatant.hostile_to_player && combatant.is_alive()
                                })
                    })
                    .map(|candidate| candidate.id)
                    .next()
            };
            if let Some(interfering_hostile) = interfering_hostile {
                self.defeat_entity(interfering_hostile, false, starting_tick, maximum_turns)?;
                continue;
            }
            let target = self
                .campaign
                .simulation()
                .entity(entity)
                .map(|target| target.position)
                .ok_or_else(|| format!("combat target {} vanished", entity.0))?;
            let command = if preserve_ranged_for_target
                && self
                    .campaign
                    .simulation()
                    .check_ranged_attack(self.campaign.simulation().player_id(), entity)
                    .is_ok()
            {
                GameCommand::FireAt(entity)
            } else if taxicab_distance(player.grid, target.grid) <= 1 {
                GameCommand::Attack(entity)
            } else {
                let direction = path_to_position_avoiding_hostiles(
                    self.campaign.simulation(),
                    target.grid,
                    1,
                    Some(entity),
                )
                .or_else(|| path_to_position(self.campaign.simulation(), target.grid))
                .and_then(|path| path.first().copied())
                .ok_or_else(|| format!("no combat path to entity {}", entity.0))?;
                GameCommand::Move(direction)
            };
            self.apply_counted(command, starting_tick, maximum_turns)?;
        }
        Ok(())
    }

    fn heal_if_needed(&mut self) {
        if self
            .campaign
            .simulation()
            .player_combatant()
            .is_none_or(|combatant| combatant.health > 4)
        {
            return;
        }
        let consumable = self
            .campaign
            .simulation()
            .player_inventory()
            .into_iter()
            .flat_map(|inventory| inventory.items.iter().copied())
            .find(|item| {
                self.campaign.simulation().item(*item).is_some_and(|item| {
                    matches!(item.kind, ItemKind::Consumable { .. }) && item.quantity > 0
                })
            });
        if let Some(item) = consumable {
            self.apply(GameCommand::UseItem(item));
        }
    }

    fn apply_counted(
        &mut self,
        command: GameCommand,
        starting_tick: u64,
        maximum_turns: u32,
    ) -> Result<(), String> {
        if self
            .campaign
            .simulation()
            .tick
            .saturating_sub(starting_tick)
            >= u64::from(maximum_turns)
        {
            return Err(format!("campaign slice exceeded {maximum_turns} turns"));
        }
        let before = self.campaign.simulation().tick;
        self.apply(command);
        if self.campaign.simulation().tick == before {
            return Err("a required campaign action did not advance or change state".to_string());
        }
        Ok(())
    }

    fn push_message(&mut self, message: impl Into<String>) {
        self.messages.push(message.into());
        if self.messages.len() > 20 {
            self.messages.remove(0);
        }
    }

    fn inspect_context(&mut self) {
        let player = self.campaign.simulation().player().position;
        if player.map == self.campaign.site_plan().regional_map {
            let history_site = self
                .campaign
                .site_plan()
                .regional_history_sites
                .iter()
                .filter_map(|site| {
                    let distance = grid_distance(player.grid, site.position);
                    (distance <= 2).then_some((distance, site))
                })
                .min_by_key(|(distance, site)| (*distance, site.event))
                .map(|(_, site)| site.clone());
            if let Some(site) = history_site {
                self.apply_campaign(CampaignCommand::InspectHistoricalSite(site.event));
                return;
            }
            let party = self
                .campaign
                .history()
                .world()
                .regional_parties()
                .values()
                .filter_map(|party| {
                    let entity = PlayableSitePlan::regional_party_entity(party.id);
                    let position = self.campaign.simulation().entity(entity)?.position;
                    let distance = grid_distance(player.grid, position.grid);
                    (distance <= 2).then_some((distance, party))
                })
                .min_by_key(|(distance, party)| (*distance, party.id));
            if let Some((_, party)) = party {
                self.push_message(format!(
                    "{} is {:?}, traveling from {} to {}.",
                    party.name,
                    party.kind,
                    self.campaign.history().world().sites()[&party.origin].name,
                    self.campaign.history().world().sites()[&party.destination].name
                ));
                return;
            }
        }
        let resident = self
            .campaign
            .site_plan()
            .residents
            .iter()
            .filter_map(|resident| {
                let position = self.campaign.simulation().entity(resident.entity)?.position;
                let distance = grid_distance(player.grid, position.grid);
                (position.map == player.map && distance <= 2).then_some((distance, resident))
            })
            .min_by_key(|(distance, resident)| (*distance, resident.person));
        if let Some((_, resident)) = resident {
            let activity = self
                .campaign
                .resident_agents()
                .get(&resident.person)
                .map_or("living their day".to_string(), |agent| {
                    format!("{:?}", agent.goal).to_ascii_lowercase()
                });
            self.push_message(format!(
                "{} is a {:?}, currently {activity}.",
                resident.name, resident.occupation
            ));
            return;
        }
        let landmark = self
            .campaign
            .simulation()
            .landmarks()
            .filter(|landmark| landmark.position.map == player.map)
            .filter_map(|landmark| {
                let distance = grid_distance(player.grid, landmark.position.grid);
                (distance <= 2).then_some((distance, landmark.name.clone()))
            })
            .min_by_key(|(distance, name)| (*distance, name.clone()));
        if let Some((_, landmark)) = landmark {
            self.push_message(format!("You inspect {landmark}."));
        } else {
            self.push_message("There is nothing notable nearby.");
        }
    }
}

pub fn observation_json(
    simulation: &Simulation,
    history: &HistoryEngine,
    site_plan: &PlayableSitePlan,
    messages: &[String],
    radius: i32,
) -> String {
    let radius = radius.clamp(2, 64);
    let player = simulation.player();
    let position = player.position;
    let layer = if position.map == site_plan.regional_map {
        "overworld"
    } else if position.grid.z < 0 {
        "dungeon"
    } else {
        "town"
    };
    let health = simulation
        .player_combatant()
        .map_or((0, 0), |combatant| (combatant.health, combatant.max_health));
    let progression = simulation.progression();
    let ascii = viewport_ascii(simulation, radius);
    let mut landmarks = simulation
        .landmarks()
        .filter(|landmark| landmark.position.map == position.map)
        .map(|landmark| {
            (
                grid_distance(position.grid, landmark.position.grid),
                landmark.name.as_str(),
                landmark.position.grid,
            )
        })
        .collect::<Vec<_>>();
    landmarks.sort_by_key(|landmark| landmark.0);
    landmarks.truncate(12);
    let landmark_json = landmarks
        .iter()
        .map(|(distance, name, grid)| {
            format!(
                "{{\"name\":\"{}\",\"distance\":{},\"x\":{},\"y\":{},\"z\":{}}}",
                json_escape(name),
                distance,
                grid.x,
                grid.y,
                grid.z
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let entity_json = simulation
        .entities()
        .filter(|entity| {
            entity.id != player.id
                && entity.position.map == position.map
                && entity.position.grid.z == position.grid.z
                && grid_distance(position.grid, entity.position.grid) <= radius
        })
        .take(32)
        .map(|entity| {
            let resident = site_plan
                .residents
                .iter()
                .find(|resident| resident.entity == entity.id);
            let party = history
                .world()
                .regional_parties()
                .values()
                .find(|party| PlayableSitePlan::regional_party_entity(party.id) == entity.id);
            let name = resident
                .map(|resident| resident.name.as_str())
                .or_else(|| party.map(|party| party.name.as_str()))
                .unwrap_or_else(|| entity_kind_name(entity.kind));
            let activity = resident
                .and_then(|resident| {
                    site_plan
                        .resident_activity(resident.person, simulation.tick)
                        .map(|(activity, _)| format!("{activity:?}"))
                })
                .unwrap_or_else(|| "none".to_string());
            format!(
                concat!(
                    "{{\"id\":{},\"kind\":\"{}\",\"name\":\"{}\",\"activity\":\"{}\",",
                    "\"x\":{},\"y\":{},\"distance\":{}}}"
                ),
                entity.id.0,
                entity_kind_name(entity.kind),
                json_escape(name),
                activity,
                entity.position.grid.x,
                entity.position.grid.y,
                grid_distance(position.grid, entity.position.grid)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let message_json = messages
        .iter()
        .rev()
        .take(8)
        .rev()
        .map(|message| format!("\"{}\"", json_escape(message)))
        .collect::<Vec<_>>()
        .join(",");
    let formula_json = simulation
        .known_formulas()
        .iter()
        .filter_map(|id| simulation.rules().formula(*id))
        .map(|formula| {
            format!(
                "{{\"id\":{},\"name\":\"{}\",\"effect\":\"{}\",\"condition\":\"{:?}\",\"reagents\":[{}]}}",
                formula.id.0,
                json_escape(&formula.name),
                formula.effect.name(),
                formula.condition,
                formula
                    .reagents
                    .iter()
                    .map(|material| format!("\"{}\"", material.name()))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let inventory_json = simulation
        .player_inventory()
        .into_iter()
        .flat_map(|inventory| inventory.items.iter())
        .filter_map(|id| simulation.item(*id))
        .map(|item| {
            let kind = match item.kind {
                ItemKind::MeleeWeapon { .. } => "melee_weapon",
                ItemKind::RangedWeapon { .. } => "ranged_weapon",
                ItemKind::Ammunition { .. } => "ammunition",
                ItemKind::Consumable { .. } => "consumable",
                ItemKind::Reagent { .. } => "reagent",
                ItemKind::InscribedArtifact { .. } => "inscribed_artifact",
                ItemKind::Artifact => "artifact",
            };
            format!(
                "{{\"id\":{},\"name\":\"{}\",\"kind\":\"{}\",\"quantity\":{}}}",
                item.id.0,
                json_escape(&item.name),
                kind,
                item.quantity
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"ok\":true,\"type\":\"observation\",\"seed\":{},\"tick\":{},",
            "\"date\":{{\"year\":{},\"month\":{}}},\"layer\":\"{}\",\"map\":{},",
            "\"paused\":{},",
            "\"player\":{{\"x\":{},\"y\":{},\"z\":{},\"health\":{},\"max_health\":{},",
            "\"level\":{},\"experience\":{},\"discoveries\":{},\"arcane_lore\":{}}},",
            "\"known_formulas\":[{}],\"inventory\":[{}],",
            "\"viewport\":{{\"radius\":{},\"ascii\":\"{}\"}},",
            "\"landmarks\":[{}],\"entities\":[{}],\"messages\":[{}]}}"
        ),
        simulation.campaign_seed,
        simulation.tick,
        history.world().date.year,
        history.world().date.month,
        layer,
        position.map.0,
        simulation.paused,
        position.grid.x,
        position.grid.y,
        position.grid.z,
        health.0,
        health.1,
        progression.level,
        progression.experience,
        progression.discoveries,
        progression.arcane_lore,
        formula_json,
        inventory_json,
        radius,
        json_escape(&ascii),
        landmark_json,
        entity_json,
        message_json,
    )
}

pub fn world_json(history: &HistoryEngine, site_plan: &PlayableSitePlan) -> String {
    let atlas = history.world().atlas();
    let mut terrain = std::collections::BTreeMap::<&'static str, usize>::new();
    for (_, cell) in atlas.cells() {
        *terrain.entry(biome_name(cell.biome)).or_default() += 1;
    }
    let terrain_json = terrain
        .into_iter()
        .map(|(name, count)| format!("\"{name}\":{count}"))
        .collect::<Vec<_>>()
        .join(",");
    let settlements = site_plan
        .regional_sites
        .iter()
        .map(|site| {
            let settlement = &history.world().regional_settlements()[&site.site];
            format!(
                concat!(
                    "{{\"name\":\"{}\",\"x\":{},\"y\":{},\"role\":\"{:?}\",",
                    "\"population\":{},\"shortage\":{},\"unrest\":{}}}"
                ),
                json_escape(&site.name),
                site.position.x,
                site.position.y,
                settlement.role,
                settlement.population,
                settlement.shortage,
                settlement.unrest
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let routes = site_plan
        .regional_routes
        .iter()
        .map(|route| {
            let state = &history.world().routes()[&route.route];
            format!(
                concat!(
                    "{{\"name\":\"{}\",\"length\":{},\"condition\":{},",
                    "\"danger\":{},\"disrupted\":{}}}"
                ),
                json_escape(&route.name),
                route.path.len(),
                state.condition,
                state.danger,
                state.disrupted
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let parties = history
        .world()
        .regional_parties()
        .values()
        .filter(|party| party.status == RegionalPartyStatus::Traveling)
        .map(|party| {
            let origin = &history.world().sites()[&party.origin].name;
            let destination = &history.world().sites()[&party.destination].name;
            format!(
                concat!(
                    "{{\"name\":\"{}\",\"kind\":\"{}\",\"origin\":\"{}\",",
                    "\"destination\":\"{}\",\"progress\":{}}}"
                ),
                json_escape(&party.name),
                json_escape(&format!("{:?}", party.kind)),
                json_escape(origin),
                json_escape(destination),
                party.progress
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let active_parties = history
        .world()
        .regional_parties()
        .values()
        .filter(|party| {
            matches!(
                party.status,
                RegionalPartyStatus::Traveling | RegionalPartyStatus::Stationed
            )
        })
        .map(|party| {
            format!(
                "{{\"name\":\"{}\",\"kind\":\"{:?}\",\"status\":\"{:?}\"}}",
                json_escape(&party.name),
                party.kind,
                party.status
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let history_sites = site_plan
        .regional_history_sites
        .iter()
        .map(|site| {
            format!(
                concat!(
                    "{{\"name\":\"{}\",\"event\":{},\"x\":{},\"y\":{},",
                    "\"description\":\"{}\"}}"
                ),
                json_escape(&site.name),
                site.event.0,
                site.position.x,
                site.position.y,
                json_escape(&site.description)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let mut event_kinds = BTreeMap::<String, usize>::new();
    for event in history.world().events().values() {
        *event_kinds.entry(format!("{:?}", event.kind)).or_default() += 1;
    }
    let event_kind_json = event_kinds
        .into_iter()
        .map(|(kind, count)| format!("\"{}\":{count}", json_escape(&kind)))
        .collect::<Vec<_>>()
        .join(",");
    let goals = history
        .world()
        .regional_goals()
        .values()
        .filter(|goal| goal.status == RegionalGoalStatus::Open)
        .map(|goal| {
            format!(
                "{{\"title\":\"{}\",\"kind\":\"{:?}\"}}",
                json_escape(&goal.title),
                goal.kind
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let fronts = StrategicFront::ALL
        .into_iter()
        .map(|front| {
            format!(
                "\"{front:?}\":{}",
                history.world().struggle().balance(front)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let strategic_actors = history
        .world()
        .struggle()
        .actors
        .values()
        .map(|actor| {
            let objective = actor
                .objective
                .as_ref()
                .map_or("none".to_string(), |objective| {
                    format!("{:?}", objective.kind)
                });
            format!(
                concat!(
                    "{{\"role\":\"{:?}\",\"name\":\"{}\",\"capacity\":{},",
                    "\"reserves\":{},\"influence\":{},\"objective\":\"{}\"}}"
                ),
                actor.role,
                json_escape(&actor.name),
                actor.capacity,
                actor.reserves,
                actor.influence,
                json_escape(&objective)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"ok\":true,\"type\":\"world\",\"date\":{{\"year\":{},\"month\":{}}},",
            "\"width\":{},\"height\":{},\"historical_events\":{},\"event_kinds\":{{{}}},",
            "\"terrain\":{{{}}},\"settlements\":[{}],\"routes\":[{}],",
            "\"traveling_parties\":[{}],\"active_parties\":[{}],",
            "\"history_sites\":[{}],\"open_goals\":[{}],\"strategic_actors\":[{}],",
            "\"strategic_fronts\":{{{}}}}}"
        ),
        history.world().date.year,
        history.world().date.month,
        atlas.width(),
        atlas.height(),
        history.world().events().len(),
        event_kind_json,
        terrain_json,
        settlements,
        routes,
        parties,
        active_parties,
        history_sites,
        goals,
        strategic_actors,
        fronts
    )
}

pub fn local_objectives_json(
    simulation: &Simulation,
    history: &HistoryEngine,
    site_plan: &PlayableSitePlan,
    progress: LocalObjectiveProgress,
) -> String {
    let quest = simulation
        .quest(site_plan.dungeon.quest)
        .expect("playable site always installs its dungeon quest");
    let objectives = quest
        .objectives
        .iter()
        .enumerate()
        .map(|(index, objective)| {
            format!(
                "{{\"index\":{},\"description\":\"{}\",\"completed\":{}}}",
                index,
                json_escape(&objective.description),
                objective.completed
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let resolutions = history
        .crisis_resolution_options(site_plan.crisis_event)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, option)| {
            format!(
                concat!(
                    "{{\"index\":{},\"kind\":\"{:?}\",\"title\":\"{}\",",
                    "\"description\":\"{}\"}}"
                ),
                index,
                option.kind,
                json_escape(&option.title),
                json_escape(&option.description)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"ok\":true,\"type\":\"objectives\",",
            "\"investigation\":{{\"met_contact\":{},\"inspected_evidence\":{},",
            "\"questioned_factions\":{}}},",
            "\"quest\":{{\"title\":\"{}\",\"status\":\"{:?}\",\"objectives\":[{}]}},",
            "\"crisis\":{{\"resolved\":{},\"aftermath_complete\":{},",
            "\"resolution_options\":[{}]}}}}"
        ),
        progress.met_contact,
        progress.inspected_evidence,
        progress.questioned_factions,
        json_escape(&quest.title),
        quest.status,
        objectives,
        progress.crisis_resolved,
        progress.aftermath_complete,
        resolutions
    )
}

pub fn open_goal_ids(history: &HistoryEngine) -> Vec<GoalId> {
    let mut goals = history
        .world()
        .regional_goals()
        .values()
        .filter(|goal| goal.status == RegionalGoalStatus::Open)
        .collect::<Vec<_>>();
    goals.sort_by_key(|goal| {
        let urgency = match goal.kind {
            RegionalGoalKind::SecureRoute(_) => 0_u8,
            RegionalGoalKind::RelieveShortage(_) => 1_u8,
        };
        (urgency, goal.created, goal.id)
    });
    goals.into_iter().map(|goal| goal.id).collect()
}

pub fn goals_json(
    history: &HistoryEngine,
    site_plan: &PlayableSitePlan,
    simulation: &Simulation,
) -> String {
    let player = simulation.player().position;
    let goals = open_goal_ids(history)
        .into_iter()
        .enumerate()
        .map(|(index, goal_id)| {
            let goal = &history.world().regional_goals()[&goal_id];
            let sponsor = &history.world().factions()[&goal.sponsor].name;
            let (target_name, target) = site_plan
                .regional_goal_target(goal.kind)
                .map_or(("unknown".to_string(), None), |(name, target)| {
                    (name.to_string(), Some(target))
                });
            let distance = target
                .filter(|_| player.map == site_plan.regional_map)
                .map(|target| grid_distance(player.grid, target));
            let (active_raiders, force_target) = match goal.kind {
                RegionalGoalKind::SecureRoute(route) => {
                    let raiders = history.active_route_raiders(route);
                    let force_target = raiders.first().and_then(|party| {
                        let entity = PlayableSitePlan::regional_party_entity(*party);
                        let position = simulation.entity(entity)?.position;
                        let name = &history.world().regional_parties()[party].name;
                        Some(format!(
                            "{{\"name\":\"{}\",\"x\":{},\"y\":{},\"distance\":{}}}",
                            json_escape(name),
                            position.grid.x,
                            position.grid.y,
                            if player.map == position.map {
                                grid_distance(player.grid, position.grid).to_string()
                            } else {
                                "null".to_string()
                            }
                        ))
                    });
                    (
                        raiders.len(),
                        force_target.unwrap_or_else(|| "null".to_string()),
                    )
                }
                RegionalGoalKind::RelieveShortage(_) => (0, "null".to_string()),
            };
            let options = history
                .regional_goal_options(goal_id)
                .unwrap_or_default()
                .into_iter()
                .enumerate()
                .map(|(option_index, option)| {
                    format!(
                        concat!(
                            "{{\"index\":{},\"title\":\"{}\",\"approach\":\"{:?}\",",
                            "\"description\":\"{}\"}}"
                        ),
                        option_index,
                        json_escape(&option.title),
                        option.approach,
                        json_escape(&option.description)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                concat!(
                    "{{\"index\":{},\"id\":{},\"title\":\"{}\",\"kind\":\"{:?}\",",
                    "\"sponsor\":\"{}\",\"description\":\"{}\",",
                    "\"target\":{{\"name\":\"{}\",\"x\":{},\"y\":{},\"distance\":{}}},",
                    "\"active_raiders\":{},\"force_target\":{},\"options\":[{}]}}"
                ),
                index,
                goal.id.0,
                json_escape(&goal.title),
                goal.kind,
                json_escape(sponsor),
                json_escape(&goal.description),
                json_escape(&target_name),
                target.map_or(0, |target| target.x),
                target.map_or(0, |target| target.y),
                distance.map_or_else(|| "null".to_string(), |distance| distance.to_string()),
                active_raiders,
                force_target,
                options
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"ok\":true,\"type\":\"goals\",\"goals\":[{goals}]}}")
}

pub fn path_to_landmark(simulation: &Simulation, query: &str) -> Option<Vec<Direction>> {
    let player = simulation.player().position;
    let query = query.to_ascii_lowercase();
    let targets = simulation
        .landmarks()
        .filter(|landmark| {
            landmark.position.map == player.map
                && landmark.position.grid.z == player.grid.z
                && landmark.name.to_ascii_lowercase().contains(&query)
        })
        .map(|landmark| landmark.position.grid)
        .collect::<BTreeSet<_>>();
    path_to_targets(simulation, &targets)
}

pub fn path_to_position(simulation: &Simulation, target: GridPos) -> Option<Vec<Direction>> {
    path_to_targets(simulation, &BTreeSet::from([target]))
}

fn path_to_position_avoiding_hostiles(
    simulation: &Simulation,
    target: GridPos,
    threshold: i32,
    allowed_hostile: Option<EntityId>,
) -> Option<Vec<Direction>> {
    let player = simulation.player().position;
    if player.grid.z != target.z {
        return None;
    }
    let map = simulation.map(player.map)?;
    let blocked = simulation
        .entities()
        .filter(|entity| allowed_hostile != Some(entity.id))
        .filter(|entity| {
            entity.position.map == player.map
                && entity.position.grid.z == player.grid.z
                && simulation
                    .combatant(entity.id)
                    .is_some_and(|combatant| combatant.hostile_to_player && combatant.is_alive())
        })
        .map(|entity| entity.position.grid)
        .collect::<BTreeSet<_>>();
    let mut queue = VecDeque::from([player.grid]);
    let mut previous = BTreeMap::<GridPos, (GridPos, Direction)>::new();
    let mut seen = BTreeSet::from([player.grid]);
    while let Some(current) = queue.pop_front() {
        if taxicab_distance(current, target) <= threshold {
            return Some(reconstruct_path(player.grid, current, &previous));
        }
        for direction in Direction::ALL {
            let (dx, dy) = direction.delta();
            let neighbor = current.offset(dx, dy, 0);
            if seen.contains(&neighbor)
                || blocked.contains(&neighbor)
                || map.cell(neighbor).is_none_or(|cell| cell.movement_blocked)
            {
                continue;
            }
            seen.insert(neighbor);
            previous.insert(neighbor, (current, direction));
            queue.push_back(neighbor);
        }
    }
    None
}

pub fn path_to_unvisited(simulation: &Simulation, tracker: &ExperienceTracker) -> Vec<Direction> {
    let player = simulation.player().position;
    let landmark_targets = simulation
        .landmarks()
        .filter(|landmark| {
            landmark.position.map == player.map
                && landmark.position.grid.z == player.grid.z
                && !tracker.has_discovered_landmark(landmark.position, &landmark.name)
        })
        .map(|landmark| landmark.position.grid)
        .collect::<BTreeSet<_>>();
    if !landmark_targets.is_empty()
        && let Some(path) = path_to_targets(simulation, &landmark_targets)
    {
        return path;
    }
    let Some(map) = simulation.map(player.map) else {
        return Vec::new();
    };
    let mut queue = VecDeque::from([player.grid]);
    let mut previous = BTreeMap::<GridPos, (GridPos, Direction)>::new();
    let mut seen = BTreeSet::from([player.grid]);
    let mut target = None;
    while let Some(current) = queue.pop_front() {
        if current != player.grid
            && !tracker.has_visited(WorldPosition {
                map: player.map,
                grid: current,
            })
        {
            target = Some(current);
            break;
        }
        for direction in Direction::ALL {
            let (dx, dy) = direction.delta();
            let neighbor = current.offset(dx, dy, 0);
            if seen.contains(&neighbor)
                || map.cell(neighbor).is_none_or(|cell| cell.movement_blocked)
            {
                continue;
            }
            seen.insert(neighbor);
            previous.insert(neighbor, (current, direction));
            queue.push_back(neighbor);
        }
    }
    target
        .map(|target| reconstruct_path(player.grid, target, &previous))
        .unwrap_or_default()
}

fn path_to_targets(simulation: &Simulation, targets: &BTreeSet<GridPos>) -> Option<Vec<Direction>> {
    let player = simulation.player().position;
    let map = simulation.map(player.map)?;
    let mut queue = VecDeque::from([player.grid]);
    let mut previous = BTreeMap::<GridPos, (GridPos, Direction)>::new();
    let mut seen = BTreeSet::from([player.grid]);
    while let Some(current) = queue.pop_front() {
        if targets.contains(&current) {
            return Some(reconstruct_path(player.grid, current, &previous));
        }
        for direction in Direction::ALL {
            let (dx, dy) = direction.delta();
            let neighbor = current.offset(dx, dy, 0);
            if seen.contains(&neighbor)
                || map.cell(neighbor).is_none_or(|cell| cell.movement_blocked)
            {
                continue;
            }
            seen.insert(neighbor);
            previous.insert(neighbor, (current, direction));
            queue.push_back(neighbor);
        }
    }
    None
}

fn reconstruct_path(
    start: GridPos,
    target: GridPos,
    previous: &BTreeMap<GridPos, (GridPos, Direction)>,
) -> Vec<Direction> {
    let mut path = Vec::new();
    let mut current = target;
    while current != start {
        let Some((parent, direction)) = previous.get(&current).copied() else {
            return Vec::new();
        };
        path.push(direction);
        current = parent;
    }
    path.reverse();
    path
}

pub fn help_json() -> String {
    concat!(
        "{\"ok\":true,\"type\":\"help\",\"commands\":[",
        "\"observe [radius]\",\"move <north|east|south|west> [count]\",",
        "\"interact\",\"wait [turns]\",\"explore [turns]\",\"metrics\",",
        "\"inspect\",\"study\",\"experiment <item id> <item id>\",",
        "\"cast [formula id]\",\"goto <landmark name>\",\"world\",\"goals\",",
        "\"pursue [goal index] [option index]\",\"objectives\",",
        "\"aid <consent|purchase|theft|alternative>\",",
        "\"slice [resolution index]\",\"reset [seed]\",\"quit\"],",
        "\"json_example\":{\"command\":\"move\",\"direction\":\"east\",\"count\":10}}"
    )
    .to_string()
}

pub fn error_json(error: &str) -> String {
    format!("{{\"ok\":false,\"error\":\"{}\"}}", json_escape(error))
}

fn viewport_ascii(simulation: &Simulation, radius: i32) -> String {
    let player = simulation.player();
    let center = player.position;
    let Some(map) = simulation.map(center.map) else {
        return String::new();
    };
    let mut output = String::new();
    for y in (center.grid.y - radius)..=(center.grid.y + radius) {
        for x in (center.grid.x - radius)..=(center.grid.x + radius) {
            let grid = GridPos::new(x, y, center.grid.z);
            let mut glyph = map
                .cell(grid)
                .map_or(' ', |cell| terrain_glyph(cell.terrain));
            if simulation.landmarks().any(|landmark| {
                landmark.position.map == center.map && landmark.position.grid == grid
            }) {
                glyph = 'L';
            }
            if let Some(entity) = simulation
                .entities()
                .find(|entity| entity.position.map == center.map && entity.position.grid == grid)
            {
                glyph = match entity.kind {
                    EntityKind::Player => '@',
                    EntityKind::Character => 'n',
                    EntityKind::Creature => '!',
                    EntityKind::Item => '*',
                };
            }
            output.push(glyph);
        }
        if y != center.grid.y + radius {
            output.push('\n');
        }
    }
    output
}

fn terrain_glyph(terrain: TerrainKind) -> char {
    match terrain {
        TerrainKind::Grass => '.',
        TerrainKind::Forest => '"',
        TerrainKind::Hills => '^',
        TerrainKind::Mountain => 'M',
        TerrainKind::Sand => ':',
        TerrainKind::Snow => '*',
        TerrainKind::Swamp => ';',
        TerrainKind::Dirt => ',',
        TerrainKind::Road => '=',
        TerrainKind::Ocean => 'O',
        TerrainKind::Water => '~',
        TerrainKind::Bridge => '#',
        TerrainKind::StoneFloor => '_',
        TerrainKind::Wall => 'W',
        TerrainKind::Farmland => '%',
        TerrainKind::Rubble => 'x',
        TerrainKind::StairsUp => '<',
        TerrainKind::StairsDown => '>',
    }
}

fn event_summary(event: &SimulationEvent) -> String {
    match event {
        SimulationEvent::Damaged { amount, .. } => format!("Combat damage: {amount}."),
        SimulationEvent::Defeated { entity, .. } => format!("Entity {} defeated.", entity.0),
        SimulationEvent::RevivedAtHealer { healer, health, .. } => {
            format!("Revived by entity {} with {health} health.", healer.0)
        }
        SimulationEvent::Traversed { destination, .. } => {
            format!(
                "Entered map {} depth {}.",
                destination.map.0, destination.grid.z
            )
        }
        SimulationEvent::ExperienceGained { amount, .. } => {
            format!("Gained {amount} experience.")
        }
        SimulationEvent::LevelGained { level, .. } => format!("Reached level {level}."),
        SimulationEvent::QuestAdvanced { .. } => "Quest objective advanced.".to_string(),
        SimulationEvent::QuestReadyToTurnIn { .. } => "Quest ready to turn in.".to_string(),
        SimulationEvent::QuestCompleted { .. } => "Quest completed.".to_string(),
        SimulationEvent::ActionFailed(reason) => format!("Action failed: {reason:?}."),
        other => format!("{other:?}"),
    }
}

fn campaign_rejection(outcome: &CampaignOutcome) -> String {
    outcome
        .campaign_events
        .iter()
        .find_map(|event| match event {
            CampaignEvent::ActionRejected(reason) => Some(reason.clone()),
            _ => None,
        })
        .or_else(|| outcome.errors.first().cloned())
        .unwrap_or_else(|| "campaign action produced no result".to_string())
}

fn entity_kind_name(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Player => "player",
        EntityKind::Character => "character",
        EntityKind::Creature => "creature",
        EntityKind::Item => "item",
    }
}

fn biome_name(biome: ultimate_fate_world_atlas::Biome) -> &'static str {
    use ultimate_fate_world_atlas::Biome;
    match biome {
        Biome::Ocean => "ocean",
        Biome::Coast => "coast",
        Biome::Grassland => "grassland",
        Biome::Forest => "forest",
        Biome::Desert => "desert",
        Biome::Swamp => "swamp",
        Biome::Tundra => "tundra",
        Biome::Hills => "hills",
        Biome::Mountains => "mountains",
    }
}

fn grid_distance(first: GridPos, second: GridPos) -> i32 {
    (first.x - second.x).abs().max((first.y - second.y).abs())
}

fn taxicab_distance(first: GridPos, second: GridPos) -> i32 {
    (first.x - second.x).abs() + (first.y - second.y).abs()
}

fn json_escape(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(escaped, "\\u{:04x}", control as u32);
            }
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(unix)]
pub mod bridge {
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        path::{Path, PathBuf},
        sync::mpsc::{self, Receiver, SyncSender},
        thread,
    };

    use super::{LabCommand, error_json, parse_command};

    pub struct BridgeRequest {
        pub command: LabCommand,
        response: SyncSender<String>,
    }

    impl BridgeRequest {
        pub fn respond(self, response: String) {
            let _ = self.response.send(response);
        }
    }

    pub struct RuntimeBridge {
        path: PathBuf,
        receiver: Receiver<BridgeRequest>,
    }

    impl RuntimeBridge {
        pub fn start(path: impl AsRef<Path>) -> std::io::Result<Self> {
            let path = path.as_ref().to_path_buf();
            if path.exists() {
                fs::remove_file(&path)?;
            }
            let listener = UnixListener::bind(&path)?;
            let (sender, receiver) = mpsc::channel::<BridgeRequest>();
            thread::Builder::new()
                .name("ultimate-fate-lab-bridge".to_string())
                .spawn(move || {
                    for connection in listener.incoming() {
                        let Ok(mut stream) = connection else {
                            continue;
                        };
                        let Ok(reader_stream) = stream.try_clone() else {
                            continue;
                        };
                        let mut reader = BufReader::new(reader_stream);
                        loop {
                            let mut line = String::new();
                            let Ok(read) = reader.read_line(&mut line) else {
                                break;
                            };
                            if read == 0 {
                                break;
                            }
                            let command = match parse_command(&line) {
                                Ok(command) => command,
                                Err(error) => {
                                    let _ = writeln!(stream, "{}", error_json(&error));
                                    let _ = stream.flush();
                                    continue;
                                }
                            };
                            let quit = command == LabCommand::Quit;
                            let (response_sender, response_receiver) = mpsc::sync_channel(1);
                            if sender
                                .send(BridgeRequest {
                                    command,
                                    response: response_sender,
                                })
                                .is_err()
                            {
                                break;
                            }
                            let Ok(response) = response_receiver.recv() else {
                                break;
                            };
                            let _ = writeln!(stream, "{response}");
                            let _ = stream.flush();
                            if quit {
                                break;
                            }
                        }
                    }
                })?;
            Ok(Self { path, receiver })
        }

        pub fn try_requests(&self) -> Vec<BridgeRequest> {
            self.receiver.try_iter().collect()
        }
    }

    impl Drop for RuntimeBridge {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_accepts_shell_and_json_commands() {
        assert_eq!(
            parse_command("move west 12"),
            Ok(LabCommand::Move {
                direction: Direction::West,
                count: 12
            })
        );
        assert_eq!(
            parse_command(r#"{"command":"observe","radius":20}"#),
            Ok(LabCommand::Observe { radius: 20 })
        );
        assert_eq!(
            parse_command("pursue 2 1"),
            Ok(LabCommand::PursueGoal {
                goal_index: 2,
                option_index: 1,
                maximum_turns: 10_000,
            })
        );
        assert_eq!(
            parse_command(
                r#"{"command":"pursue","goal_index":1,"option_index":2,"maximum_turns":500}"#
            ),
            Ok(LabCommand::PursueGoal {
                goal_index: 1,
                option_index: 2,
                maximum_turns: 500,
            })
        );
        assert_eq!(
            parse_command("slice 2"),
            Ok(LabCommand::PlaySlice {
                resolution_index: 2,
                maximum_turns: 10_000,
            })
        );
        assert_eq!(
            parse_command(r#"{"command":"slice","resolution_index":1,"maximum_turns":750}"#),
            Ok(LabCommand::PlaySlice {
                resolution_index: 1,
                maximum_turns: 750,
            })
        );
        assert_eq!(parse_command("study"), Ok(LabCommand::Study));
        assert_eq!(
            parse_command("experiment 17 23"),
            Ok(LabCommand::Experiment {
                first: ultimate_fate_core::ItemId(17),
                second: ultimate_fate_core::ItemId(23),
            })
        );
        assert_eq!(
            parse_command(r#"{"command":"cast","formula":2}"#),
            Ok(LabCommand::Cast {
                formula: FormulaId(2)
            })
        );
        assert_eq!(
            parse_command("aid theft"),
            Ok(LabCommand::ResolveAid {
                approach: AidResolutionKind::TakenWithoutConsent,
                maximum_turns: 10_000,
            })
        );
        assert_eq!(
            parse_command(r#"{"command":"aid","approach":"purchase","maximum_turns":750}"#),
            Ok(LabCommand::ResolveAid {
                approach: AidResolutionKind::Purchased,
                maximum_turns: 750,
            })
        );
    }

    #[test]
    fn headless_session_can_be_observed_and_autoplayed() {
        let mut session = LabSession::new(DEFAULT_SEED).expect("lab session");
        let observation = session.execute(LabCommand::Observe { radius: 8 });
        assert!(observation.contains("\"type\":\"observation\""));
        assert!(observation.contains("\"ascii\":"));

        let metrics = session.execute(LabCommand::Explore { turns: 200 });
        assert!(metrics.contains("\"type\":\"metrics\""));
        assert!(session.simulation().tick > 0);
    }

    #[test]
    fn game_lab_completes_and_replays_four_material_aid_routes() {
        for approach in [
            AidResolutionKind::ReleasedByConsent,
            AidResolutionKind::Purchased,
            AidResolutionKind::TakenWithoutConsent,
            AidResolutionKind::AlternativeTreatment,
        ] {
            let mut session = LabSession::new(DEFAULT_SEED).expect("lab session");
            let response = session.execute(LabCommand::ResolveAid {
                approach,
                maximum_turns: 2_000,
            });
            assert!(response.contains("\"ok\":true"), "{approach:?}: {response}");
            assert_eq!(
                session.campaign.aid_delivery().map(|(_, method)| method),
                Some(approach)
            );
            assert!(session.campaign.progress().aid_aftermath_event.is_some());
            assert!(
                session
                    .objectives_json()
                    .contains("\"status\":\"Resolved\"")
            );

            let save = session.campaign.save_to_string();
            let loaded = CampaignSession::load_from_str(&save).expect("replay aid route");
            assert_eq!(loaded.simulation(), session.campaign.simulation());
            assert_eq!(loaded.history().world(), session.campaign.history().world());
            assert_eq!(loaded.progress(), session.campaign.progress());
            assert_eq!(loaded.start(), session.campaign.start());
        }
    }

    #[test]
    fn generated_force_contract_requires_and_records_physical_victory() {
        let mut session = (0..64)
            .find_map(|seed| {
                let session = LabSession::new(seed).ok()?;
                open_goal_ids(session.history())
                    .iter()
                    .any(|goal| {
                        matches!(
                            session.history().world().regional_goals()[goal].kind,
                            RegionalGoalKind::SecureRoute(_)
                        )
                    })
                    .then_some(session)
            })
            .expect("at least one audited seed should produce a route contract");
        let goal = open_goal_ids(session.history())
            .into_iter()
            .find(|goal| {
                matches!(
                    session.history().world().regional_goals()[goal].kind,
                    RegionalGoalKind::SecureRoute(_)
                )
            })
            .expect("selected seed should produce a route contract");
        let RegionalGoalKind::SecureRoute(route) =
            session.history().world().regional_goals()[&goal].kind
        else {
            unreachable!()
        };
        assert!(session.history().world().routes()[&route].disrupted);
        assert_eq!(session.history().active_route_raiders(route).len(), 1);
        let goal_index = open_goal_ids(session.history())
            .iter()
            .position(|candidate| *candidate == goal)
            .expect("goal index");

        let response = session.execute(LabCommand::PursueGoal {
            goal_index,
            option_index: 0,
            maximum_turns: 10_000,
        });

        assert!(response.contains("\"ok\":true"), "{response}");
        assert!(!session.history().world().routes()[&route].disrupted);
        assert!(session.history().active_route_raiders(route).is_empty());
        assert_eq!(
            session.history().world().regional_goals()[&goal].status,
            RegionalGoalStatus::Resolved
        );
        assert_eq!(session.start().journal.entries.len(), 3);
        assert!(session.simulation().progression().experience > 0);
    }

    #[test]
    fn complete_local_slice_reaches_distinct_resolutions_without_overload() {
        let mut quest_titles = BTreeSet::new();
        for (seed, resolution_index) in [(0, 0), (1, 1), (2, 2)] {
            let mut session = LabSession::new(seed).expect("lab session");
            quest_titles.insert(session.site_plan().dungeon.quest_title.clone());
            let response = session.execute(LabCommand::PlaySlice {
                resolution_index,
                maximum_turns: 1_000,
            });
            assert!(response.contains("\"ok\":true"), "seed {seed}: {response}");
            assert_eq!(
                session
                    .simulation()
                    .quest(session.site_plan().dungeon.quest)
                    .map(|quest| quest.status),
                Some(QuestStatus::Completed)
            );
            assert!(session.progress().recorded_dungeon_clear);
            assert!(session.resolved_crisis.is_some());
            assert!(session.aftermath_complete);
            assert!(
                session.start().journal.entries.len() <= 12,
                "seed {seed} generated an overloaded journal"
            );
            assert_eq!(session.simulation().progression().discoveries, 1);
            assert_eq!(session.simulation().progression().arcane_lore, 1);
            assert_eq!(session.simulation().known_formulas().len(), 1);
            assert_eq!(
                session.history().world().significant_items()
                    [&session.site_plan().dungeon.world_item]
                    .custodian,
                ultimate_fate_history::ItemCustodian::Site(session.history().primary_site())
            );
            assert!(
                session.tracker.longest_quiet_stretch <= 100,
                "seed {seed} quiet for {} turns",
                session.tracker.longest_quiet_stretch
            );
            assert!(session.simulation().progression().level >= 2);
        }
        assert_eq!(
            quest_titles.len(),
            3,
            "fixed campaign corpus should exercise distinct history-born dungeons"
        );
    }

    #[cfg(unix)]
    #[test]
    fn runtime_bridge_round_trips_a_live_request() {
        use std::{
            io::{BufRead, BufReader, Write},
            os::unix::net::UnixStream,
            time::{Duration, Instant},
        };

        let path = std::env::temp_dir().join(format!(
            "ultimate-fate-lab-test-{}.sock",
            std::process::id()
        ));
        let bridge = match bridge::RuntimeBridge::start(&path) {
            Ok(bridge) => bridge,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("bridge: {error}"),
        };
        let mut client = UnixStream::connect(&path).expect("connect");
        writeln!(client, "observe 6").expect("request");
        client.flush().expect("flush");

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if let Some(request) = bridge.try_requests().into_iter().next() {
                assert_eq!(request.command, LabCommand::Observe { radius: 6 });
                request.respond("{\"ok\":true,\"live\":true}".to_string());
                break;
            }
            assert!(Instant::now() < deadline, "bridge request timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut response = String::new();
        BufReader::new(client)
            .read_line(&mut response)
            .expect("response");
        assert_eq!(response.trim(), "{\"ok\":true,\"live\":true}");
    }
}
