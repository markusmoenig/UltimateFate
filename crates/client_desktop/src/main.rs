use std::{
    collections::{BTreeSet, VecDeque},
    error::Error,
    ops::{Deref, DerefMut},
    sync::Arc,
    time::{Duration, Instant},
};

use ultimate_fate_core::{
    ActionFailure, CombatMethod, Direction, EntityId, GameCommand, GridPos, ItemId, ItemKind,
    QuestStatus, SimulationEvent, TerrainKind,
};
use ultimate_fate_history::{
    AidResolutionKind, CrisisResolutionKind, CrisisResolutionOption, CrisisResolutionOutcome,
    FactionId, GoalId, HistoricalEventKind, Occupation, PartyId, PersonId, RegionalGoalApproach,
    RegionalGoalKind, RegionalGoalStatus, RegionalPartyKind, RegionalPartyStatus,
    SettlementProjectPhase,
};
use ultimate_fate_input::{
    ActionEvent, DigitalInput, GameplayButton, InputAction, InputController,
};
#[cfg(unix)]
use ultimate_fate_lab::bridge::RuntimeBridge;
use ultimate_fate_lab::{
    ExperienceTracker, LabCommand, LocalObjectiveProgress, error_json as lab_error_json,
    goals_json as lab_goals_json, help_json as lab_help_json,
    local_objectives_json as lab_local_objectives_json, objects_json as lab_objects_json,
    observation_json as lab_observation_json, open_goal_ids as lab_open_goal_ids,
    path_to_landmark as lab_path_to_landmark, path_to_position as lab_path_to_position,
    path_to_unvisited as lab_path_to_unvisited, shop_json as lab_shop_json,
    world_json as lab_world_json,
};
use ultimate_fate_present::{PresentationSnapshot, ViewportRequest};
use ultimate_fate_render::{
    ClassicArtPack, OverheadProjection, Projection, UiDrawList, UiRect, UiTextStyle, ViewportSize,
    WgpuRenderer,
};
use ultimate_fate_session::{
    CampaignCommand, CampaignEvent, CampaignOutcome, CampaignSession, ResidentGoal, TradeDirection,
};
use ultimate_fate_text::{Conversation, ConversationContext, ConversationTopicKind};
use ultimate_fate_worldgen::{LocationSource, PlayableSitePlan};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

/// A standing player still yields turns to the living world. Input-driven
/// actions reset this deadline, preventing a heartbeat and a key repeat from
/// advancing two turns at once.
const WORLD_HEARTBEAT: Duration = Duration::from_millis(600);
/// Ordinary player actions are turns, not months. Continuous walking now takes
/// several real minutes per simulated month and normal play considerably longer.
#[cfg(test)]
const LIVING_MONTH_TICKS: u64 = ultimate_fate_session::LIVING_MONTH_TURNS;
const CAMPAIGN_SEED: u64 = 0x55aa_2026;
const HISTORY_YEARS: u32 = 20;
const REGIONAL_VIEW_SCALE: f32 = 0.55;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum UiMode {
    #[default]
    Briefing,
    Exploration,
    Journal,
    Region,
    Conversation,
    Inventory,
    Container,
    Trade,
    Targeting,
    Resolution,
}

#[derive(Clone, Copy)]
struct InputPrompts {
    movement: &'static str,
    vertical_navigation: &'static str,
    primary: &'static str,
    back: &'static str,
    inspect: &'static str,
    journal: &'static str,
    menu: &'static str,
}

impl InputPrompts {
    const fn desktop_keyboard() -> Self {
        Self {
            movement: "WASD / ARROWS",
            vertical_navigation: "W / S / UP / DOWN",
            primary: "E / ENTER",
            back: "ESC",
            inspect: "X / L",
            journal: "J",
            menu: "P",
        }
    }

    fn legend(self) -> String {
        format!(
            "MOVE  {}\nACT / ATTACK / STAIRS  {}   INSPECT  {}\nJOURNAL  {}   INVENTORY  {}",
            self.movement, self.primary, self.inspect, self.journal, self.menu
        )
    }

    fn begin(self) -> String {
        format!("[{}] BEGIN", self.primary)
    }

    fn close_journal(self) -> String {
        format!(
            "{} PAGE   [{}] REGIONAL SITUATIONS   [{} / {}] CLOSE",
            self.vertical_navigation, self.inspect, self.journal, self.back
        )
    }

    fn region_help(self) -> String {
        format!(
            "{} SITUATION   A / D RESPONSE   {} TRACK / RESOLVE   {} JOURNAL   {} CLOSE",
            self.vertical_navigation, self.primary, self.inspect, self.back
        )
    }

    fn conversation_help(self) -> String {
        format!(
            "{} CHOOSE   {} ASK   {} LEAVE",
            self.vertical_navigation, self.primary, self.back
        )
    }

    fn inventory_help(self) -> String {
        format!(
            "{} CHOOSE   A / D AMOUNT   {} EQUIP / USE   {} DROP   {} / {} CLOSE",
            self.vertical_navigation, self.primary, self.inspect, self.back, self.menu
        )
    }

    fn trade_help(self) -> String {
        format!(
            "{} CHOOSE   A / D AMOUNT   {} BUY / SELL   {} TRADE   {} CLOSE",
            self.vertical_navigation, self.inspect, self.primary, self.back
        )
    }

    fn container_help(self) -> String {
        format!(
            "{} CHOOSE   A / D AMOUNT   {} CONTENTS / PACK   {} MOVE   {} CLOSE",
            self.vertical_navigation, self.inspect, self.primary, self.back
        )
    }

    fn targeting_help(self) -> String {
        format!(
            "{} AIM   {} FIRE   {} CANCEL",
            self.movement, self.primary, self.back
        )
    }

    fn resolution_help(self) -> String {
        format!(
            "{} CHOOSE   {} COMMIT   {} LEAVE",
            self.vertical_navigation, self.primary, self.back
        )
    }

    fn resolution_continue(self) -> String {
        format!("[{}] RETURN TO TOWN", self.primary)
    }
}

struct ActiveConversation {
    conversation: Conversation,
    selected: usize,
    response: Option<String>,
}

struct ActiveTrade {
    merchant: PersonId,
    selected: usize,
    direction: TradeDirection,
    quantity: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContainerSide {
    Contents,
    Pack,
}

struct ActiveContainer {
    entity: EntityId,
    selected: usize,
    side: ContainerSide,
    quantity: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TargetingState {
    cursor: GridPos,
    range: u8,
}

struct ActiveResolution {
    options: Vec<CrisisResolutionOption>,
    selected: usize,
    outcome: Option<CrisisResolutionOutcome>,
}

struct SidebarContent {
    location: String,
    date: String,
    status: String,
    threat: String,
    lead: String,
    context: String,
    controls: String,
}

#[derive(Default)]
struct App {
    state: Option<State>,
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: WgpuRenderer,
    campaign: CampaignSession,
    projection: OverheadProjection,
    art_pack: ClassicArtPack,
    next_tick: Instant,
    last_input_update: Instant,
    input: InputController,
    input_prompts: InputPrompts,
    met_contact: bool,
    inspected_evidence: bool,
    active_conversation: Option<ActiveConversation>,
    active_trade: Option<ActiveTrade>,
    active_container: Option<ActiveContainer>,
    learned_topics: BTreeSet<(PersonId, ConversationTopicKind)>,
    questioned_factions: BTreeSet<FactionId>,
    inventory_selected: usize,
    inventory_quantity: u16,
    targeting: Option<TargetingState>,
    active_resolution: Option<ActiveResolution>,
    regional_goal_selected: usize,
    regional_option_selected: usize,
    tracked_regional_goal: Option<GoalId>,
    journal_page: usize,
    resolved_crisis: Option<CrisisResolutionOutcome>,
    aftermath_complete: bool,
    received_starter_sword: bool,
    ui_mode: UiMode,
    message_log: Vec<String>,
    lab_tracker: ExperienceTracker,
    lab_exploration_cursor: usize,
    lab_exploration_path: VecDeque<Direction>,
    #[cfg(unix)]
    lab_bridge: Option<RuntimeBridge>,
}

impl Deref for State {
    type Target = CampaignSession;

    fn deref(&self) -> &Self::Target {
        &self.campaign
    }
}

impl DerefMut for State {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.campaign
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Ultimate Fate")
                        .with_inner_size(LogicalSize::new(1100.0, 720.0)),
                )
                .expect("failed to create the Ultimate Fate window"),
        );
        let state = pollster::block_on(State::new(window));
        event_loop.set_control_flow(ControlFlow::WaitUntil(state.next_tick));
        self.state = Some(state);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if state.window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::KeyboardInput { event, .. } => {
                let changed = state.key_changed(
                    event.physical_key,
                    event.state == ElementState::Pressed,
                    Instant::now(),
                );
                if changed {
                    state.window.request_redraw();
                }
            }
            WindowEvent::Focused(false) => state.input.clear(),
            WindowEvent::RedrawRequested => match state.render() {
                Ok(()) | Err(wgpu::SurfaceError::Timeout) => {}
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    state.reconfigure_surface();
                }
                Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                Err(wgpu::SurfaceError::Other) => {}
            },
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        let now = Instant::now();
        let mut redraw = state.advance_input(now) | state.service_lab_bridge();
        if now >= state.next_tick {
            if state.ui_mode == UiMode::Exploration {
                let outcome = state.campaign.apply_game_command(GameCommand::Wait);
                redraw |= outcome.advanced_time() || outcome.changed_world();
                state.process_simulation_outcome(outcome);
            }
            state.next_tick = now + WORLD_HEARTBEAT;
        }
        if redraw {
            state.window.request_redraw();
        }
        let mut next_wake = state
            .input
            .next_repeat_in()
            .map(|remaining| now + remaining)
            .map_or(state.next_tick, |repeat| state.next_tick.min(repeat));
        if state.lab_bridge_active() {
            next_wake = next_wake.min(now + Duration::from_millis(50));
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(next_wake));
    }
}

impl State {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .expect("failed to create WGPU surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("failed to find a compatible GPU adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("ultimate-fate-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .expect("failed to create WGPU device");
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let present_mode = if capabilities
            .present_modes
            .contains(&wgpu::PresentMode::Fifo)
        {
            wgpu::PresentMode::Fifo
        } else {
            capabilities.present_modes[0]
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        let renderer = WgpuRenderer::new(&device, format);
        let campaign = CampaignSession::with_history_years(CAMPAIGN_SEED, HISTORY_YEARS)
            .expect("failed to create campaign session");
        let lab_tracker = ExperienceTracker::new(
            campaign.simulation(),
            campaign.history().world().events().len(),
        );
        #[cfg(unix)]
        let lab_bridge = std::env::var_os("ULTIMATE_FATE_LAB_SOCKET")
            .filter(|path| !path.is_empty())
            .and_then(|path| RuntimeBridge::start(path).ok());
        let history = campaign.history();
        let site_plan = campaign.site_plan();
        let dungeon_dx = site_plan.dungeon.entrance.x - site_plan.player_spawn.x;
        let dungeon_dy = site_plan.dungeon.entrance.y - site_plan.player_spawn.y;
        let dungeon_message = format!(
            "Quest: {}. {} lies {}; recover {}.",
            site_plan.dungeon.quest_title,
            site_plan.dungeon.name,
            relative_steps(dungeon_dx, dungeon_dy),
            site_plan.dungeon.relic.name
        );
        let project_count = history.world().projects().len();
        let living_world_message = if project_count == 0 {
            "The settlement currently has no public works planned.".to_string()
        } else {
            format!(
                "Living world: {project_count} factions are pursuing public works using shared materials and named workers. Some plans may stall or provoke conflict."
            )
        };
        let strategic_message = format!(
            "Larger struggle: The Free Realms oppose {} across military, economic, territorial, political, spiritual, and magical fronts.",
            history.world().struggle().dark_power
        );
        let regional_message = format!(
            "Region: {} settlements exchange food and materials across {} roads; raids, shortages, and migration continue while you explore.",
            history.world().regional_settlements().len(),
            history.world().routes().len()
        );

        Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            campaign,
            projection: OverheadProjection::default(),
            art_pack: ClassicArtPack,
            next_tick: Instant::now() + WORLD_HEARTBEAT,
            last_input_update: Instant::now(),
            input: InputController::default(),
            input_prompts: InputPrompts::desktop_keyboard(),
            met_contact: false,
            inspected_evidence: false,
            active_conversation: None,
            active_trade: None,
            active_container: None,
            learned_topics: BTreeSet::new(),
            questioned_factions: BTreeSet::new(),
            inventory_selected: 0,
            inventory_quantity: 1,
            targeting: None,
            active_resolution: None,
            regional_goal_selected: 0,
            regional_option_selected: 0,
            tracked_regional_goal: None,
            journal_page: 0,
            resolved_crisis: None,
            aftermath_complete: false,
            received_starter_sword: false,
            ui_mode: UiMode::Briefing,
            message_log: vec![
                "A new campaign history has been generated.".to_string(),
                "Press Enter to begin exploring.".to_string(),
                dungeon_message,
                living_world_message,
                regional_message,
                strategic_message,
            ],
            lab_tracker,
            lab_exploration_cursor: 0,
            lab_exploration_path: VecDeque::new(),
            #[cfg(unix)]
            lab_bridge,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.reconfigure_surface();
        self.window.request_redraw();
    }

    fn reconfigure_surface(&self) {
        self.surface.configure(&self.device, &self.config);
    }

    #[cfg(unix)]
    fn service_lab_bridge(&mut self) -> bool {
        let requests = self
            .lab_bridge
            .as_ref()
            .map(RuntimeBridge::try_requests)
            .unwrap_or_default();
        let changed = !requests.is_empty();
        for request in requests {
            let response = self.execute_lab_command(request.command.clone());
            request.respond(response);
        }
        changed
    }

    #[cfg(not(unix))]
    fn service_lab_bridge(&mut self) -> bool {
        false
    }

    #[cfg(unix)]
    fn lab_bridge_active(&self) -> bool {
        self.lab_bridge.is_some()
    }

    #[cfg(not(unix))]
    fn lab_bridge_active(&self) -> bool {
        false
    }

    fn execute_lab_command(&mut self, command: LabCommand) -> String {
        match command {
            LabCommand::Help => lab_help_json(),
            LabCommand::Observe { radius } => lab_observation_json(
                self.campaign.simulation(),
                self.campaign.history(),
                self.campaign.site_plan(),
                &self.message_log,
                radius,
            ),
            LabCommand::Metrics => self.lab_tracker.metrics_json(
                self.campaign.simulation(),
                self.campaign.history(),
                self.campaign.start().journal.entries.len(),
            ),
            LabCommand::World => lab_world_json(self.campaign.history(), self.campaign.site_plan()),
            LabCommand::Goals => lab_goals_json(
                self.campaign.history(),
                self.campaign.site_plan(),
                self.campaign.simulation(),
            ),
            LabCommand::Objectives => lab_local_objectives_json(
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
            ),
            LabCommand::Objects => lab_objects_json(&self.campaign),
            LabCommand::ObjectAction { command, .. } => {
                let outcome = self.campaign.apply_game_command(command);
                let failed = outcome
                    .simulation
                    .events
                    .iter()
                    .any(|event| matches!(event, SimulationEvent::ActionFailed(_)));
                self.process_simulation_outcome(outcome);
                if failed {
                    lab_error_json("object action failed in the live simulation")
                } else {
                    lab_objects_json(&self.campaign)
                }
            }
            LabCommand::Shop => lab_shop_json(&self.campaign),
            LabCommand::Trade {
                direction,
                item,
                quantity,
                ..
            } => {
                let merchant = self.campaign.site_plan().shop.merchant;
                let outcome = self.campaign.apply_command(quantity.map_or(
                    CampaignCommand::Trade {
                        merchant,
                        item,
                        direction,
                    },
                    |quantity| CampaignCommand::TradeQuantity {
                        merchant,
                        item,
                        direction,
                        quantity,
                    },
                ));
                let rejected = outcome
                    .campaign_events
                    .iter()
                    .find_map(|event| match event {
                        CampaignEvent::ActionRejected(reason) => Some(reason.clone()),
                        _ => None,
                    });
                self.process_simulation_outcome(outcome);
                rejected.map_or_else(
                    || lab_shop_json(&self.campaign),
                    |reason| lab_error_json(&reason),
                )
            }
            LabCommand::Move { direction, count } => {
                for _ in 0..count {
                    self.move_player(direction, false);
                }
                lab_observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.message_log,
                    12,
                )
            }
            LabCommand::Wait { turns } => {
                for _ in 0..turns {
                    let outcome = self.campaign.apply_game_command(GameCommand::Wait);
                    self.process_simulation_outcome(outcome);
                }
                self.lab_tracker.metrics_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.start().journal.entries.len(),
                )
            }
            LabCommand::Interact => {
                self.interact();
                lab_observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.message_log,
                    12,
                )
            }
            LabCommand::Inspect => {
                self.inspect();
                lab_observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.message_log,
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
                    let outcome = self
                        .campaign
                        .apply_game_command(GameCommand::Study(artifact));
                    self.process_simulation_outcome(outcome);
                } else {
                    self.push_message("No carried inscription can be studied.");
                }
                lab_observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.message_log,
                    12,
                )
            }
            LabCommand::Experiment { first, second } => {
                let outcome = self
                    .campaign
                    .apply_game_command(GameCommand::Experiment { first, second });
                self.process_simulation_outcome(outcome);
                lab_observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.message_log,
                    12,
                )
            }
            LabCommand::Cast { formula } => {
                let outcome = self.campaign.apply_game_command(GameCommand::Cast {
                    formula,
                    target: None,
                });
                self.process_simulation_outcome(outcome);
                lab_observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.message_log,
                    12,
                )
            }
            LabCommand::Explore { turns } => {
                self.lab_exploration_path.clear();
                for _ in 0..turns {
                    if let Some(target) = self.campaign.simulation().hostile_in_melee_range() {
                        let outcome = self
                            .campaign
                            .apply_game_command(GameCommand::Attack(target));
                        self.process_simulation_outcome(outcome);
                    } else if let Some(target) = self.campaign.simulation().hostile_in_ranged_line()
                    {
                        let outcome = self
                            .campaign
                            .apply_game_command(GameCommand::FireAt(target));
                        self.process_simulation_outcome(outcome);
                    } else {
                        let direction = self.lab_exploration_direction();
                        self.move_player(direction, false);
                    }
                }
                self.lab_tracker.metrics_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.start().journal.entries.len(),
                )
            }
            LabCommand::Goto {
                target,
                maximum_turns,
            } => {
                let Some(path) = lab_path_to_landmark(self.campaign.simulation(), &target) else {
                    return lab_error_json(&format!("no reachable landmark matching {target:?}"));
                };
                for direction in path.into_iter().take(maximum_turns as usize) {
                    self.move_player(direction, false);
                }
                lab_observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.message_log,
                    12,
                )
            }
            LabCommand::PursueGoal {
                goal_index,
                option_index,
                maximum_turns,
            } => match self.execute_lab_pursue_goal(goal_index, option_index, maximum_turns) {
                Ok(()) => lab_observation_json(
                    self.campaign.simulation(),
                    self.campaign.history(),
                    self.campaign.site_plan(),
                    &self.message_log,
                    12,
                ),
                Err(error) => lab_error_json(&error),
            },
            LabCommand::ResolveAid { .. } => lab_error_json(
                "aid-route automation is headless-only; use the generated conversation actions in a live window",
            ),
            LabCommand::PlaySlice { .. } => lab_error_json(
                "slice automation is headless-only; use objectives and semantic commands when observing a live player window",
            ),
            LabCommand::Reset { .. } => {
                lab_error_json("reset is supported by the headless lab, not a live window")
            }
            LabCommand::Quit => "{\"ok\":true,\"type\":\"quit\"}".to_string(),
        }
    }

    fn execute_lab_pursue_goal(
        &mut self,
        goal_index: usize,
        option_index: usize,
        maximum_turns: u32,
    ) -> Result<(), String> {
        let goal_id = lab_open_goal_ids(self.campaign.history())
            .get(goal_index)
            .copied()
            .ok_or_else(|| format!("no open regional goal at index {goal_index}"))?;
        let goal = self.campaign.history().world().regional_goals()[&goal_id].clone();
        let option = self
            .campaign
            .history()
            .regional_goal_options(goal_id)
            .map_err(|error| error.to_string())?
            .get(option_index)
            .cloned()
            .ok_or_else(|| format!("goal {goal_index} has no option at index {option_index}"))?;
        let mut turns = 0_u32;

        if self.campaign.simulation().player().position.map
            != self.campaign.site_plan().regional_map
        {
            let path = lab_path_to_landmark(self.campaign.simulation(), "world road gate")
                .ok_or_else(|| "the world road gate is not reachable from town".to_string())?;
            for direction in path {
                self.move_player(direction, false);
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
                let outcome = self.campaign.apply_game_command(GameCommand::Traverse);
                self.process_simulation_outcome(outcome);
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
                    && ranged_grid_distance(player.grid, target.grid) <= 1
                {
                    GameCommand::Attack(entity)
                } else if self.campaign.simulation().hostile_in_ranged_line() == Some(entity) {
                    GameCommand::FireAt(entity)
                } else {
                    let direction = lab_path_to_position(self.campaign.simulation(), target.grid)
                        .and_then(|path| path.first().copied())
                        .ok_or_else(|| {
                            format!(
                                "no route to raider party {}",
                                self.campaign.history().world().regional_parties()[&party].name
                            )
                        })?;
                    GameCommand::Move(direction)
                };
                let outcome = self.campaign.apply_game_command(command);
                self.process_simulation_outcome(outcome);
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
        while ranged_grid_distance(self.campaign.simulation().player().position.grid, target) > 2 {
            if turns >= maximum_turns {
                return Err(format!(
                    "turn limit reached before arriving at {target_name} for {}",
                    goal.title
                ));
            }
            let direction = lab_path_to_position(self.campaign.simulation(), target)
                .and_then(|path| path.first().copied())
                .ok_or_else(|| format!("{target_name} is not reachable"))?;
            self.move_player(direction, false);
            turns += 1;
        }

        let campaign_outcome = self
            .campaign
            .apply_command(CampaignCommand::ResolveRegionalGoal {
                goal: goal_id,
                approach: option.approach,
            });
        let outcome = campaign_outcome
            .campaign_events
            .iter()
            .find_map(|event| match event {
                CampaignEvent::RegionalGoalResolved(outcome) => Some(outcome.clone()),
                _ => None,
            })
            .ok_or_else(|| "the regional intervention was rejected".to_string())?;
        self.process_simulation_outcome(campaign_outcome);
        self.push_message(format!(
            "REGIONAL OUTCOME after {turns} turns: {}.",
            outcome.summary
        ));
        self.tracked_regional_goal = None;
        Ok(())
    }

    fn lab_exploration_direction(&mut self) -> Direction {
        if let Some(direction) = self.lab_exploration_path.pop_front() {
            return direction;
        }
        self.lab_exploration_path =
            lab_path_to_unvisited(self.campaign.simulation(), &self.lab_tracker).into();
        if let Some(direction) = self.lab_exploration_path.pop_front() {
            return direction;
        }
        let player = self.campaign.simulation().player().position;
        let directions = Direction::ALL;
        for offset in 0..directions.len() {
            let direction = directions[(self.lab_exploration_cursor + offset) % directions.len()];
            let (dx, dy) = direction.delta();
            let next = ultimate_fate_core::WorldPosition {
                map: player.map,
                grid: player.grid.offset(dx, dy, 0),
            };
            let passable = self
                .campaign
                .simulation()
                .map(player.map)
                .and_then(|map| map.cell(next.grid))
                .is_some_and(|cell| !cell.movement_blocked);
            if passable && !self.lab_tracker.has_visited(next) {
                self.lab_exploration_cursor =
                    (self.lab_exploration_cursor + offset + 1) % directions.len();
                return direction;
            }
        }
        let direction = directions[self.lab_exploration_cursor % directions.len()];
        self.lab_exploration_cursor = (self.lab_exploration_cursor + 1) % directions.len();
        direction
    }

    fn key_changed(&mut self, key: PhysicalKey, pressed: bool, now: Instant) -> bool {
        let mut changed = self.advance_input(now);
        let Some(input) = keyboard_input(key) else {
            return changed;
        };
        self.input.set_digital(input, pressed);
        changed |= self.dispatch_input();
        changed
    }

    fn advance_input(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last_input_update);
        self.last_input_update = now;
        self.input.update(elapsed);
        self.dispatch_input()
    }

    fn dispatch_input(&mut self) -> bool {
        let events = self.input.drain_events().collect::<Vec<_>>();
        let mut changed = false;
        for event in events {
            changed |= self.handle_action(event);
        }
        changed
    }

    fn handle_action(&mut self, event: ActionEvent) -> bool {
        match self.ui_mode {
            UiMode::Briefing => match event.action {
                InputAction::Button(GameplayButton::Primary | GameplayButton::Back)
                | InputAction::Menu => {
                    self.ui_mode = UiMode::Exploration;
                    self.input.clear();
                    self.push_message("You enter the town and begin your investigation.");
                    true
                }
                _ => false,
            },
            UiMode::Journal => match event.action {
                InputAction::Move(Direction::North) => {
                    self.move_journal_page(-1);
                    true
                }
                InputAction::Move(Direction::South) => {
                    self.move_journal_page(1);
                    true
                }
                InputAction::Button(GameplayButton::Inspect) => {
                    self.open_region_view();
                    true
                }
                InputAction::Button(GameplayButton::Back | GameplayButton::Journal)
                | InputAction::Menu => {
                    self.ui_mode = UiMode::Exploration;
                    self.input.clear();
                    self.push_message("Journal closed.");
                    true
                }
                _ => false,
            },
            UiMode::Region => match event.action {
                InputAction::Move(Direction::North) => {
                    self.move_regional_goal_selection(-1);
                    true
                }
                InputAction::Move(Direction::South) => {
                    self.move_regional_goal_selection(1);
                    true
                }
                InputAction::Move(Direction::West) => {
                    self.move_regional_option_selection(-1);
                    true
                }
                InputAction::Move(Direction::East) => {
                    self.move_regional_option_selection(1);
                    true
                }
                InputAction::Button(GameplayButton::Primary) => {
                    self.commit_regional_goal();
                    true
                }
                InputAction::Button(GameplayButton::Inspect | GameplayButton::Journal) => {
                    self.ui_mode = UiMode::Journal;
                    self.input.clear();
                    true
                }
                InputAction::Button(GameplayButton::Back) | InputAction::Menu => {
                    self.ui_mode = UiMode::Exploration;
                    self.input.clear();
                    true
                }
            },
            UiMode::Inventory => match event.action {
                InputAction::Move(Direction::North) => {
                    self.move_inventory_selection(-1);
                    true
                }
                InputAction::Move(Direction::South) => {
                    self.move_inventory_selection(1);
                    true
                }
                InputAction::Move(Direction::West) => {
                    self.adjust_inventory_quantity(-1);
                    true
                }
                InputAction::Move(Direction::East) => {
                    self.adjust_inventory_quantity(1);
                    true
                }
                InputAction::Button(GameplayButton::Primary) => {
                    self.activate_inventory_item();
                    true
                }
                InputAction::Button(GameplayButton::Inspect) => {
                    self.drop_inventory_item();
                    true
                }
                InputAction::Button(GameplayButton::Back) | InputAction::Menu => {
                    self.ui_mode = UiMode::Exploration;
                    self.input.clear();
                    true
                }
                _ => false,
            },
            UiMode::Container => match event.action {
                InputAction::Move(Direction::North) => {
                    self.move_container_selection(-1);
                    true
                }
                InputAction::Move(Direction::South) => {
                    self.move_container_selection(1);
                    true
                }
                InputAction::Move(Direction::West) => {
                    self.adjust_container_quantity(-1);
                    true
                }
                InputAction::Move(Direction::East) => {
                    self.adjust_container_quantity(1);
                    true
                }
                InputAction::Button(GameplayButton::Inspect) => {
                    self.switch_container_side();
                    true
                }
                InputAction::Button(GameplayButton::Primary) => {
                    self.transfer_container_item();
                    true
                }
                InputAction::Button(GameplayButton::Back) | InputAction::Menu => {
                    self.close_container();
                    true
                }
                _ => false,
            },
            UiMode::Trade => match event.action {
                InputAction::Move(Direction::North) => {
                    self.move_trade_selection(-1);
                    true
                }
                InputAction::Move(Direction::South) => {
                    self.move_trade_selection(1);
                    true
                }
                InputAction::Move(Direction::West) => {
                    self.adjust_trade_quantity(-1);
                    true
                }
                InputAction::Move(Direction::East) => {
                    self.adjust_trade_quantity(1);
                    true
                }
                InputAction::Button(GameplayButton::Inspect) => {
                    self.switch_trade_direction();
                    true
                }
                InputAction::Button(GameplayButton::Primary) => {
                    self.execute_trade();
                    true
                }
                InputAction::Button(GameplayButton::Back) | InputAction::Menu => {
                    self.close_trade();
                    true
                }
                _ => false,
            },
            UiMode::Conversation => match event.action {
                InputAction::Move(Direction::North) => {
                    self.move_conversation_selection(-1);
                    true
                }
                InputAction::Move(Direction::South) => {
                    self.move_conversation_selection(1);
                    true
                }
                InputAction::Button(GameplayButton::Primary) => {
                    self.ask_conversation_topic();
                    true
                }
                InputAction::Button(GameplayButton::Back) | InputAction::Menu => {
                    self.close_conversation();
                    true
                }
                _ => false,
            },
            UiMode::Targeting => match event.action {
                InputAction::Move(direction) => {
                    self.move_target_cursor(direction);
                    true
                }
                InputAction::Button(GameplayButton::Primary) => {
                    self.fire_at_target_cursor();
                    true
                }
                InputAction::Button(GameplayButton::Back) | InputAction::Menu => {
                    self.close_targeting();
                    true
                }
                _ => false,
            },
            UiMode::Resolution => {
                let resolved = self
                    .active_resolution
                    .as_ref()
                    .is_some_and(|resolution| resolution.outcome.is_some());
                match event.action {
                    InputAction::Move(Direction::North) if !resolved => {
                        self.move_resolution_selection(-1);
                        true
                    }
                    InputAction::Move(Direction::South) if !resolved => {
                        self.move_resolution_selection(1);
                        true
                    }
                    InputAction::Button(GameplayButton::Primary) if !resolved => {
                        self.commit_crisis_resolution();
                        true
                    }
                    InputAction::Button(GameplayButton::Primary | GameplayButton::Back)
                    | InputAction::Menu => {
                        self.close_crisis_resolution();
                        true
                    }
                    _ => false,
                }
            }
            UiMode::Exploration => match event.action {
                InputAction::Move(direction) => {
                    self.move_player(direction, !event.repeated);
                    true
                }
                InputAction::Button(GameplayButton::Primary) => {
                    self.interact();
                    true
                }
                InputAction::Button(GameplayButton::Back) | InputAction::Menu => {
                    if matches!(event.action, InputAction::Menu) {
                        self.open_inventory();
                    } else {
                        self.toggle_pause();
                    }
                    true
                }
                InputAction::Button(GameplayButton::Inspect) => {
                    self.inspect();
                    true
                }
                InputAction::Button(GameplayButton::Journal) => {
                    self.journal_page = 0;
                    self.ui_mode = UiMode::Journal;
                    self.input.clear();
                    true
                }
            },
        }
    }

    fn move_player(&mut self, direction: Direction, announce_step: bool) {
        let outcome = self
            .campaign
            .apply_game_command(GameCommand::Move(direction));
        let changed_world = outcome.changed_world();
        let had_events = !outcome.simulation.events.is_empty();
        self.process_simulation_outcome(outcome);
        if had_events {
            return;
        }
        if !changed_world {
            if announce_step {
                self.push_message("The way is blocked.");
            }
            return;
        }

        let position = self.campaign.simulation().player().position;
        let destination = self
            .campaign
            .simulation()
            .landmarks()
            .find(|landmark| landmark.position == position)
            .map(|landmark| landmark.name.clone());
        if let Some(destination) = destination {
            self.push_message(format!("You arrive at {destination}."));
        } else if announce_step {
            self.push_message(format!("You move {}.", direction_name(direction)));
        }
    }

    fn interact(&mut self) {
        if let Some(target) = self.campaign.simulation().hostile_in_melee_range() {
            let outcome = self
                .campaign
                .apply_game_command(GameCommand::Attack(target));
            self.process_simulation_outcome(outcome);
            return;
        }
        let player = self.campaign.simulation().player().position;
        if self.campaign.simulation().transition_at(player).is_some() {
            let outcome = self.campaign.apply_game_command(GameCommand::Traverse);
            self.process_simulation_outcome(outcome);
            return;
        }
        let resident = self
            .campaign
            .site_plan()
            .residents
            .iter()
            .filter_map(|resident| {
                let position = self.campaign.simulation().entity(resident.entity)?.position;
                let dx = position.grid.x - player.grid.x;
                let dy = position.grid.y - player.grid.y;
                (position.map == player.map
                    && position.grid.z == player.grid.z
                    && dx.abs() + dy.abs() <= 1)
                    .then_some((
                        resident.person,
                        resident.name.clone(),
                        resident.occupation,
                        resident.faction,
                    ))
            })
            .min_by_key(|(person, _, _, _)| {
                if *person == self.campaign.site_plan().contact {
                    0
                } else {
                    1
                }
            });
        if let Some((person, name, occupation, faction)) = resident {
            let resident_entity = self
                .campaign
                .site_plan()
                .residents
                .iter()
                .find(|resident| resident.person == person)
                .map(|resident| resident.entity);
            let ready_quest = resident_entity.and_then(|resident_entity| {
                self.campaign
                    .simulation()
                    .quests()
                    .find(|quest| {
                        quest.giver == resident_entity && quest.status == QuestStatus::ReadyToTurnIn
                    })
                    .map(|quest| quest.id)
            });
            if let Some(quest) = ready_quest {
                let outcome = self
                    .campaign
                    .apply_game_command(GameCommand::TurnInQuest(quest));
                self.process_simulation_outcome(outcome);
                return;
            }
            if person == self.campaign.site_plan().contact
                && self.inspected_evidence
                && self.questioned_factions.len() >= 2
                && self.resolved_crisis.is_none()
                && self.open_crisis_resolution()
            {
                return;
            }
            let faction_name = self.campaign.history().world().factions()[&faction]
                .name
                .clone();
            self.push_message(format!(
                "You speak with {name}, a {} of {faction_name}.",
                occupation_name(occupation)
            ));
            let mut context = ConversationContext::default();
            if self.inspected_evidence {
                context
                    .examined_evidence
                    .insert(self.campaign.site_plan().evidence_event);
            }
            match self.campaign.conversation_for_person(person, &context) {
                Ok(conversation) => {
                    self.active_conversation = Some(ActiveConversation {
                        conversation,
                        selected: 0,
                        response: None,
                    });
                    self.ui_mode = UiMode::Conversation;
                    self.input.clear();
                }
                Err(_) => {
                    self.push_message(format!("{name} has nothing to say."));
                }
            }
            return;
        }

        if self.open_ranged_targeting() {
            return;
        }

        let nearby_place = self.campaign.simulation().landmarks().find_map(|landmark| {
            let position = landmark.position;
            let dx = position.grid.x - player.grid.x;
            let dy = position.grid.y - player.grid.y;
            (position.map == player.map
                && position.grid.z == player.grid.z
                && dx.abs() + dy.abs() <= 1)
                .then_some(landmark.name.clone())
        });
        if let Some(place) = nearby_place {
            self.push_message(format!("You find no one available at {place}."));
        } else {
            self.push_message("There is nothing nearby to interact with.");
        }
    }

    fn open_ranged_targeting(&mut self) -> bool {
        let player = self.campaign.simulation().player();
        let Some(range) = self
            .campaign
            .simulation()
            .equipped_ranged_range(self.campaign.simulation().player_id())
        else {
            return false;
        };
        let player_position = player.position;
        let fallback_cursor = {
            let (dx, dy) = player.facing.delta();
            player_position.grid.offset(dx, dy, 0)
        };
        let cursor =
            self.campaign
                .simulation()
                .entities()
                .filter(|entity| {
                    entity.id != self.campaign.simulation().player_id()
                        && entity.position.map == player_position.map
                        && entity.position.grid.z == player_position.grid.z
                        && self.campaign.simulation().combatant(entity.id).is_some_and(
                            |combatant| combatant.hostile_to_player && combatant.is_alive(),
                        )
                })
                .filter_map(|entity| {
                    let distance = ranged_grid_distance(player_position.grid, entity.position.grid);
                    (distance <= i32::from(range)).then_some((
                        self.campaign
                            .simulation()
                            .check_ranged_attack(self.campaign.simulation().player_id(), entity.id)
                            .is_err(),
                        distance,
                        entity.id,
                        entity.position.grid,
                    ))
                })
                .min()
                .map(|(_, _, _, position)| position)
                .unwrap_or(fallback_cursor);

        self.targeting = Some(TargetingState { cursor, range });
        self.ui_mode = UiMode::Targeting;
        self.input.clear();
        self.push_message("Choose a ranged target.");
        true
    }

    fn move_target_cursor(&mut self, direction: Direction) {
        let Some(targeting) = self.targeting else {
            return;
        };
        let (dx, dy) = direction.delta();
        let next = targeting.cursor.offset(dx, dy, 0);
        let player = self.campaign.simulation().player().position.grid;
        if ranged_grid_distance(player, next) <= i32::from(targeting.range) {
            self.targeting = Some(TargetingState {
                cursor: next,
                ..targeting
            });
        }
    }

    fn target_at_cursor(&self) -> Option<EntityId> {
        let cursor = self.targeting?.cursor;
        let player = self.campaign.simulation().player();
        self.campaign
            .simulation()
            .entities()
            .filter(|entity| {
                entity.id != player.id
                    && entity.position.map == player.position.map
                    && entity.position.grid == cursor
            })
            .find(|entity| {
                self.campaign
                    .simulation()
                    .combatant(entity.id)
                    .is_some_and(|combatant| combatant.is_alive())
            })
            .map(|entity| entity.id)
    }

    fn fire_at_target_cursor(&mut self) {
        let Some(target) = self.target_at_cursor() else {
            self.push_message("There is no target under the cursor.");
            return;
        };
        let outcome = self
            .campaign
            .apply_game_command(GameCommand::FireAt(target));
        let fired = outcome.changed_world();
        self.process_simulation_outcome(outcome);
        if fired {
            self.targeting = None;
            self.ui_mode = UiMode::Exploration;
            self.input.clear();
        }
    }

    fn close_targeting(&mut self) {
        self.targeting = None;
        self.ui_mode = UiMode::Exploration;
        self.input.clear();
        self.push_message("Targeting cancelled.");
    }

    fn move_conversation_selection(&mut self, offset: isize) {
        let Some(active) = self.active_conversation.as_mut() else {
            return;
        };
        let count = active.conversation.topics.len();
        if count == 0 {
            return;
        }
        active.selected = (active.selected as isize + offset).rem_euclid(count as isize) as usize;
        active.response = None;
    }

    fn ask_conversation_topic(&mut self) {
        let Some((speaker, topic)) = self.active_conversation.as_ref().and_then(|active| {
            active
                .conversation
                .topics
                .get(active.selected)
                .cloned()
                .map(|topic| (active.conversation.speaker, topic))
        }) else {
            return;
        };
        if topic.kind == ConversationTopicKind::Trade {
            self.open_trade(speaker);
            return;
        }

        let response = topic.response.clone();
        let outcome = self.campaign.apply_command(CampaignCommand::Talk {
            person: speaker,
            topic: topic.kind,
        });
        let accepted = outcome
            .campaign_events
            .iter()
            .any(|event| matches!(event, CampaignEvent::Conversation { .. }));
        self.process_simulation_outcome(outcome);
        if accepted && let Some(active) = self.active_conversation.as_mut() {
            active.response = Some(response);
        }
    }

    fn open_trade(&mut self, merchant: PersonId) {
        if merchant != self.campaign.site_plan().shop.merchant {
            self.push_message("This person has no ordinary goods to trade.");
            return;
        }
        self.active_conversation = None;
        self.active_trade = Some(ActiveTrade {
            merchant,
            selected: 0,
            direction: TradeDirection::Buy,
            quantity: 1,
        });
        self.ui_mode = UiMode::Trade;
        self.input.clear();
    }

    fn trade_item_ids(&self, direction: TradeDirection) -> Vec<ItemId> {
        let shop = &self.campaign.site_plan().shop;
        self.campaign.trade_items(shop.merchant, direction)
    }

    fn move_trade_selection(&mut self, offset: isize) {
        let Some(active) = self.active_trade.as_ref() else {
            return;
        };
        let count = self.trade_item_ids(active.direction).len();
        if count == 0 {
            return;
        }
        if let Some(active) = self.active_trade.as_mut() {
            active.selected =
                (active.selected as isize + offset).rem_euclid(count as isize) as usize;
            active.quantity = 1;
        }
    }

    fn switch_trade_direction(&mut self) {
        if let Some(active) = self.active_trade.as_mut() {
            active.direction = match active.direction {
                TradeDirection::Buy => TradeDirection::Sell,
                TradeDirection::Sell => TradeDirection::Buy,
            };
            active.selected = 0;
            active.quantity = 1;
        }
    }

    fn adjust_trade_quantity(&mut self, offset: i16) {
        let Some((selected, direction, current)) = self
            .active_trade
            .as_ref()
            .map(|trade| (trade.selected, trade.direction, trade.quantity))
        else {
            return;
        };
        let maximum = self
            .trade_item_ids(direction)
            .get(selected)
            .and_then(|item| self.campaign.simulation().item(*item))
            .map(|item| item.quantity)
            .unwrap_or(1);
        if let Some(active) = self.active_trade.as_mut() {
            active.quantity =
                (i32::from(current) + i32::from(offset)).clamp(1, i32::from(maximum)) as u16;
        }
    }

    fn execute_trade(&mut self) {
        let Some((merchant, selected, direction, quantity)) =
            self.active_trade.as_ref().map(|trade| {
                (
                    trade.merchant,
                    trade.selected,
                    trade.direction,
                    trade.quantity,
                )
            })
        else {
            return;
        };
        let Some(item) = self.trade_item_ids(direction).get(selected).copied() else {
            self.push_message(match direction {
                TradeDirection::Buy => "The merchant has no lawful stock left.",
                TradeDirection::Sell => "You have no lawful ordinary goods to sell.",
            });
            return;
        };
        let outcome = self.campaign.apply_command(CampaignCommand::TradeQuantity {
            merchant,
            item,
            direction,
            quantity,
        });
        self.process_simulation_outcome(outcome);
        let count = self.trade_item_ids(direction).len();
        if let Some(active) = self.active_trade.as_mut() {
            active.selected = active.selected.min(count.saturating_sub(1));
            active.quantity = 1;
        }
    }

    fn close_trade(&mut self) {
        self.active_trade = None;
        self.ui_mode = UiMode::Exploration;
        self.input.clear();
        self.push_message("You close the merchant's ledger.");
    }

    fn close_conversation(&mut self) {
        let speaker = self
            .active_conversation
            .as_ref()
            .map(|active| active.conversation.speaker_name.clone());
        self.active_conversation = None;
        self.ui_mode = UiMode::Exploration;
        self.input.clear();
        if let Some(speaker) = speaker {
            self.push_message(format!("You end your conversation with {speaker}."));
        }
    }

    fn open_crisis_resolution(&mut self) -> bool {
        let Ok(options) = self
            .campaign
            .history()
            .crisis_resolution_options(self.campaign.site_plan().crisis_event)
        else {
            self.push_message("The crisis cannot yet be resolved.");
            return false;
        };
        if options.is_empty() {
            return false;
        }
        self.active_resolution = Some(ActiveResolution {
            options,
            selected: 0,
            outcome: None,
        });
        self.ui_mode = UiMode::Resolution;
        self.input.clear();
        self.push_message(format!(
            "{} asks what should be done about the crisis.",
            self.campaign.site_plan().contact_name
        ));
        true
    }

    fn move_resolution_selection(&mut self, offset: isize) {
        let Some(resolution) = self.active_resolution.as_mut() else {
            return;
        };
        let count = resolution.options.len();
        if count == 0 || resolution.outcome.is_some() {
            return;
        }
        resolution.selected =
            (resolution.selected as isize + offset).rem_euclid(count as isize) as usize;
    }

    fn commit_crisis_resolution(&mut self) {
        let Some(choice) = self.active_resolution.as_ref().and_then(|resolution| {
            resolution
                .options
                .get(resolution.selected)
                .map(|option| option.kind)
        }) else {
            return;
        };
        let outcome = self
            .campaign
            .apply_command(CampaignCommand::ResolveCrisis(choice));
        self.process_simulation_outcome(outcome);
        self.input.clear();
    }

    fn close_crisis_resolution(&mut self) {
        self.active_resolution = None;
        self.ui_mode = UiMode::Exploration;
        self.input.clear();
    }

    fn open_region_view(&mut self) {
        let count = self.open_regional_goal_ids().len();
        self.regional_goal_selected = self.regional_goal_selected.min(count.saturating_sub(1));
        self.regional_option_selected = 0;
        self.ui_mode = UiMode::Region;
        self.input.clear();
    }

    fn move_journal_page(&mut self, offset: isize) {
        let entry_count = self.campaign.start().journal.entries.len();
        let page_count = entry_count.max(1).div_ceil(4);
        self.journal_page =
            (self.journal_page as isize + offset).rem_euclid(page_count as isize) as usize;
    }

    fn open_regional_goal_ids(&self) -> Vec<GoalId> {
        let mut goals = self
            .campaign
            .history()
            .world()
            .regional_goals()
            .values()
            .filter(|goal| goal.status == RegionalGoalStatus::Open)
            .collect::<Vec<_>>();
        goals.sort_by_key(|goal| {
            let urgency = match goal.kind {
                RegionalGoalKind::SecureRoute(_) => 0,
                RegionalGoalKind::RelieveShortage(_) => 1,
            };
            (
                u8::from(Some(goal.id) != self.tracked_regional_goal),
                urgency,
                goal.created,
                goal.id,
            )
        });
        goals.into_iter().map(|goal| goal.id).collect()
    }

    fn move_regional_goal_selection(&mut self, offset: isize) {
        let count = self.open_regional_goal_ids().len();
        if count == 0 {
            return;
        }
        self.regional_goal_selected =
            (self.regional_goal_selected as isize + offset).rem_euclid(count as isize) as usize;
        self.regional_option_selected = 0;
    }

    fn move_regional_option_selection(&mut self, offset: isize) {
        let Some(goal) = self
            .open_regional_goal_ids()
            .get(self.regional_goal_selected)
            .copied()
        else {
            return;
        };
        let count = self
            .campaign
            .history()
            .regional_goal_options(goal)
            .map_or(0, |options| options.len());
        if count == 0 {
            return;
        }
        self.regional_option_selected =
            (self.regional_option_selected as isize + offset).rem_euclid(count as isize) as usize;
    }

    fn commit_regional_goal(&mut self) {
        let Some(goal) = self
            .open_regional_goal_ids()
            .get(self.regional_goal_selected)
            .copied()
        else {
            self.push_message("No regional situation currently requires a decision.");
            return;
        };
        let Some(goal_record) = self
            .campaign
            .history()
            .world()
            .regional_goals()
            .get(&goal)
            .cloned()
        else {
            return;
        };
        let Some(approach) = self
            .campaign
            .history()
            .regional_goal_options(goal)
            .ok()
            .and_then(|options| {
                options
                    .get(self.regional_option_selected)
                    .map(|option| option.approach)
            })
        else {
            return;
        };
        let Some((target_name, target)) =
            self.regional_goal_target_for_approach(goal_record.kind, approach)
        else {
            return;
        };
        let player = self.campaign.simulation().player().position;
        let at_target = player.map == self.campaign.site_plan().regional_map
            && (player.grid.x - target.x).abs() + (player.grid.y - target.y).abs() <= 2;
        if !at_target {
            self.tracked_regional_goal = Some(goal);
            self.ui_mode = UiMode::Exploration;
            self.input.clear();
            if player.map == self.campaign.site_plan().regional_map {
                self.push_message(format!(
                    "Tracking {}: travel {} to {target_name}, then reopen Regional Situations.",
                    goal_record.title,
                    relative_steps(target.x - player.grid.x, target.y - player.grid.y)
                ));
            } else {
                let gate = self.campaign.site_plan().nearest_regional_gate(player.grid);
                self.push_message(format!(
                    "Tracking {}: leave town via the regional gate {}, then travel to {target_name}.",
                    goal_record.title,
                    relative_steps(gate.x - player.grid.x, gate.y - player.grid.y)
                ));
            }
            return;
        }
        if let RegionalGoalKind::SecureRoute(route) = goal_record.kind
            && approach == RegionalGoalApproach::RestoreByForce
            && let Some(raider) = self
                .campaign
                .history()
                .active_route_raiders(route)
                .first()
                .copied()
        {
            let name = self.campaign.history().world().regional_parties()[&raider]
                .name
                .clone();
            self.tracked_regional_goal = Some(goal);
            self.ui_mode = UiMode::Exploration;
            self.input.clear();
            self.push_message(format!(
                "Force requires action: defeat {name}, then return to Regional Situations to report the road clear."
            ));
            return;
        }
        let outcome = self
            .campaign
            .apply_command(CampaignCommand::ResolveRegionalGoal { goal, approach });
        let resolved = outcome
            .campaign_events
            .iter()
            .any(|event| matches!(event, CampaignEvent::RegionalGoalResolved(_)));
        self.process_simulation_outcome(outcome);
        if resolved {
            self.tracked_regional_goal = None;
        }
        self.regional_goal_selected = 0;
        self.regional_option_selected = 0;
    }

    fn regional_goal_target_for_approach(
        &self,
        kind: RegionalGoalKind,
        approach: RegionalGoalApproach,
    ) -> Option<(String, GridPos)> {
        if let RegionalGoalKind::SecureRoute(route) = kind
            && approach == RegionalGoalApproach::RestoreByForce
            && let Some(party) = self
                .campaign
                .history()
                .active_route_raiders(route)
                .first()
                .copied()
        {
            let entity = PlayableSitePlan::regional_party_entity(party);
            let position = self.campaign.simulation().entity(entity)?.position;
            return Some((
                self.campaign.history().world().regional_parties()[&party]
                    .name
                    .clone(),
                position.grid,
            ));
        }
        self.campaign
            .site_plan()
            .regional_goal_target(kind)
            .map(|(name, target)| (name.to_string(), target))
    }

    fn open_inventory(&mut self) {
        let count = self.inventory_item_ids().len();
        self.inventory_selected = self.inventory_selected.min(count.saturating_sub(1));
        self.ui_mode = UiMode::Inventory;
        self.input.clear();
    }

    fn open_nearby_container(&mut self) -> bool {
        let player = self.campaign.simulation().player().position;
        let nearby = self
            .campaign
            .simulation()
            .containers()
            .filter_map(|container| {
                let position = self
                    .campaign
                    .simulation()
                    .entity(container.entity)?
                    .position;
                let distance = (position.grid.x - player.grid.x).abs()
                    + (position.grid.y - player.grid.y).abs();
                (position.map == player.map && position.grid.z == player.grid.z && distance <= 1)
                    .then_some((distance, container.entity))
            })
            .min()
            .map(|(_, entity)| entity);
        let Some(entity) = nearby else {
            return false;
        };
        if self
            .campaign
            .simulation()
            .container(entity)
            .is_some_and(|container| container.locked)
        {
            let lock_code = self
                .campaign
                .simulation()
                .container(entity)
                .and_then(|container| container.lock_code);
            let key = self
                .campaign
                .simulation()
                .player_inventory()
                .into_iter()
                .flat_map(|inventory| inventory.items.iter().copied())
                .find(|item| {
                    self.campaign.simulation().item(*item).is_some_and(
                        |item| matches!(item.kind, ItemKind::Key { lock_code: code } if Some(code) == lock_code),
                    )
                });
            let Some(key) = key else {
                let name = self
                    .campaign
                    .simulation()
                    .container(entity)
                    .map(|container| container.name.as_str())
                    .unwrap_or("container");
                self.push_message(format!("The {name} is locked."));
                return true;
            };
            let outcome = self
                .campaign
                .apply_game_command(GameCommand::UnlockContainer {
                    container: entity,
                    key,
                });
            self.process_simulation_outcome(outcome);
        }
        let outcome = self
            .campaign
            .apply_game_command(GameCommand::OpenContainer(entity));
        let opened = outcome.simulation.events.iter().any(
            |event| matches!(event, SimulationEvent::ContainerOpened { container } if *container == entity),
        );
        self.process_simulation_outcome(outcome);
        if opened {
            self.active_container = Some(ActiveContainer {
                entity,
                selected: 0,
                side: ContainerSide::Contents,
                quantity: 1,
            });
            self.ui_mode = UiMode::Container;
            self.input.clear();
        }
        true
    }

    fn active_container_item_ids(&self, side: ContainerSide) -> Vec<ItemId> {
        let owner = match side {
            ContainerSide::Contents => self.active_container.as_ref().map(|active| active.entity),
            ContainerSide::Pack => Some(self.campaign.simulation().player_id()),
        };
        owner
            .and_then(|owner| self.campaign.simulation().inventory(owner))
            .map(|inventory| inventory.items.iter().copied().collect())
            .unwrap_or_default()
    }

    fn move_container_selection(&mut self, offset: isize) {
        let Some(active) = self.active_container.as_ref() else {
            return;
        };
        let count = self.active_container_item_ids(active.side).len();
        if count == 0 {
            return;
        }
        if let Some(active) = self.active_container.as_mut() {
            active.selected =
                (active.selected as isize + offset).rem_euclid(count as isize) as usize;
            active.quantity = 1;
        }
    }

    fn switch_container_side(&mut self) {
        if let Some(active) = self.active_container.as_mut() {
            active.side = match active.side {
                ContainerSide::Contents => ContainerSide::Pack,
                ContainerSide::Pack => ContainerSide::Contents,
            };
            active.selected = 0;
            active.quantity = 1;
        }
    }

    fn adjust_container_quantity(&mut self, offset: i16) {
        let Some((selected, side, current)) = self
            .active_container
            .as_ref()
            .map(|container| (container.selected, container.side, container.quantity))
        else {
            return;
        };
        let maximum = self
            .active_container_item_ids(side)
            .get(selected)
            .and_then(|item| self.campaign.simulation().item(*item))
            .map(|item| item.quantity)
            .unwrap_or(1);
        if let Some(active) = self.active_container.as_mut() {
            active.quantity =
                (i32::from(current) + i32::from(offset)).clamp(1, i32::from(maximum)) as u16;
        }
    }

    fn transfer_container_item(&mut self) {
        let Some((container, selected, side, quantity)) = self
            .active_container
            .as_ref()
            .map(|active| (active.entity, active.selected, active.side, active.quantity))
        else {
            return;
        };
        let Some(item) = self.active_container_item_ids(side).get(selected).copied() else {
            return;
        };
        let command = match side {
            ContainerSide::Contents => GameCommand::TakeQuantity {
                item,
                from: container,
                quantity,
            },
            ContainerSide::Pack => GameCommand::PlaceQuantity {
                item,
                container,
                quantity,
            },
        };
        let outcome = self.campaign.apply_game_command(command);
        self.process_simulation_outcome(outcome);
        let count = self.active_container_item_ids(side).len();
        if let Some(active) = self.active_container.as_mut() {
            active.selected = active.selected.min(count.saturating_sub(1));
            active.quantity = 1;
        }
    }

    fn close_container(&mut self) {
        self.active_container = None;
        self.ui_mode = UiMode::Exploration;
        self.input.clear();
    }

    fn inventory_item_ids(&self) -> Vec<ItemId> {
        self.campaign
            .simulation()
            .player_inventory()
            .map(|inventory| inventory.items.iter().copied().collect())
            .unwrap_or_default()
    }

    fn move_inventory_selection(&mut self, offset: isize) {
        let count = self.inventory_item_ids().len();
        if count == 0 {
            return;
        }
        self.inventory_selected =
            (self.inventory_selected as isize + offset).rem_euclid(count as isize) as usize;
        self.inventory_quantity = 1;
    }

    fn adjust_inventory_quantity(&mut self, offset: i16) {
        let maximum = self
            .inventory_item_ids()
            .get(self.inventory_selected)
            .and_then(|item| self.campaign.simulation().item(*item))
            .map(|item| item.quantity)
            .unwrap_or(1);
        self.inventory_quantity = (i32::from(self.inventory_quantity) + i32::from(offset))
            .clamp(1, i32::from(maximum)) as u16;
    }

    fn activate_inventory_item(&mut self) {
        let Some(item) = self
            .inventory_item_ids()
            .get(self.inventory_selected)
            .copied()
        else {
            return;
        };
        let Some(kind) = self.campaign.simulation().item(item).map(|item| item.kind) else {
            return;
        };
        let command = match kind {
            ItemKind::MeleeWeapon { .. } | ItemKind::RangedWeapon { .. } => {
                GameCommand::Equip(item)
            }
            ItemKind::Consumable { .. } => GameCommand::UseItem(item),
            ItemKind::Food { .. } => GameCommand::Eat(item),
            ItemKind::Drink { .. } => GameCommand::Drink(item),
            ItemKind::Book { .. } => GameCommand::Read(item),
            ItemKind::Key { .. } => {
                self.push_message("Use this key while inspecting its matching locked container.");
                return;
            }
            ItemKind::Tool => {
                self.push_message("This tool has no direct use in the current situation.");
                return;
            }
            ItemKind::Ammunition { .. } => {
                self.push_message("Ammunition is consumed by its matching ranged weapon.");
                return;
            }
            ItemKind::Reagent { .. } => {
                self.push_message("This reagent is consumed by a matching magical formula.");
                return;
            }
            ItemKind::InscribedArtifact { formula, .. } => {
                if self
                    .campaign
                    .simulation()
                    .known_formulas()
                    .contains(&formula)
                {
                    GameCommand::Cast {
                        formula,
                        target: None,
                    }
                } else {
                    GameCommand::Study(item)
                }
            }
            ItemKind::Artifact => {
                self.push_message("This artifact is tied to a quest and cannot be used directly.");
                return;
            }
        };
        let outcome = self.campaign.apply_game_command(command);
        self.process_simulation_outcome(outcome);
    }

    fn drop_inventory_item(&mut self) {
        let Some(item) = self
            .inventory_item_ids()
            .get(self.inventory_selected)
            .copied()
        else {
            return;
        };
        let outcome = self.campaign.apply_game_command(GameCommand::DropQuantity {
            item,
            quantity: self.inventory_quantity,
        });
        self.process_simulation_outcome(outcome);
        let count = self.inventory_item_ids().len();
        self.inventory_selected = self.inventory_selected.min(count.saturating_sub(1));
        self.inventory_quantity = 1;
    }

    fn process_simulation_outcome(&mut self, outcome: CampaignOutcome) {
        self.lab_tracker
            .record(self.campaign.simulation(), &outcome.simulation);
        if outcome.simulation.advanced_time {
            self.next_tick = Instant::now() + WORLD_HEARTBEAT;
        }
        let resident_moves = outcome.resident_moves;
        let month_summaries = outcome.month_summaries;
        let campaign_events = outcome.campaign_events;
        for error in outcome.errors {
            self.push_message(format!("Campaign error: {error}"));
        }
        for event in outcome.simulation.events {
            match event {
                SimulationEvent::Damaged {
                    attacker,
                    target,
                    amount,
                    remaining_health,
                    method,
                } => {
                    let attacker_name = self.entity_name(attacker);
                    let target_name = self.entity_name(target);
                    let player = self.campaign.simulation().player_id();
                    let message = if attacker == player {
                        let verb = match method {
                            CombatMethod::Melee => "strike",
                            CombatMethod::Ranged => "hit",
                            CombatMethod::Magic => "sear",
                            CombatMethod::Retaliation => "retaliate against",
                        };
                        format!(
                            "You {verb} {target_name} for {amount}; {remaining_health} health remains."
                        )
                    } else if target == player {
                        let verb = match method {
                            CombatMethod::Melee => "strikes",
                            CombatMethod::Ranged => "hits",
                            CombatMethod::Magic => "sears",
                            CombatMethod::Retaliation => "strikes",
                        };
                        format!(
                            "{attacker_name} {verb} you for {amount}; you have {remaining_health} health."
                        )
                    } else {
                        format!(
                            "{attacker_name} hits {target_name} for {amount}; {remaining_health} health remains."
                        )
                    };
                    self.push_message(message);
                }
                SimulationEvent::Defeated { entity, by } => {
                    let name = self.entity_name(entity);
                    let victor = self.entity_name(by);
                    if entity == self.campaign.simulation().player_id() {
                        self.push_message(format!("You are defeated by {victor}."));
                    } else if by == self.campaign.simulation().player_id() {
                        self.push_message(format!("{name} is defeated by you."));
                    } else {
                        self.push_message(format!("{name} is defeated by {victor}."));
                    }
                }
                SimulationEvent::ItemEquipped { owner, item } => {
                    if owner == self.campaign.simulation().player_id() {
                        let name = &self
                            .campaign
                            .simulation()
                            .item(item)
                            .map(|item| item.name.as_str())
                            .unwrap_or("item");
                        self.push_message(format!("You equip {name}."));
                    }
                }
                SimulationEvent::ItemTransferred { item, to, .. } => {
                    if to == self.campaign.simulation().player_id() {
                        let item_record = self
                            .campaign
                            .simulation()
                            .item(item)
                            .map(|item| (item.name.clone(), item.kind));
                        let name = item_record
                            .as_ref()
                            .map(|(name, _)| name.as_str())
                            .unwrap_or("item");
                        self.push_message(format!("You receive {name}."));
                    }
                }
                SimulationEvent::ItemQuantityTransferred {
                    item, quantity, to, ..
                } => {
                    if to == self.campaign.simulation().player_id() {
                        let name = self
                            .campaign
                            .simulation()
                            .item(item)
                            .map(|item| item.name.as_str())
                            .unwrap_or("item");
                        self.push_message(format!("You receive {name} x{quantity}."));
                    }
                }
                SimulationEvent::ItemConsumed {
                    owner,
                    item,
                    remaining,
                } => {
                    if owner == self.campaign.simulation().player_id()
                        && self.campaign.simulation().item(item).is_some_and(|item| {
                            matches!(
                                item.kind,
                                ItemKind::Consumable { .. }
                                    | ItemKind::Food { .. }
                                    | ItemKind::Drink { .. }
                            )
                        })
                    {
                        let name = self
                            .campaign
                            .simulation()
                            .item(item)
                            .expect("item checked")
                            .name
                            .as_str();
                        self.push_message(format!("{name}: {remaining} remaining."));
                    }
                }
                SimulationEvent::ContainerOpened { .. } => {}
                SimulationEvent::ContainerUnlocked { container, .. } => {
                    let name = self
                        .campaign
                        .simulation()
                        .container(container)
                        .map(|container| container.name.as_str())
                        .unwrap_or("container");
                    self.push_message(format!("You unlock the {name}."));
                }
                SimulationEvent::ItemDropped { item, .. } => {
                    let name = self
                        .campaign
                        .simulation()
                        .item(item)
                        .map(|item| item.name.as_str())
                        .unwrap_or("item");
                    self.push_message(format!("You drop {name}."));
                }
                SimulationEvent::ItemRead {
                    item,
                    newly_learned,
                    ..
                } => {
                    let name = self
                        .campaign
                        .simulation()
                        .item(item)
                        .map(|item| item.name.as_str())
                        .unwrap_or("book");
                    self.push_message(if newly_learned {
                        format!("You read {name} and record what you learned.")
                    } else {
                        format!("You reread {name}.")
                    });
                }
                SimulationEvent::NeedsChanged { .. } => {}
                SimulationEvent::Healed {
                    entity,
                    amount,
                    health,
                } => {
                    if entity == self.campaign.simulation().player_id() {
                        self.push_message(format!(
                            "You recover {amount} health and now have {health}."
                        ));
                    }
                }
                SimulationEvent::FormulaLearned { formula, .. } => {
                    let formula_rule = self.campaign.simulation().rules().formula(formula).cloned();
                    let name = formula_rule
                        .as_ref()
                        .map(|formula| formula.name.as_str())
                        .unwrap_or("Unknown formula");
                    self.push_message(format!(
                        "You reconstruct {name}. Its reagents and condition are now recorded."
                    ));
                }
                SimulationEvent::SpellCast {
                    formula, effect, ..
                } => {
                    let name = self
                        .campaign
                        .simulation()
                        .rules()
                        .formula(formula)
                        .map(|formula| formula.name.as_str())
                        .unwrap_or("the formula");
                    self.push_message(format!("You perform {name}: {}.", effect.name()));
                }
                SimulationEvent::RevivedAtHealer {
                    entity,
                    healer,
                    health,
                    ..
                } => {
                    if entity == self.campaign.simulation().player_id() {
                        let healer_name = self.entity_name(healer);
                        self.ui_mode = UiMode::Exploration;
                        self.targeting = None;
                        self.active_conversation = None;
                        self.push_message(format!(
                            "You awaken beside {healer_name}, restored to {health} health."
                        ));
                    }
                }
                SimulationEvent::Traversed { kind, destination } => {
                    let verb = match kind {
                        ultimate_fate_core::TransitionKind::Descend => "descend",
                        ultimate_fate_core::TransitionKind::Ascend => "ascend",
                    };
                    self.push_message(format!(
                        "You {verb} to depth {}.",
                        i32::from(destination.grid.z).unsigned_abs()
                    ));
                }
                SimulationEvent::ExperienceGained { amount, total } => {
                    self.push_message(format!("You gain {amount} experience ({total} total)."));
                }
                SimulationEvent::LevelGained {
                    level,
                    max_health,
                    attack_bonus,
                } => {
                    self.push_message(format!(
                        "You reach level {level}: health {max_health}, attack bonus +{attack_bonus}."
                    ));
                }
                SimulationEvent::QuestAdvanced { quest, objective } => {
                    let title = self
                        .campaign
                        .simulation()
                        .quest(quest)
                        .map(|quest| quest.title.as_str())
                        .unwrap_or("Quest");
                    self.push_message(format!("{title}: objective {} completed.", objective + 1));
                }
                SimulationEvent::QuestReadyToTurnIn { quest } => {
                    let title = self
                        .campaign
                        .simulation()
                        .quest(quest)
                        .map(|quest| quest.title.as_str())
                        .unwrap_or("Quest");
                    self.push_message(format!("{title}: return to the quest giver."));
                }
                SimulationEvent::QuestCompleted { quest } => {
                    let title = self
                        .campaign
                        .simulation()
                        .quest(quest)
                        .map(|quest| quest.title.as_str())
                        .unwrap_or("Quest");
                    self.push_message(format!("{title} completed."));
                    if quest == self.campaign.site_plan().dungeon.quest {
                        self.push_message(
                            "The recovered artifact and cleared depths enter public history.",
                        );
                    }
                }
                SimulationEvent::ActionFailed(reason) => {
                    self.push_message(action_failure_text(reason));
                }
            }
        }
        for event in campaign_events {
            match event {
                CampaignEvent::Conversation { .. } => {}
                CampaignEvent::EvidenceInspected {
                    name, description, ..
                } => self.push_message(format!("You inspect {name}: {description}.")),
                CampaignEvent::ItemGifted { item, .. } => {
                    let name = self
                        .campaign
                        .simulation()
                        .item(item)
                        .map(|item| item.name.as_str())
                        .unwrap_or("item");
                    self.push_message(format!(
                        "{} lends you the {name}.",
                        self.campaign.site_plan().contact_name
                    ));
                }
                CampaignEvent::AidSupported { advocate, patient } => {
                    self.push_message(format!(
                        "{} agrees to support care for {}.",
                        self.person_name(advocate),
                        self.person_name(patient)
                    ));
                }
                CampaignEvent::ItemAcquired { item, from, method } => {
                    let name = self
                        .campaign
                        .simulation()
                        .item(item)
                        .map(|item| item.name.as_str())
                        .unwrap_or("medicine");
                    self.push_message(format!(
                        "You receive {name} from {} by {}.",
                        self.person_name(from),
                        aid_method_name(method)
                    ));
                }
                CampaignEvent::ItemTraded {
                    item,
                    direction,
                    quantity,
                    price,
                    ..
                } => {
                    let name = self
                        .campaign
                        .simulation()
                        .item(item)
                        .map(|item| item.name.as_str())
                        .unwrap_or("item");
                    self.push_message(match direction {
                        TradeDirection::Buy => {
                            format!("You buy {name} x{quantity} for {price} coin.")
                        }
                        TradeDirection::Sell => {
                            format!("You sell {name} x{quantity} for {price} coin.")
                        }
                    });
                }
                CampaignEvent::AidDelivered {
                    patient, method, ..
                } => {
                    self.push_message(format!(
                        "{} receives treatment. The outcome enters local history as {}.",
                        self.person_name(patient),
                        aid_method_name(method)
                    ));
                }
                CampaignEvent::CrisisResolved(resolution) => {
                    self.push_message(format!(
                        "Your intervention becomes public record: {}.",
                        resolution.summary
                    ));
                    if let Some(active) = self.active_resolution.as_mut() {
                        active.outcome = Some(resolution);
                    }
                }
                CampaignEvent::RegionalGoalResolved(resolution) => {
                    self.push_message(format!("REGIONAL OUTCOME: {}.", resolution.summary));
                    self.tracked_regional_goal = None;
                }
                CampaignEvent::StandingChanged { amount, reason, .. } => {
                    self.push_message(format!("Faction standing {amount:+}: {reason}."));
                }
                CampaignEvent::ActionRejected(reason) => self.push_message(reason),
            }
        }
        let progress = self.campaign.progress().clone();
        self.met_contact = progress.met_contact;
        self.inspected_evidence = progress.inspected_evidence;
        self.questioned_factions = progress.questioned_factions;
        self.learned_topics = progress.learned_topics;
        self.received_starter_sword = progress.received_starter_sword;
        self.resolved_crisis = progress.resolved_crisis;
        self.aftermath_complete = progress.aftermath_complete;
        if resident_moves > 0 {
            self.lab_tracker.record_local_npc_moves(resident_moves);
        }
        for month in month_summaries {
            if self.tracked_regional_goal.is_some_and(|goal| {
                self.campaign
                    .history()
                    .world()
                    .regional_goals()
                    .get(&goal)
                    .is_none_or(|goal| goal.status != RegionalGoalStatus::Open)
            }) {
                self.tracked_regional_goal = None;
            }
            if !month.events.is_empty() {
                let mut alerts = Vec::new();
                for event in month.events {
                    let record = &self.campaign.history().world().events()[&event];
                    let summary = record.summary.clone();
                    if direct_world_alert(record.kind) {
                        alerts.push(summary);
                    }
                }
                if let Some(first) = alerts.first() {
                    let more = alerts.len().saturating_sub(1);
                    let first = compact_notification(first, 12);
                    self.push_message(if more == 0 {
                        format!("WORLD ALERT: {first}.")
                    } else {
                        format!("WORLD ALERT: {first}. +{more} more in Journal.")
                    });
                }
            }
        }
    }

    fn regional_party_for_entity(&self, entity: EntityId) -> Option<PartyId> {
        self.campaign
            .history()
            .world()
            .regional_parties()
            .keys()
            .copied()
            .find(|party| PlayableSitePlan::regional_party_entity(*party) == entity)
    }

    fn entity_name(&self, entity: EntityId) -> String {
        if entity == self.campaign.simulation().player_id() {
            return "You".to_string();
        }
        if let Some(party) = self.regional_party_for_entity(entity) {
            return self.campaign.history().world().regional_parties()[&party]
                .name
                .clone();
        }
        if entity == self.campaign.site_plan().encounter.entity {
            return self.campaign.site_plan().encounter.name.clone();
        }
        if let Some(enemy) = self
            .campaign
            .site_plan()
            .dungeon
            .levels
            .iter()
            .flat_map(|level| level.enemies.iter())
            .find(|enemy| enemy.entity == entity)
        {
            return enemy.name.clone();
        }
        self.campaign
            .site_plan()
            .residents
            .iter()
            .find(|resident| resident.entity == entity)
            .map(|resident| resident.name.clone())
            .unwrap_or_else(|| "Unknown combatant".to_string())
    }

    fn person_name(&self, person: PersonId) -> String {
        self.campaign
            .site_plan()
            .residents
            .iter()
            .find(|resident| resident.person == person)
            .map(|resident| resident.name.clone())
            .unwrap_or_else(|| format!("Person {}", person.0))
    }

    fn inspect(&mut self) {
        if self.open_nearby_container() {
            return;
        }
        let player = self.campaign.simulation().player().position;
        if player.map == self.campaign.site_plan().regional_map {
            let nearby_party = self
                .campaign
                .history()
                .world()
                .regional_parties()
                .values()
                .filter(|party| {
                    matches!(
                        party.status,
                        RegionalPartyStatus::Traveling | RegionalPartyStatus::Stationed
                    )
                })
                .filter_map(|party| {
                    let entity = PlayableSitePlan::regional_party_entity(party.id);
                    let position = self.campaign.simulation().entity(entity)?.position;
                    let distance = (position.grid.x - player.grid.x).abs()
                        + (position.grid.y - player.grid.y).abs();
                    (distance <= 2).then_some((distance, party))
                })
                .min_by_key(|(distance, party)| (*distance, party.id))
                .map(|(_, party)| party.clone());
            if let Some(party) = nearby_party {
                let purpose = match party.kind {
                    RegionalPartyKind::TradeCaravan { resource, amount } => {
                        format!("carrying {amount} {resource:?}")
                    }
                    RegionalPartyKind::ReturningCaravan => {
                        "returning after a completed delivery".to_string()
                    }
                    RegionalPartyKind::Refugees { population } => {
                        format!("guiding {population} displaced people")
                    }
                    RegionalPartyKind::Patrol { .. } => "patrolling the road".to_string(),
                    RegionalPartyKind::Raiders { strength } => {
                        format!("a hostile raiding force of strength {strength}")
                    }
                };
                let leader = party
                    .leader
                    .and_then(|leader| {
                        let person = self.campaign.history().world().people().get(&leader)?;
                        let family = self
                            .campaign
                            .history()
                            .world()
                            .families()
                            .get(&person.family)?;
                        Some(format!("{} {}", person.given_name, family.surname))
                    })
                    .unwrap_or_else(|| "no known leader".to_string());
                let route = &self.campaign.history().world().routes()[&party.route].name;
                self.push_message(format!(
                    "{}: {purpose}, led by {leader}, on {route}. Its journey began with event {}.",
                    party.name, party.cause
                ));
                return;
            }
            let nearby_history = self
                .campaign
                .site_plan()
                .regional_history_sites
                .iter()
                .filter_map(|site| {
                    let distance = (site.position.x - player.grid.x).abs()
                        + (site.position.y - player.grid.y).abs();
                    (distance <= 2).then_some((distance, site))
                })
                .min_by_key(|(distance, site)| (*distance, site.event))
                .map(|(_, site)| site.clone());
            if let Some(site) = nearby_history {
                let outcome = self
                    .campaign
                    .apply_command(CampaignCommand::InspectHistoricalSite(site.event));
                self.process_simulation_outcome(outcome);
                return;
            }
        }
        if player.map == self.campaign.site_plan().map
            && player.grid.z == 0
            && let Some((resident, distance)) = self
                .campaign
                .site_plan()
                .residents
                .iter()
                .filter_map(|resident| {
                    let position = self.campaign.simulation().entity(resident.entity)?.position;
                    let distance = (position.grid.x - player.grid.x).abs()
                        + (position.grid.y - player.grid.y).abs();
                    (distance <= 2).then_some((resident, distance))
                })
                .min_by_key(|(resident, distance)| (*distance, resident.person))
        {
            let activity = self
                .campaign
                .resident_agents()
                .get(&resident.person)
                .map_or("going about their day", |agent| match agent.goal {
                    ResidentGoal::Work => "working",
                    ResidentGoal::Socialize => "seeking company",
                    ResidentGoal::Rest => "resting",
                    ResidentGoal::SeekFood => "trying to secure food",
                    ResidentGoal::SeekSafety => "seeking safety",
                });
            self.push_message(format!(
                "{} is a {} and is currently {activity} ({} step{} away).",
                resident.name,
                occupation_name(resident.occupation),
                distance,
                if distance == 1 { "" } else { "s" }
            ));
            return;
        }
        if player.grid.z == 0
            && self
                .campaign
                .simulation()
                .combatant(self.campaign.site_plan().encounter.entity)
                .is_some_and(|combatant| combatant.is_alive())
        {
            let encounter_position = self.campaign.site_plan().encounter.position;
            let distance = (encounter_position.x - player.grid.x).abs()
                + (encounter_position.y - player.grid.y).abs();
            if distance <= 7 {
                let health = self
                    .campaign
                    .simulation()
                    .combatant(self.campaign.site_plan().encounter.entity);
                let health = health
                    .map(|combatant| {
                        format!("{}/{} health", combatant.health, combatant.max_health)
                    })
                    .unwrap_or_default();
                self.push_message(format!(
                    "{}: {} {}.",
                    self.campaign.site_plan().encounter.name,
                    self.campaign.site_plan().encounter.description,
                    health
                ));
                return;
            }
        }
        if player.grid.z < 0
            && let Some(level) = self
                .campaign
                .site_plan()
                .dungeon
                .levels
                .iter()
                .find(|level| level.entry.z == player.grid.z)
        {
            self.push_message(format!("{}: {}", level.name, level.historical_context));
            return;
        }
        let evidence = self.campaign.site_plan().evidence_location();
        let evidence_distance = (evidence.position.x - player.grid.x).abs()
            + (evidence.position.y - player.grid.y).abs();
        if player.grid.z == 0 && evidence_distance <= 1 {
            let event = self.campaign.site_plan().evidence_event;
            let outcome = self
                .campaign
                .apply_command(CampaignCommand::InspectEvidence(event));
            self.process_simulation_outcome(outcome);
            return;
        }
        if player.grid.z == 0
            && let Some(planned) =
                self.campaign
                    .site_plan()
                    .living_projects
                    .iter()
                    .find(|planned| {
                        (planned.position.x - player.grid.x).abs()
                            + (planned.position.y - player.grid.y).abs()
                            <= 2
                    })
            && let Some(project) = self
                .campaign
                .history()
                .world()
                .projects()
                .get(&planned.project)
        {
            let sponsor = &self.campaign.history().world().factions()[&project.sponsor].name;
            let workers = project
                .workers
                .iter()
                .filter_map(|worker| {
                    let person = self.campaign.history().world().people().get(worker)?;
                    let family = self
                        .campaign
                        .history()
                        .world()
                        .families()
                        .get(&person.family)?;
                    Some(format!("{} {}", person.given_name, family.surname))
                })
                .collect::<Vec<_>>()
                .join(", ");
            let message = format!(
                "{}: {}. Sponsored by {}. Workers: {}. Progress {}/{} months; damage and repairs consume real settlement resources.",
                project.name,
                project_phase_name(project.phase),
                sponsor,
                workers,
                project.progress_months,
                project.required_months
            );
            self.push_message(message);
            return;
        }

        let terrain = self
            .campaign
            .simulation()
            .map(player.map)
            .and_then(|map| map.cell(player.grid))
            .map(|cell| terrain_name(cell.terrain))
            .unwrap_or("unknown ground");
        self.push_message(format!("You inspect the {terrain}."));
    }

    fn toggle_pause(&mut self) {
        let outcome = self.campaign.apply_game_command(GameCommand::Pause);
        self.process_simulation_outcome(outcome);
        if self.campaign.simulation().paused {
            self.push_message("Time is paused.");
        } else {
            self.push_message("Time resumes.");
        }
    }

    fn push_message(&mut self, message: impl Into<String>) {
        let message = message.into();
        if message.trim().is_empty()
            || self
                .message_log
                .last()
                .is_some_and(|previous| previous == &message)
        {
            return;
        }
        self.message_log.push(message);
        if self.message_log.len() > 12 {
            self.message_log.remove(0);
        }
    }

    fn render(&self) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let viewport = ViewportSize {
            width: self.config.width,
            height: self.config.height,
        };
        let player = self.campaign.simulation().player();
        let map_scale = if player.position.map == self.campaign.site_plan().regional_map {
            REGIONAL_VIEW_SCALE
        } else {
            1.0
        };
        let projection = OverheadProjection {
            cell_size: self.projection.cell_size * map_scale * self.window.scale_factor() as f32,
        };
        let half_width = (self.config.width as f32 / projection.cell_size * 0.5).ceil() as i32 + 1;
        let half_height =
            (self.config.height as f32 / projection.cell_size * 0.5).ceil() as i32 + 1;
        let snapshot = PresentationSnapshot::from_simulation(
            self.campaign.simulation(),
            ViewportRequest {
                map: player.position.map,
                center: player.position.grid,
                half_width,
                half_height,
                z: player.position.grid.z,
            },
        );
        let scale = self.window.scale_factor() as f32;
        let sidebar_width = (self.config.width as f32 * 0.36)
            .clamp(300.0 * scale, 420.0 * scale)
            .min((self.config.width as f32 - 220.0 * scale).max(0.0));
        let log_height = (self.config.height as f32 * 0.17).clamp(92.0 * scale, 120.0 * scale);
        let mut draw_list = projection.project(&snapshot, viewport, &self.art_pack);
        draw_list.camera_center[0] += sidebar_width * 0.5 / projection.cell_size;
        draw_list.camera_center[1] += log_height * 0.5 / projection.cell_size;
        let ui_draw_list = self.build_ui(viewport, &snapshot, projection.cell_size);
        self.renderer.render(
            &self.device,
            &self.queue,
            &view,
            viewport,
            &draw_list,
            &ui_draw_list,
        );
        frame.present();
        Ok(())
    }

    fn build_ui(
        &self,
        viewport: ViewportSize,
        snapshot: &PresentationSnapshot,
        cell_size: f32,
    ) -> UiDrawList {
        let mut ui = UiDrawList::default();
        let width = viewport.width as f32;
        let height = viewport.height as f32;
        let scale = self.window.scale_factor() as f32;
        let sidebar_width = (width * 0.36)
            .clamp(300.0 * scale, 420.0 * scale)
            .min((width - 220.0 * scale).max(0.0));
        let log_height = (height * 0.17).clamp(92.0 * scale, 120.0 * scale);
        let sidebar = UiRect::new(width - sidebar_width, 0.0, sidebar_width, height);
        let message_log = UiRect::new(0.0, height - log_height, width - sidebar_width, log_height);
        let panel_fill = [0.035, 0.045, 0.055, 0.96];
        let panel_border = [0.52, 0.43, 0.24, 1.0];
        let text = UiTextStyle {
            pixel_scale: 2.0 * scale,
            line_spacing: 4.0 * scale,
            ..Default::default()
        };
        let heading = UiTextStyle {
            color: [0.95, 0.78, 0.32, 1.0],
            pixel_scale: 2.0 * scale,
            line_spacing: 5.0 * scale,
        };

        self.draw_landmark_labels(
            &mut ui,
            snapshot,
            cell_size,
            width - sidebar_width,
            height - log_height,
            scale,
        );
        self.draw_map_layer_hint(&mut ui, snapshot, width - sidebar_width, scale);
        self.draw_targeting_overlay(
            &mut ui,
            snapshot,
            cell_size,
            width - sidebar_width,
            height - log_height,
            scale,
        );
        ui.bordered_panel(sidebar, panel_fill, panel_border, 2.0 * scale);
        ui.bordered_panel(message_log, panel_fill, panel_border, 2.0 * scale);

        self.draw_sidebar(&mut ui, sidebar, scale);

        ui.text(
            UiRect::new(
                message_log.x + 14.0 * scale,
                message_log.y + 12.0 * scale,
                message_log.width - 28.0 * scale,
                20.0 * scale,
            ),
            "RECENT",
            heading,
        );
        let recent_messages = self
            .message_log
            .iter()
            .rev()
            .take(4)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        ui.tail_text(
            UiRect::new(
                message_log.x + 14.0 * scale,
                message_log.y + 40.0 * scale,
                message_log.width - 28.0 * scale,
                message_log.height - 52.0 * scale,
            ),
            recent_messages,
            text,
        );

        match self.ui_mode {
            UiMode::Briefing => self.draw_briefing_overlay(&mut ui, width, height, scale),
            UiMode::Journal => self.draw_journal_overlay(&mut ui, width, height, scale),
            UiMode::Region => self.draw_region_overlay(&mut ui, width, height, scale),
            UiMode::Conversation => {
                self.draw_conversation_overlay(&mut ui, width, height, scale);
            }
            UiMode::Inventory => self.draw_inventory_overlay(&mut ui, width, height, scale),
            UiMode::Container => self.draw_container_overlay(&mut ui, width, height, scale),
            UiMode::Trade => self.draw_trade_overlay(&mut ui, width, height, scale),
            UiMode::Resolution => self.draw_resolution_overlay(&mut ui, width, height, scale),
            UiMode::Exploration | UiMode::Targeting => {}
        }
        ui
    }

    fn draw_sidebar(&self, ui: &mut UiDrawList, sidebar: UiRect, scale: f32) {
        let inner = sidebar.inset(14.0 * scale);
        let compact = inner.height / scale < 620.0;
        let content = self.sidebar_content(compact);
        let mut y = inner.y;
        let header_height = inner.height * 0.07;
        ui.text(
            UiRect::new(inner.x, y, inner.width, header_height * 0.58),
            content.location,
            UiTextStyle {
                color: [1.0, 0.82, 0.34, 1.0],
                pixel_scale: if compact { 1.8 * scale } else { 2.1 * scale },
                line_spacing: 3.0 * scale,
            },
        );
        ui.text(
            UiRect::new(
                inner.x,
                y + header_height * 0.52,
                inner.width,
                header_height * 0.45,
            ),
            content.date,
            UiTextStyle {
                color: [0.68, 0.74, 0.76, 1.0],
                pixel_scale: if compact { 1.3 * scale } else { 1.5 * scale },
                line_spacing: 2.0 * scale,
            },
        );
        y += header_height;

        let mut sections = vec![
            (
                "STATUS",
                content.status.as_str(),
                [0.42, 0.88, 0.54, 1.0],
                0.16_f32,
            ),
            (
                "CURRENT LEAD",
                content.lead.as_str(),
                [1.0, 0.76, 0.24, 1.0],
                0.24,
            ),
            (
                "CONTROLS",
                content.controls.as_str(),
                [0.48, 0.70, 1.0, 1.0],
                0.22,
            ),
        ];
        if !content.context.is_empty() {
            sections.insert(
                2,
                (
                    "WORLD ALERT",
                    content.context.as_str(),
                    [0.78, 0.58, 0.92, 1.0],
                    0.18,
                ),
            );
        }
        if content.threat != "No immediate threat" {
            sections.insert(
                1,
                (
                    "IMMEDIATE THREAT",
                    content.threat.as_str(),
                    [0.96, 0.38, 0.28, 1.0],
                    0.13,
                ),
            );
        }
        let section_count = sections.len();
        let remaining_height = inner.y + inner.height - y;
        let total_weight = sections
            .iter()
            .map(|(_, _, _, weight)| *weight)
            .sum::<f32>();
        for (index, (title, body, color, weight)) in sections.into_iter().enumerate() {
            let is_last = index + 1 == section_count;
            let height = if is_last {
                inner.y + inner.height - y
            } else {
                remaining_height * weight / total_weight
            };
            draw_sidebar_section(
                ui,
                UiRect::new(inner.x, y, inner.width, height.max(0.0)),
                title,
                body,
                color,
                scale,
                compact,
            );
            y += height;
        }
    }

    fn sidebar_content(&self, _compact: bool) -> SidebarContent {
        let site =
            &self.campaign.history().world().sites()[&self.campaign.history().primary_site()];
        let player = self.campaign.simulation().player().position;
        let health = self
            .campaign
            .simulation()
            .player_combatant()
            .map(|combatant| format!("{}/{}", combatant.health, combatant.max_health))
            .unwrap_or_else(|| "--".to_string());
        let inventory = self.campaign.simulation().player_inventory();
        let melee = inventory
            .and_then(|inventory| inventory.equipped_melee)
            .and_then(|item| self.campaign.simulation().item(item))
            .map(|item| item.name.as_str())
            .unwrap_or("Unarmed");
        let ranged = inventory
            .and_then(|inventory| inventory.equipped_ranged)
            .and_then(|item| self.campaign.simulation().item(item))
            .map(|item| item.name.as_str())
            .unwrap_or("None");
        let arrows: u16 = inventory
            .into_iter()
            .flat_map(|inventory| inventory.items.iter())
            .filter_map(|item| self.campaign.simulation().item(*item))
            .filter(|item| matches!(item.kind, ItemKind::Ammunition { .. }))
            .map(|item| item.quantity)
            .sum();
        let progression = self.campaign.simulation().progression();
        let progress = format!(
            "LEVEL {}  XP {}/{}",
            progression.level,
            progression.experience,
            progression.experience_for_next_level()
        );
        let mut status = format!(
            "{progress}   HEALTH {health}   COIN {}\nMELEE  {melee}\nRANGED  {ranged} / {arrows}",
            self.campaign.progress().player_coin
        );
        let needs = self.campaign.simulation().player_needs();
        if needs.hunger >= 60 || needs.thirst >= 60 {
            status.push_str(&format!(
                "\nNEEDS  HUNGER {}  THIRST {}",
                needs.hunger, needs.thirst
            ));
        }
        let threat =
            self.campaign
                .simulation()
                .entities()
                .filter(|entity| {
                    entity.position.map == player.map
                        && entity.position.grid.z == player.grid.z
                        && self.campaign.simulation().combatant(entity.id).is_some_and(
                            |combatant| combatant.hostile_to_player && combatant.is_alive(),
                        )
                })
                .map(|entity| {
                    let dx = entity.position.grid.x - player.grid.x;
                    let dy = entity.position.grid.y - player.grid.y;
                    (dx.abs() + dy.abs(), entity.id, dx, dy)
                })
                .filter(|(distance, _, _, _)| *distance <= 10)
                .min()
                .and_then(|(_, entity, dx, dy)| {
                    let combatant = self.campaign.simulation().combatant(entity)?;
                    Some(format!(
                        "{}  {}\nHEALTH  {}/{}",
                        self.entity_name(entity),
                        relative_steps(dx, dy),
                        combatant.health,
                        combatant.max_health
                    ))
                })
                .unwrap_or_else(|| "No immediate threat".to_string());
        let tracked_goal = self
            .tracked_regional_goal
            .and_then(|goal| self.campaign.history().world().regional_goals().get(&goal))
            .filter(|goal| goal.status == RegionalGoalStatus::Open);
        let active_quest = self
            .campaign
            .simulation()
            .quests()
            .find(|quest| quest.status != QuestStatus::Completed);
        let objective = if let Some(goal) = tracked_goal {
            let approach = self
                .campaign
                .history()
                .regional_goal_options(goal.id)
                .ok()
                .and_then(|options| {
                    options
                        .get(self.regional_option_selected)
                        .map(|option| option.approach)
                })
                .unwrap_or(RegionalGoalApproach::NegotiatePassage);
            let (target_name, target) = self
                .regional_goal_target_for_approach(goal.kind, approach)
                .unwrap_or_else(|| ("regional destination".to_string(), GridPos::new(0, 0, 0)));
            if player.map == self.campaign.site_plan().regional_map {
                format!(
                    "{}\n{}  {}",
                    goal.title,
                    target_name,
                    relative_steps(target.x - player.grid.x, target.y - player.grid.y)
                )
            } else {
                let gate = self.campaign.site_plan().nearest_regional_gate(player.grid);
                format!(
                    "{}\nREGIONAL GATE  {}",
                    goal.title,
                    relative_steps(gate.x - player.grid.x, gate.y - player.grid.y)
                )
            }
        } else if self.campaign.aid_delivery().is_none() {
            let aid = &self.campaign.site_plan().aid;
            let target = if self
                .campaign
                .simulation()
                .player_inventory()
                .is_some_and(|inventory| inventory.items.contains(&aid.medicine.id))
            {
                (
                    aid.patient_name.as_str(),
                    aid.patient_entity,
                    "Deliver treatment to",
                )
            } else if self
                .campaign
                .progress()
                .aid_supporters
                .contains(&aid.advocate)
            {
                (
                    aid.custodian_name.as_str(),
                    aid.custodian_entity,
                    "Appeal to",
                )
            } else {
                (
                    aid.advocate_name.as_str(),
                    aid.advocate_entity,
                    "Seek support from",
                )
            };
            let direction = self
                .campaign
                .simulation()
                .entity(target.1)
                .filter(|entity| entity.position.map == player.map)
                .map(|entity| {
                    relative_steps(
                        entity.position.grid.x - player.grid.x,
                        entity.position.grid.y - player.grid.y,
                    )
                })
                .unwrap_or_else(|| "OFF MAP".to_string());
            format!("{}\n{} {}  {}", aid.title, target.2, target.0, direction)
        } else if let Some(quest) = active_quest {
            match quest.status {
                QuestStatus::Active => {
                    let objective = quest
                        .objectives
                        .iter()
                        .find(|objective| !objective.completed)
                        .map(|objective| objective.description.as_str())
                        .unwrap_or("Explore the dungeon");
                    if player.grid.z == 0 {
                        let dx = self.campaign.site_plan().dungeon.entrance.x - player.grid.x;
                        let dy = self.campaign.site_plan().dungeon.entrance.y - player.grid.y;
                        format!(
                            "{}\n{}  {}",
                            objective,
                            self.campaign.site_plan().dungeon.name,
                            relative_steps(dx, dy)
                        )
                    } else {
                        format!("{}\nDEPTH {}", objective, player.grid.z.unsigned_abs())
                    }
                }
                QuestStatus::ReadyToTurnIn => {
                    format!("Return to {}", self.campaign.site_plan().contact_name)
                }
                QuestStatus::Completed => unreachable!("completed quests filtered out"),
            }
        } else if let Some(outcome) = &self.resolved_crisis {
            if self.aftermath_complete {
                format!(
                    "Watch for new consequences of {}",
                    crisis_resolution_label(outcome.kind)
                )
            } else {
                let location = self
                    .campaign
                    .site_plan()
                    .locations
                    .iter()
                    .find(|location| {
                        location.source == LocationSource::FactionSeat(outcome.reaction_faction)
                    })
                    .map(|location| location.name.as_str())
                    .unwrap_or("the faction seat");
                format!("{} at {location}", outcome.aftermath_prompt)
            }
        } else if !self.met_contact {
            let resident = self.campaign.site_plan().contact_resident();
            self.campaign
                .simulation()
                .entity(resident.entity)
                .filter(|contact| contact.position.map == player.map)
                .map(|contact| {
                    let dx = contact.position.grid.x - player.grid.x;
                    let dy = contact.position.grid.y - player.grid.y;
                    if contact.position.grid == resident.position {
                        self.campaign.site_plan().first_objective()
                    } else {
                        format!(
                            "Find {} — currently {}",
                            resident.name,
                            relative_steps(dx, dy)
                        )
                    }
                })
                .unwrap_or_else(|| self.campaign.site_plan().first_objective())
        } else if !self.inspected_evidence {
            self.campaign.site_plan().evidence_objective()
        } else if self.questioned_factions.is_empty() {
            format!(
                "Compare accounts concerning {}",
                self.campaign.site_plan().evidence_location
            )
        } else if self.questioned_factions.len() == 1 {
            format!(
                "Question another faction about {}",
                self.campaign.site_plan().evidence_location
            )
        } else {
            "Decide which account deserves your support".to_string()
        };
        let controls = self.input_prompts.legend();
        let mut projects = self
            .campaign
            .history()
            .world()
            .projects()
            .values()
            .collect::<Vec<_>>();
        projects.sort_by_key(|project| {
            let urgency = match project.phase {
                SettlementProjectPhase::Damaged => 0,
                SettlementProjectPhase::Stalled => 1,
                SettlementProjectPhase::Structure => 2,
                SettlementProjectPhase::Foundation => 3,
                SettlementProjectPhase::Planned => 4,
                SettlementProjectPhase::Completed => 5,
            };
            (urgency, project.id)
        });
        let project_alert = projects
            .into_iter()
            .find(|project| {
                matches!(
                    project.phase,
                    SettlementProjectPhase::Damaged | SettlementProjectPhase::Stalled
                )
            })
            .map(|project| {
                let direction = self
                    .campaign
                    .site_plan()
                    .living_projects
                    .iter()
                    .find(|planned| planned.project == project.id)
                    .map(|planned| {
                        relative_steps(
                            planned.position.x - player.grid.x,
                            planned.position.y - player.grid.y,
                        )
                    })
                    .unwrap_or_else(|| "OFF MAP".to_string());
                format!("WORK HALTED: {}\n{}", project.name, direction)
            });
        let regional_alert = self
            .open_regional_goal_ids()
            .into_iter()
            .find(|goal| Some(*goal) != self.tracked_regional_goal)
            .and_then(|goal| self.campaign.history().world().regional_goals().get(&goal))
            .map(|goal| format!("REGIONAL: {}\nJ > X DETAILS", goal.title));
        let surface_context = regional_alert.or(project_alert).unwrap_or_default();
        let location = if player.map == self.campaign.site_plan().regional_map {
            self.campaign
                .site_plan()
                .regional_sites
                .iter()
                .map(|site| {
                    (
                        (site.position.x - player.grid.x).abs()
                            + (site.position.y - player.grid.y).abs(),
                        site.name.as_str(),
                    )
                })
                .min_by_key(|(distance, _)| *distance)
                .filter(|(distance, _)| *distance <= 2)
                .map_or_else(
                    || {
                        self.campaign
                            .simulation()
                            .map(player.map)
                            .and_then(|map| map.cell(player.grid))
                            .map_or_else(
                                || "Regional Wilds".to_string(),
                                |cell| regional_landscape_name(cell.terrain).to_string(),
                            )
                    },
                    |(_, name)| name.to_string(),
                )
        } else {
            self.campaign
                .site_plan()
                .dungeon
                .levels
                .iter()
                .find(|level| level.entry.z == player.grid.z)
                .map(|level| level.name.clone())
                .unwrap_or_else(|| site.name.clone())
        };
        SidebarContent {
            location,
            date: if player.grid.z < 0 {
                format!(
                    "YEAR {} MONTH {}  DEPTH {}",
                    self.campaign.history().world().date.year,
                    self.campaign.history().world().date.month,
                    player.grid.z.unsigned_abs()
                )
            } else {
                format!(
                    "YEAR {} MONTH {}  SURFACE",
                    self.campaign.history().world().date.year,
                    self.campaign.history().world().date.month
                )
            },
            status,
            threat,
            lead: objective,
            context: if let Some(level) = self
                .campaign
                .site_plan()
                .dungeon
                .levels
                .iter()
                .find(|level| level.entry.z == player.grid.z)
            {
                first_sentence(&level.historical_context)
            } else {
                surface_context
            },
            controls,
        }
    }

    fn draw_landmark_labels(
        &self,
        ui: &mut UiDrawList,
        snapshot: &PresentationSnapshot,
        cell_size: f32,
        map_width: f32,
        map_height: f32,
        scale: f32,
    ) {
        let mut placed_labels = Vec::<UiRect>::new();
        for landmark in &snapshot.landmarks {
            let landmark_distance = (landmark.position.x - snapshot.camera_center.x)
                .abs()
                .max((landmark.position.y - snapshot.camera_center.y).abs());
            if snapshot.map == self.campaign.site_plan().regional_map
                && landmark.kind != ultimate_fate_core::LandmarkKind::TownSquare
                && landmark_distance > 8
            {
                continue;
            }
            let x = map_width * 0.5
                + (landmark.position.x - snapshot.camera_center.x) as f32 * cell_size;
            let y = map_height * 0.5
                + (landmark.position.y - snapshot.camera_center.y) as f32 * cell_size;
            let label_width =
                (landmark.name.len() as f32 * 12.0 * scale).clamp(96.0 * scale, 210.0 * scale);
            let bounds = UiRect::new(
                x + cell_size * 0.42,
                y - 10.0 * scale,
                label_width,
                17.0 * scale,
            );
            if bounds.x < 4.0 * scale
                || bounds.y < 4.0 * scale
                || bounds.x + bounds.width > map_width - 4.0 * scale
                || bounds.y + bounds.height > map_height - 4.0 * scale
            {
                continue;
            }
            if placed_labels.iter().any(|placed| {
                bounds.x < placed.x + placed.width
                    && bounds.x + bounds.width > placed.x
                    && bounds.y < placed.y + placed.height
                    && bounds.y + bounds.height > placed.y
            }) {
                continue;
            }
            placed_labels.push(bounds);
            ui.bordered_panel(
                bounds,
                [0.025, 0.035, 0.045, 0.88],
                [0.72, 0.57, 0.25, 0.95],
                scale,
            );
            ui.text(
                UiRect::new(
                    bounds.x + 5.0 * scale,
                    bounds.y + 3.0 * scale,
                    bounds.width - 10.0 * scale,
                    bounds.height - 5.0 * scale,
                ),
                &landmark.name,
                UiTextStyle {
                    color: [1.0, 0.84, 0.38, 1.0],
                    pixel_scale: 1.5 * scale,
                    line_spacing: 2.0 * scale,
                },
            );
        }
    }

    fn draw_map_layer_hint(
        &self,
        ui: &mut UiDrawList,
        snapshot: &PresentationSnapshot,
        map_width: f32,
        scale: f32,
    ) {
        let player = self.campaign.simulation().player().position;
        let label = if snapshot.map == self.campaign.site_plan().map && player.grid.z == 0 {
            let gate = self.campaign.site_plan().nearest_regional_gate(player.grid);
            format!(
                "TOWN - WORLD EXIT {}",
                relative_steps(gate.x - player.grid.x, gate.y - player.grid.y)
            )
        } else if snapshot.map == self.campaign.site_plan().regional_map {
            let capital = self
                .campaign
                .site_plan()
                .regional_sites
                .iter()
                .find(|site| site.site == self.campaign.site_plan().site)
                .map(|site| site.position)
                .unwrap_or_default();
            format!(
                "REGION - {} {}",
                self.campaign.site_plan().town_name,
                relative_steps(capital.x - player.grid.x, capital.y - player.grid.y)
            )
        } else {
            return;
        };
        let label_width = (label.len() as f32 * 10.0 * scale + 16.0 * scale)
            .clamp(180.0 * scale, 300.0 * scale)
            .min(map_width - 20.0 * scale);
        let bounds = UiRect::new(10.0 * scale, 10.0 * scale, label_width, 32.0 * scale);
        ui.bordered_panel(
            bounds,
            [0.025, 0.035, 0.045, 0.90],
            [0.35, 0.72, 0.66, 0.95],
            scale,
        );
        ui.text(
            UiRect::new(
                bounds.x + 6.0 * scale,
                bounds.y + 7.0 * scale,
                bounds.width - 12.0 * scale,
                20.0 * scale,
            ),
            label,
            UiTextStyle {
                color: [0.56, 0.94, 0.84, 1.0],
                pixel_scale: 1.35 * scale,
                line_spacing: 2.0 * scale,
            },
        );
    }

    fn draw_targeting_overlay(
        &self,
        ui: &mut UiDrawList,
        snapshot: &PresentationSnapshot,
        cell_size: f32,
        map_width: f32,
        map_height: f32,
        scale: f32,
    ) {
        let Some(targeting) = self.targeting else {
            return;
        };
        let player = self.campaign.simulation().player();
        let target = self.target_at_cursor();
        let check = target.map(|target| {
            self.campaign
                .simulation()
                .check_ranged_attack(self.campaign.simulation().player_id(), target)
        });
        let cursor_color = match check {
            Some(Ok(())) => [0.35, 1.0, 0.48, 1.0],
            Some(Err(_)) => [1.0, 0.32, 0.24, 1.0],
            None => [1.0, 0.78, 0.22, 1.0],
        };
        let cursor_x =
            map_width * 0.5 + (targeting.cursor.x - snapshot.camera_center.x) as f32 * cell_size;
        let cursor_y =
            map_height * 0.5 + (targeting.cursor.y - snapshot.camera_center.y) as f32 * cell_size;
        let cursor_size = cell_size * 0.88;
        let cursor = UiRect::new(
            cursor_x - cursor_size * 0.5,
            cursor_y - cursor_size * 0.5,
            cursor_size,
            cursor_size,
        );
        let stroke = (2.0 * scale).max(1.0);
        ui.rect(
            UiRect::new(cursor.x, cursor.y, cursor.width, stroke),
            cursor_color,
        );
        ui.rect(
            UiRect::new(
                cursor.x,
                cursor.y + cursor.height - stroke,
                cursor.width,
                stroke,
            ),
            cursor_color,
        );
        ui.rect(
            UiRect::new(cursor.x, cursor.y, stroke, cursor.height),
            cursor_color,
        );
        ui.rect(
            UiRect::new(
                cursor.x + cursor.width - stroke,
                cursor.y,
                stroke,
                cursor.height,
            ),
            cursor_color,
        );

        let distance = ranged_grid_distance(player.position.grid, targeting.cursor);
        let (target_name, health, status) = match (target, check) {
            (Some(target), Some(result)) => {
                let health = self
                    .campaign
                    .simulation()
                    .combatant(target)
                    .map(|combatant| format!("{}/{}", combatant.health, combatant.max_health))
                    .unwrap_or_else(|| "--".to_string());
                let status = match result {
                    Ok(()) => "READY TO FIRE".to_string(),
                    Err(reason) => action_failure_text(reason).to_string(),
                };
                (self.entity_name(target), health, status)
            }
            _ => (
                "NO TARGET".to_string(),
                "--".to_string(),
                "SELECT A COMBATANT".to_string(),
            ),
        };
        let panel = UiRect::new(
            12.0 * scale,
            12.0 * scale,
            (map_width * 0.58)
                .clamp(330.0 * scale, 590.0 * scale)
                .min((map_width - 24.0 * scale).max(1.0)),
            112.0 * scale,
        );
        ui.bordered_panel(
            panel,
            [0.025, 0.035, 0.045, 0.96],
            cursor_color,
            2.0 * scale,
        );
        ui.text(
            UiRect::new(
                panel.x + 12.0 * scale,
                panel.y + 10.0 * scale,
                panel.width - 24.0 * scale,
                22.0 * scale,
            ),
            "RANGED TARGETING",
            UiTextStyle {
                color: cursor_color,
                pixel_scale: 2.0 * scale,
                line_spacing: 3.0 * scale,
            },
        );
        ui.text(
            UiRect::new(
                panel.x + 12.0 * scale,
                panel.y + 36.0 * scale,
                panel.width - 24.0 * scale,
                48.0 * scale,
            ),
            format!(
                "{target_name}  HEALTH {health}\nDISTANCE {distance} / RANGE {}\n{status}",
                targeting.range
            ),
            UiTextStyle {
                pixel_scale: 1.55 * scale,
                line_spacing: 2.0 * scale,
                ..Default::default()
            },
        );
        ui.text(
            UiRect::new(
                panel.x + 12.0 * scale,
                panel.y + 88.0 * scale,
                panel.width - 24.0 * scale,
                18.0 * scale,
            ),
            self.input_prompts.targeting_help(),
            UiTextStyle {
                color: [0.65, 0.82, 0.92, 1.0],
                pixel_scale: 1.35 * scale,
                line_spacing: 2.0 * scale,
            },
        );
    }

    fn draw_briefing_overlay(&self, ui: &mut UiDrawList, width: f32, height: f32, scale: f32) {
        ui.rect(
            UiRect::new(0.0, 0.0, width, height),
            [0.01, 0.015, 0.02, 0.82],
        );
        let overlay_width = (width * 0.78)
            .clamp(320.0 * scale, 900.0 * scale)
            .min((width - 24.0 * scale).max(1.0));
        let overlay_height = (height * 0.84)
            .clamp(260.0 * scale, 680.0 * scale)
            .min((height - 24.0 * scale).max(1.0));
        let panel = UiRect::new(
            (width - overlay_width) * 0.5,
            (height - overlay_height) * 0.5,
            overlay_width,
            overlay_height,
        );
        ui.bordered_panel(
            panel,
            [0.025, 0.035, 0.045, 0.99],
            [0.72, 0.57, 0.25, 1.0],
            3.0 * scale,
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 20.0 * scale,
                panel.width - 48.0 * scale,
                28.0 * scale,
            ),
            &self.campaign.start().briefing.title,
            UiTextStyle {
                color: [1.0, 0.80, 0.30, 1.0],
                pixel_scale: 3.0 * scale,
                line_spacing: 5.0 * scale,
            },
        );
        let body = self
            .campaign
            .start()
            .briefing
            .paragraphs
            .iter()
            .map(|paragraph| paragraph.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 64.0 * scale,
                panel.width - 48.0 * scale,
                panel.height - 112.0 * scale,
            ),
            body,
            UiTextStyle {
                pixel_scale: 2.0 * scale,
                line_spacing: 4.0 * scale,
                ..Default::default()
            },
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + panel.height - 36.0 * scale,
                panel.width - 48.0 * scale,
                20.0 * scale,
            ),
            self.input_prompts.begin(),
            UiTextStyle {
                color: [0.65, 0.82, 0.92, 1.0],
                pixel_scale: 2.0 * scale,
                line_spacing: 4.0 * scale,
            },
        );
    }

    fn draw_conversation_overlay(&self, ui: &mut UiDrawList, width: f32, height: f32, scale: f32) {
        let Some(active) = self.active_conversation.as_ref() else {
            return;
        };
        ui.rect(
            UiRect::new(0.0, 0.0, width, height),
            [0.01, 0.015, 0.02, 0.78],
        );
        let panel = UiRect::new(width * 0.11, height * 0.09, width * 0.78, height * 0.82);
        ui.bordered_panel(
            panel,
            [0.035, 0.042, 0.048, 0.99],
            [0.72, 0.57, 0.25, 1.0],
            3.0 * scale,
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 20.0 * scale,
                panel.width - 48.0 * scale,
                30.0 * scale,
            ),
            &active.conversation.speaker_name,
            UiTextStyle {
                color: [1.0, 0.80, 0.30, 1.0],
                pixel_scale: 3.0 * scale,
                line_spacing: 5.0 * scale,
            },
        );
        let affiliation = format!(
            "{} — {}",
            occupation_name(active.conversation.occupation),
            active.conversation.faction_name
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 54.0 * scale,
                panel.width - 48.0 * scale,
                22.0 * scale,
            ),
            affiliation,
            UiTextStyle {
                color: [0.75, 0.82, 0.86, 1.0],
                pixel_scale: 2.0 * scale,
                line_spacing: 4.0 * scale,
            },
        );

        let topics = active
            .conversation
            .topics
            .iter()
            .enumerate()
            .map(|(index, topic)| {
                let cursor = if index == active.selected {
                    "[X]"
                } else {
                    "[ ]"
                };
                let learned = if self
                    .learned_topics
                    .contains(&(active.conversation.speaker, topic.kind))
                {
                    "+"
                } else {
                    " "
                };
                format!("{cursor}{learned} {}", topic.prompt)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let topic_height = (active.conversation.topics.len() as f32 * 28.0 * scale)
            .clamp(90.0 * scale, 205.0 * scale);
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 88.0 * scale,
                panel.width - 48.0 * scale,
                topic_height,
            ),
            topics,
            UiTextStyle {
                color: [0.78, 0.88, 0.94, 1.0],
                pixel_scale: 2.0 * scale,
                line_spacing: 7.0 * scale,
            },
        );

        let response_top = panel.y + 100.0 * scale + topic_height;
        ui.bordered_panel(
            UiRect::new(
                panel.x + 20.0 * scale,
                response_top,
                panel.width - 40.0 * scale,
                panel.y + panel.height - response_top - 58.0 * scale,
            ),
            [0.055, 0.048, 0.035, 0.94],
            [0.42, 0.36, 0.23, 1.0],
            2.0 * scale,
        );
        let response = active
            .response
            .as_deref()
            .unwrap_or("Choose what to ask. A + marks a topic already recorded in your journal.");
        ui.text(
            UiRect::new(
                panel.x + 38.0 * scale,
                response_top + 18.0 * scale,
                panel.width - 76.0 * scale,
                panel.y + panel.height - response_top - 92.0 * scale,
            ),
            response,
            UiTextStyle {
                pixel_scale: 2.0 * scale,
                line_spacing: 5.0 * scale,
                ..Default::default()
            },
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + panel.height - 36.0 * scale,
                panel.width - 48.0 * scale,
                20.0 * scale,
            ),
            self.input_prompts.conversation_help(),
            UiTextStyle {
                color: [0.65, 0.82, 0.92, 1.0],
                pixel_scale: 2.0 * scale,
                line_spacing: 4.0 * scale,
            },
        );
    }

    fn draw_resolution_overlay(&self, ui: &mut UiDrawList, width: f32, height: f32, scale: f32) {
        let Some(resolution) = &self.active_resolution else {
            return;
        };
        ui.rect(
            UiRect::new(0.0, 0.0, width, height),
            [0.01, 0.015, 0.02, 0.82],
        );
        let panel = UiRect::new(width * 0.12, height * 0.10, width * 0.76, height * 0.80);
        ui.bordered_panel(
            panel,
            [0.035, 0.042, 0.048, 0.99],
            [0.80, 0.58, 0.22, 1.0],
            3.0 * scale,
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 20.0 * scale,
                panel.width - 48.0 * scale,
                30.0 * scale,
            ),
            if resolution.outcome.is_some() {
                "THE TOWN RECORDS YOUR DECISION"
            } else {
                "HOW SHOULD THE CRISIS END?"
            },
            UiTextStyle {
                color: [1.0, 0.80, 0.30, 1.0],
                pixel_scale: 2.7 * scale,
                line_spacing: 4.0 * scale,
            },
        );

        if let Some(outcome) = &resolution.outcome {
            ui.text(
                UiRect::new(
                    panel.x + 28.0 * scale,
                    panel.y + 82.0 * scale,
                    panel.width - 56.0 * scale,
                    panel.height - 160.0 * scale,
                ),
                format!(
                    "{}.\n\nFOOD RESERVE  {}\nTOWN COIN  {}\nACTIVE LAWS  {}\n\nThe intervention is now a structured historical event. Future simulation can use its resource, legal, and faction consequences.",
                    outcome.summary,
                    outcome.food_after,
                    outcome.coin_after,
                    outcome.active_laws
                ),
                UiTextStyle {
                    pixel_scale: 2.0 * scale,
                    line_spacing: 6.0 * scale,
                    ..Default::default()
                },
            );
            ui.text(
                UiRect::new(
                    panel.x + 28.0 * scale,
                    panel.y + panel.height - 42.0 * scale,
                    panel.width - 56.0 * scale,
                    22.0 * scale,
                ),
                self.input_prompts.resolution_continue(),
                UiTextStyle {
                    color: [0.52, 0.90, 0.62, 1.0],
                    pixel_scale: 1.8 * scale,
                    line_spacing: 3.0 * scale,
                },
            );
            return;
        }

        let option_lines = resolution
            .options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                let marker = if index == resolution.selected {
                    "[X]"
                } else {
                    "[ ]"
                };
                format!("{marker} {}", option.title)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        ui.text(
            UiRect::new(
                panel.x + 28.0 * scale,
                panel.y + 76.0 * scale,
                panel.width * 0.42,
                panel.height - 142.0 * scale,
            ),
            option_lines,
            UiTextStyle {
                pixel_scale: 2.0 * scale,
                line_spacing: 8.0 * scale,
                ..Default::default()
            },
        );
        let detail_panel = UiRect::new(
            panel.x + panel.width * 0.48,
            panel.y + 72.0 * scale,
            panel.width * 0.48,
            panel.height - 132.0 * scale,
        );
        ui.bordered_panel(
            detail_panel,
            [0.055, 0.048, 0.035, 0.94],
            [0.42, 0.36, 0.23, 1.0],
            2.0 * scale,
        );
        let details = resolution
            .options
            .get(resolution.selected)
            .map(|option| {
                let faction =
                    &self.campaign.history().world().factions()[&option.supported_faction].name;
                format!(
                    "{}\n\n{}\n\nMOST DIRECTLY SUPPORTED\n{}",
                    option.title, option.description, faction
                )
            })
            .unwrap_or_default();
        ui.text(
            detail_panel.inset(18.0 * scale),
            details,
            UiTextStyle {
                pixel_scale: 1.8 * scale,
                line_spacing: 5.0 * scale,
                ..Default::default()
            },
        );
        ui.text(
            UiRect::new(
                panel.x + 28.0 * scale,
                panel.y + panel.height - 40.0 * scale,
                panel.width - 56.0 * scale,
                22.0 * scale,
            ),
            self.input_prompts.resolution_help(),
            UiTextStyle {
                color: [0.65, 0.82, 0.92, 1.0],
                pixel_scale: 1.7 * scale,
                line_spacing: 3.0 * scale,
            },
        );
    }

    fn draw_inventory_overlay(&self, ui: &mut UiDrawList, width: f32, height: f32, scale: f32) {
        ui.rect(
            UiRect::new(0.0, 0.0, width, height),
            [0.01, 0.015, 0.02, 0.78],
        );
        let panel = UiRect::new(width * 0.14, height * 0.10, width * 0.72, height * 0.80);
        ui.bordered_panel(
            panel,
            [0.035, 0.042, 0.048, 0.99],
            [0.72, 0.57, 0.25, 1.0],
            3.0 * scale,
        );
        let health = self
            .campaign
            .simulation()
            .player_combatant()
            .map(|combatant| format!("HEALTH {}/{}", combatant.health, combatant.max_health))
            .unwrap_or_else(|| "HEALTH --".to_string());
        let needs = self.campaign.simulation().player_needs();
        let health = format!(
            "{health}   HUNGER {}   THIRST {}",
            needs.hunger, needs.thirst
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 20.0 * scale,
                panel.width - 48.0 * scale,
                28.0 * scale,
            ),
            "INVENTORY",
            UiTextStyle {
                color: [1.0, 0.80, 0.30, 1.0],
                pixel_scale: 3.0 * scale,
                line_spacing: 5.0 * scale,
            },
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 56.0 * scale,
                panel.width - 48.0 * scale,
                22.0 * scale,
            ),
            health,
            UiTextStyle {
                color: [0.75, 0.84, 0.88, 1.0],
                pixel_scale: 2.0 * scale,
                line_spacing: 4.0 * scale,
            },
        );

        let item_ids = self.inventory_item_ids();
        let inventory = self.campaign.simulation().player_inventory();
        let item_lines = item_ids
            .iter()
            .enumerate()
            .filter_map(|(index, item_id)| {
                let item = self.campaign.simulation().item(*item_id)?;
                let selected = if index == self.inventory_selected {
                    "[X]"
                } else {
                    "[ ]"
                };
                let equipped = match item.kind {
                    ItemKind::MeleeWeapon { .. }
                        if inventory.is_some_and(|inventory| {
                            inventory.equipped_melee == Some(*item_id)
                        }) =>
                    {
                        "  MELEE"
                    }
                    ItemKind::RangedWeapon { .. }
                        if inventory.is_some_and(|inventory| {
                            inventory.equipped_ranged == Some(*item_id)
                        }) =>
                    {
                        "  RANGED"
                    }
                    _ => "",
                };
                let quantity = if item.quantity > 1
                    || matches!(
                        item.kind,
                        ItemKind::Ammunition { .. } | ItemKind::Consumable { .. }
                    ) {
                    format!(" x{}", item.quantity)
                } else {
                    String::new()
                };
                Some(format!("{selected} {}{quantity}{equipped}", item.name))
            })
            .collect::<Vec<_>>()
            .join("\n");
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 94.0 * scale,
                panel.width * 0.45,
                panel.height - 150.0 * scale,
            ),
            item_lines,
            UiTextStyle {
                pixel_scale: 2.0 * scale,
                line_spacing: 8.0 * scale,
                ..Default::default()
            },
        );

        let detail_panel = UiRect::new(
            panel.x + panel.width * 0.50,
            panel.y + 88.0 * scale,
            panel.width * 0.46,
            panel.height - 146.0 * scale,
        );
        ui.bordered_panel(
            detail_panel,
            [0.055, 0.048, 0.035, 0.94],
            [0.42, 0.36, 0.23, 1.0],
            2.0 * scale,
        );
        let details = item_ids
            .get(self.inventory_selected)
            .and_then(|item| self.campaign.simulation().item(*item))
            .map(|item| {
                let use_text = match item.kind {
                    ItemKind::MeleeWeapon { damage } => {
                        format!("MELEE WEAPON\nDAMAGE  {damage}")
                    }
                    ItemKind::RangedWeapon { damage, range, .. } => {
                        format!("RANGED WEAPON\nDAMAGE  {damage}\nRANGE  {range}")
                    }
                    ItemKind::Ammunition { .. } => {
                        "AMMUNITION\nUSED AUTOMATICALLY WITH A BOW".to_string()
                    }
                    ItemKind::Consumable { healing } => {
                        format!("FIELD SUPPLY\nRESTORES  {healing} HEALTH")
                    }
                    ItemKind::Food { nourishment } => {
                        format!("FOOD\nRELIEVES  {nourishment} HUNGER")
                    }
                    ItemKind::Drink { hydration } => {
                        format!("DRINK\nRELIEVES  {hydration} THIRST")
                    }
                    ItemKind::Book { subject } => {
                        format!("READABLE RECORD\nSUBJECT  {subject:?}")
                    }
                    ItemKind::Key { lock_code } => {
                        format!("KEY\nLOCK MARK  {lock_code:016X}")
                    }
                    ItemKind::Tool => "ORDINARY TOOL".to_string(),
                    ItemKind::Reagent { material } => {
                        format!("MAGICAL REAGENT\nSOURCE  {:?}", material.source())
                    }
                    ItemKind::InscribedArtifact { formula, .. } => {
                        let formula = self.campaign.simulation().rules().formula(formula);
                        formula.map_or_else(
                            || "INSCRIBED ARTIFACT\nTHE FORMULA IS ILLEGIBLE".to_string(),
                            |formula| {
                                let action = if self.campaign.simulation()
                                    .known_formulas()
                                    .contains(&formula.id)
                                {
                                    "PRIMARY  PERFORM FORMULA"
                                } else {
                                    "PRIMARY  STUDY INSCRIPTION"
                                };
                                let reagents = formula
                                    .reagents
                                    .iter()
                                    .map(|material| material.name())
                                    .collect::<Vec<_>>()
                                    .join(" + ");
                                format!(
                                    "INSCRIBED ARTIFACT\n{action}\n\n{}\nEFFECT  {}\nREAGENTS  {}\nCONDITION  {:?}",
                                    formula.name,
                                    formula.effect.name(),
                                    reagents,
                                    formula.condition
                                )
                            },
                        )
                    }
                    ItemKind::Artifact => {
                        "QUEST ARTIFACT\nA HISTORICAL OBJECT WITH NO DIRECT COMBAT USE".to_string()
                    }
                };
                let provenance = if item.id == self.campaign.site_plan().starter_sword.id {
                    self.campaign.site_plan().starter_sword_provenance.as_str()
                } else {
                    "Ordinary travel gear without recorded provenance."
                };
                format!(
                    "{}\n\n{}\n\nSELECTED AMOUNT  {}\nWEIGHT  {} G\nQUALITY  {}\n\n{}",
                    item.name,
                    use_text,
                    self.inventory_quantity.min(item.quantity),
                    item.weight_grams,
                    item.quality,
                    provenance
                )
            })
            .unwrap_or_else(|| "Your inventory is empty.".to_string());
        ui.text(
            detail_panel.inset(16.0 * scale),
            details,
            UiTextStyle {
                pixel_scale: 2.0 * scale,
                line_spacing: 5.0 * scale,
                ..Default::default()
            },
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + panel.height - 36.0 * scale,
                panel.width - 48.0 * scale,
                20.0 * scale,
            ),
            self.input_prompts.inventory_help(),
            UiTextStyle {
                color: [0.65, 0.82, 0.92, 1.0],
                pixel_scale: 2.0 * scale,
                line_spacing: 4.0 * scale,
            },
        );
    }

    fn draw_container_overlay(&self, ui: &mut UiDrawList, width: f32, height: f32, scale: f32) {
        let Some(active) = self.active_container.as_ref() else {
            return;
        };
        let Some(container) = self.campaign.simulation().container(active.entity) else {
            return;
        };
        let items = self.active_container_item_ids(active.side);
        let used = self
            .campaign
            .simulation()
            .container_weight(active.entity)
            .unwrap_or_default();
        ui.rect(
            UiRect::new(0.0, 0.0, width, height),
            [0.01, 0.015, 0.02, 0.78],
        );
        let panel = UiRect::new(width * 0.14, height * 0.10, width * 0.72, height * 0.80);
        ui.bordered_panel(
            panel,
            [0.035, 0.042, 0.048, 0.99],
            [0.72, 0.57, 0.25, 1.0],
            3.0 * scale,
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 20.0 * scale,
                panel.width - 48.0 * scale,
                30.0 * scale,
            ),
            &container.name,
            UiTextStyle {
                color: [1.0, 0.80, 0.30, 1.0],
                pixel_scale: 3.0 * scale,
                line_spacing: 5.0 * scale,
            },
        );
        let side = match active.side {
            ContainerSide::Contents => "CONTAINER CONTENTS",
            ContainerSide::Pack => "YOUR PACK",
        };
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 58.0 * scale,
                panel.width - 48.0 * scale,
                22.0 * scale,
            ),
            format!(
                "{side}   MOVING {}   CONTAINER WEIGHT {used}/{} G",
                active.quantity, container.capacity_grams
            ),
            UiTextStyle {
                color: [0.68, 0.86, 0.76, 1.0],
                pixel_scale: 1.9 * scale,
                line_spacing: 4.0 * scale,
            },
        );
        let lines = if items.is_empty() {
            "EMPTY".to_string()
        } else {
            items
                .iter()
                .enumerate()
                .filter_map(|(index, item_id)| {
                    let item = self.campaign.simulation().item(*item_id)?;
                    let cursor = if index == active.selected {
                        "[X]"
                    } else {
                        "[ ]"
                    };
                    let title = self.campaign.simulation().legal_owner(*item_id);
                    let legality = if title == Some(self.campaign.simulation().player_id()) {
                        "YOURS"
                    } else if self.campaign.simulation().is_stolen(*item_id) {
                        "STOLEN"
                    } else {
                        "OWNED"
                    };
                    Some(format!(
                        "{cursor} {} x{}   {legality}",
                        item.name, item.quantity
                    ))
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 96.0 * scale,
                panel.width - 48.0 * scale,
                panel.height - 154.0 * scale,
            ),
            lines,
            UiTextStyle {
                pixel_scale: 2.0 * scale,
                line_spacing: 8.0 * scale,
                ..Default::default()
            },
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + panel.height - 36.0 * scale,
                panel.width - 48.0 * scale,
                20.0 * scale,
            ),
            self.input_prompts.container_help(),
            UiTextStyle {
                color: [0.65, 0.82, 0.92, 1.0],
                pixel_scale: 1.8 * scale,
                line_spacing: 4.0 * scale,
            },
        );
    }

    fn draw_trade_overlay(&self, ui: &mut UiDrawList, width: f32, height: f32, scale: f32) {
        let Some(active) = self.active_trade.as_ref() else {
            return;
        };
        let shop = &self.campaign.site_plan().shop;
        let item_ids = self.trade_item_ids(active.direction);
        ui.rect(
            UiRect::new(0.0, 0.0, width, height),
            [0.01, 0.015, 0.02, 0.78],
        );
        let panel = UiRect::new(width * 0.12, height * 0.09, width * 0.76, height * 0.82);
        ui.bordered_panel(
            panel,
            [0.035, 0.042, 0.048, 0.99],
            [0.72, 0.57, 0.25, 1.0],
            3.0 * scale,
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 18.0 * scale,
                panel.width - 48.0 * scale,
                30.0 * scale,
            ),
            &shop.name,
            UiTextStyle {
                color: [1.0, 0.80, 0.30, 1.0],
                pixel_scale: 3.0 * scale,
                line_spacing: 5.0 * scale,
            },
        );
        let direction = match active.direction {
            TradeDirection::Buy => "BUYING FROM MERCHANT",
            TradeDirection::Sell => "SELLING TO MERCHANT",
        };
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 55.0 * scale,
                panel.width - 48.0 * scale,
                24.0 * scale,
            ),
            format!(
                "{}   YOUR COIN {}   MERCHANT COIN {}",
                direction,
                self.campaign.progress().player_coin,
                self.campaign.merchant_coin(shop.merchant)
            ),
            UiTextStyle {
                color: [0.68, 0.86, 0.76, 1.0],
                pixel_scale: 1.9 * scale,
                line_spacing: 4.0 * scale,
            },
        );
        let lines = if item_ids.is_empty() {
            match active.direction {
                TradeDirection::Buy => "NO LAWFUL STOCK REMAINS".to_string(),
                TradeDirection::Sell => "NO LAWFUL ORDINARY GOODS TO SELL".to_string(),
            }
        } else {
            item_ids
                .iter()
                .enumerate()
                .filter_map(|(index, item_id)| {
                    let item = self.campaign.simulation().item(*item_id)?;
                    let quote = self
                        .campaign
                        .trade_quote_quantity(shop.merchant, *item_id, active.direction, 1)
                        .ok()?;
                    let cursor = if index == active.selected {
                        "[X]"
                    } else {
                        "[ ]"
                    };
                    Some(format!(
                        "{cursor} {} x{}   {} COIN EACH",
                        item.name, item.quantity, quote.price
                    ))
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 96.0 * scale,
                panel.width * 0.52,
                panel.height - 154.0 * scale,
            ),
            lines,
            UiTextStyle {
                pixel_scale: 2.0 * scale,
                line_spacing: 8.0 * scale,
                ..Default::default()
            },
        );
        let detail_panel = UiRect::new(
            panel.x + panel.width * 0.57,
            panel.y + 90.0 * scale,
            panel.width * 0.39,
            panel.height - 148.0 * scale,
        );
        ui.bordered_panel(
            detail_panel,
            [0.055, 0.048, 0.035, 0.94],
            [0.42, 0.36, 0.23, 1.0],
            2.0 * scale,
        );
        let details = item_ids
            .get(active.selected)
            .and_then(|item_id| {
                let item = self.campaign.simulation().item(*item_id)?;
                let quote = self
                    .campaign
                    .trade_quote_quantity(
                        shop.merchant,
                        *item_id,
                        active.direction,
                        active.quantity.min(item.quantity),
                    )
                    .ok()?;
                let resource = quote
                    .resource
                    .map(|resource| format!("{resource:?}").to_ascii_uppercase())
                    .unwrap_or_else(|| "ORDINARY".to_string());
                Some(format!(
                    "{}\n\nSELECTED  {}\nPRICE  {} COIN\nIN STACK  {}\nQUALITY  {}\nWEIGHT  {} G\n\nMARKET  {}\nSUPPLY  {}\n\nPrices follow the settlement's actual stores and needs.",
                    item.name,
                    quote.quantity,
                    quote.price,
                    item.quantity,
                    item.quality,
                    item.weight_grams,
                    resource,
                    quote.scarcity.to_ascii_uppercase()
                ))
            })
            .unwrap_or_else(|| {
                "Change between BUY and SELL to inspect available goods.".to_string()
            });
        ui.text(
            detail_panel.inset(16.0 * scale),
            details,
            UiTextStyle {
                pixel_scale: 1.9 * scale,
                line_spacing: 5.0 * scale,
                ..Default::default()
            },
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + panel.height - 36.0 * scale,
                panel.width - 48.0 * scale,
                20.0 * scale,
            ),
            self.input_prompts.trade_help(),
            UiTextStyle {
                color: [0.65, 0.82, 0.92, 1.0],
                pixel_scale: 1.8 * scale,
                line_spacing: 4.0 * scale,
            },
        );
    }

    fn draw_journal_overlay(&self, ui: &mut UiDrawList, width: f32, height: f32, scale: f32) {
        ui.rect(
            UiRect::new(0.0, 0.0, width, height),
            [0.01, 0.015, 0.02, 0.78],
        );
        let panel = UiRect::new(width * 0.12, height * 0.10, width * 0.76, height * 0.80);
        ui.bordered_panel(
            panel,
            [0.055, 0.048, 0.035, 0.99],
            [0.72, 0.57, 0.25, 1.0],
            3.0 * scale,
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 20.0 * scale,
                panel.width - 48.0 * scale,
                28.0 * scale,
            ),
            format!(
                "JOURNAL — PAGE {} / {}",
                self.journal_page + 1,
                self.campaign
                    .start()
                    .journal
                    .entries
                    .len()
                    .max(1)
                    .div_ceil(4)
            ),
            UiTextStyle {
                color: [1.0, 0.80, 0.30, 1.0],
                pixel_scale: 3.0 * scale,
                line_spacing: 5.0 * scale,
            },
        );
        let entries = self
            .campaign
            .start()
            .journal
            .entries
            .iter()
            .rev()
            .skip(self.journal_page * 4)
            .take(4)
            .map(|entry| format!("{}\n{}", entry.title, entry.body))
            .collect::<Vec<_>>()
            .join("\n\n");
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 66.0 * scale,
                panel.width - 48.0 * scale,
                panel.height - 112.0 * scale,
            ),
            entries,
            UiTextStyle {
                pixel_scale: 2.0 * scale,
                line_spacing: 4.0 * scale,
                ..Default::default()
            },
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + panel.height - 36.0 * scale,
                panel.width - 48.0 * scale,
                20.0 * scale,
            ),
            self.input_prompts.close_journal(),
            UiTextStyle {
                color: [0.65, 0.82, 0.92, 1.0],
                pixel_scale: 2.0 * scale,
                line_spacing: 4.0 * scale,
            },
        );
    }

    fn draw_region_overlay(&self, ui: &mut UiDrawList, width: f32, height: f32, scale: f32) {
        ui.rect(
            UiRect::new(0.0, 0.0, width, height),
            [0.01, 0.015, 0.02, 0.82],
        );
        let panel = UiRect::new(width * 0.08, height * 0.07, width * 0.84, height * 0.86);
        ui.bordered_panel(
            panel,
            [0.035, 0.052, 0.052, 0.99],
            [0.25, 0.72, 0.63, 1.0],
            3.0 * scale,
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 18.0 * scale,
                panel.width - 48.0 * scale,
                28.0 * scale,
            ),
            "REGIONAL SITUATIONS",
            UiTextStyle {
                color: [0.42, 0.95, 0.82, 1.0],
                pixel_scale: 3.0 * scale,
                line_spacing: 5.0 * scale,
            },
        );
        let shortages = self
            .campaign
            .history()
            .world()
            .regional_settlements()
            .values()
            .filter(|settlement| settlement.shortage)
            .count();
        let blocked = self
            .campaign
            .history()
            .world()
            .routes()
            .values()
            .filter(|route| route.disrupted)
            .count();
        let active_parties = self
            .campaign
            .history()
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
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + 52.0 * scale,
                panel.width - 48.0 * scale,
                20.0 * scale,
            ),
            format!(
                "{} SITES   {} ROADS   {} SHORTAGES   {} BLOCKED   {} PARTIES",
                self.campaign.history().world().regional_settlements().len(),
                self.campaign.history().world().routes().len(),
                shortages,
                blocked,
                active_parties
            ),
            UiTextStyle {
                color: [0.66, 0.80, 0.76, 1.0],
                pixel_scale: 2.0 * scale,
                line_spacing: 4.0 * scale,
            },
        );

        let content_y = panel.y + 84.0 * scale;
        let content_height = panel.height - 140.0 * scale;
        let list_panel = UiRect::new(
            panel.x + 20.0 * scale,
            content_y,
            panel.width * 0.34,
            content_height,
        );
        let detail_panel = UiRect::new(
            list_panel.x + list_panel.width + 14.0 * scale,
            content_y,
            panel.width - list_panel.width - 54.0 * scale,
            content_height,
        );
        ui.bordered_panel(
            list_panel,
            [0.025, 0.032, 0.034, 0.96],
            [0.18, 0.38, 0.36, 1.0],
            2.0 * scale,
        );
        ui.bordered_panel(
            detail_panel,
            [0.025, 0.032, 0.034, 0.96],
            [0.18, 0.38, 0.36, 1.0],
            2.0 * scale,
        );

        let goal_ids = self.open_regional_goal_ids();
        let goal_list = if goal_ids.is_empty() {
            "NO URGENT CONTRACTS\n\nThe regional simulation continues. New situations appear when shortages or blocked roads require intervention.".to_string()
        } else {
            goal_ids
                .iter()
                .enumerate()
                .filter_map(|(index, id)| {
                    self.campaign
                        .history()
                        .world()
                        .regional_goals()
                        .get(id)
                        .map(|goal| {
                            format!(
                                "{} {}",
                                if index == self.regional_goal_selected {
                                    ">"
                                } else {
                                    " "
                                },
                                goal.title
                            )
                        })
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        ui.text(
            list_panel.inset(14.0 * scale),
            goal_list,
            UiTextStyle {
                color: [0.84, 0.88, 0.84, 1.0],
                pixel_scale: 2.0 * scale,
                line_spacing: 5.0 * scale,
            },
        );

        let details = goal_ids
            .get(self.regional_goal_selected)
            .and_then(|id| self.campaign.history().world().regional_goals().get(id))
            .map(|goal| {
                let sponsor = &self.campaign.history().world().factions()[&goal.sponsor].name;
                let cause = &self.campaign.history().world().events()[&goal.cause].summary;
                let state = match goal.kind {
                    RegionalGoalKind::SecureRoute(route) => {
                        let route = &self.campaign.history().world().routes()[&route];
                        format!(
                            "ROUTE: {}\n{} TO {}\nCONDITION {}   DANGER {}   BLOCKED {}",
                            route.name,
                            self.campaign.history().world().sites()[&route.first].name,
                            self.campaign.history().world().sites()[&route.second].name,
                            route.condition,
                            route.danger,
                            if route.disrupted { "YES" } else { "NO" }
                        )
                    }
                    RegionalGoalKind::RelieveShortage(site) => {
                        let settlement =
                            &self.campaign.history().world().regional_settlements()[&site];
                        let food =
                            self.campaign.history().world().sites()[&site].resources[&ultimate_fate_history::ResourceKind::Food];
                        let need =
                            settlement.monthly_consumption[&ultimate_fate_history::ResourceKind::Food];
                        format!(
                            "SETTLEMENT: {}\nPOPULATION {}   FOOD {} / {} MONTHLY\nUNREST {}",
                            self.campaign.history().world().sites()[&site].name,
                            settlement.population,
                            food,
                            need,
                            settlement.unrest
                        )
                    }
                };
                let options = self.campaign.history()
                    .regional_goal_options(goal.id)
                    .unwrap_or_default();
                let option_list = options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| {
                        format!(
                            "{} {}",
                            if index == self.regional_option_selected {
                                ">"
                            } else {
                                " "
                            },
                            option.title
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let selected = options
                    .get(self.regional_option_selected)
                    .map(|option| option.description.as_str())
                    .unwrap_or("No response is currently available.");
                format!(
                    "{}\n\nSPONSOR: {sponsor}\n{state}\n\nWHY NOW\n{cause}\n\nRESPONSES\n{option_list}\n\n{selected}",
                    goal.description
                )
            })
            .unwrap_or_else(|| {
                "Only urgent, actionable situations appear here. Routine production, trade, and history remain in the journal until they demand a decision.".to_string()
            });
        ui.text(
            detail_panel.inset(16.0 * scale),
            details,
            UiTextStyle {
                pixel_scale: 2.0 * scale,
                line_spacing: 5.0 * scale,
                ..Default::default()
            },
        );
        ui.text(
            UiRect::new(
                panel.x + 24.0 * scale,
                panel.y + panel.height - 36.0 * scale,
                panel.width - 48.0 * scale,
                20.0 * scale,
            ),
            self.input_prompts.region_help(),
            UiTextStyle {
                color: [0.65, 0.88, 0.84, 1.0],
                pixel_scale: 2.0 * scale,
                line_spacing: 4.0 * scale,
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_sidebar_section(
    ui: &mut UiDrawList,
    bounds: UiRect,
    title: &str,
    body: &str,
    color: [f32; 4],
    scale: f32,
    compact: bool,
) {
    if bounds.height <= 0.0 {
        return;
    }
    let title_height = (if compact { 15.0 } else { 18.0 } * scale).min(bounds.height * 0.34);
    ui.rect(
        UiRect::new(
            bounds.x,
            bounds.y + 1.0 * scale,
            2.0 * scale,
            (bounds.height - 4.0 * scale).max(0.0),
        ),
        [color[0], color[1], color[2], 0.72],
    );
    ui.text(
        UiRect::new(
            bounds.x + 7.0 * scale,
            bounds.y,
            bounds.width - 7.0 * scale,
            title_height,
        ),
        title,
        UiTextStyle {
            color,
            pixel_scale: if compact { 1.35 * scale } else { 1.55 * scale },
            line_spacing: 2.0 * scale,
        },
    );
    ui.text(
        UiRect::new(
            bounds.x + 7.0 * scale,
            bounds.y + title_height + 2.0 * scale,
            bounds.width - 7.0 * scale,
            (bounds.height - title_height - 4.0 * scale).max(0.0),
        ),
        body,
        UiTextStyle {
            pixel_scale: if compact { 1.5 * scale } else { 1.7 * scale },
            line_spacing: if compact { 2.0 * scale } else { 3.0 * scale },
            ..Default::default()
        },
    );
}

fn ranged_grid_distance(first: GridPos, second: GridPos) -> i32 {
    (first.x - second.x).abs().max((first.y - second.y).abs())
}

fn first_sentence(text: &str) -> String {
    let mut sentence = text.split(". ").next().unwrap_or(text).trim().to_string();
    if !sentence.is_empty() && !sentence.ends_with('.') {
        sentence.push('.');
    }
    sentence
}

fn crisis_resolution_label(kind: CrisisResolutionKind) -> &'static str {
    match kind {
        CrisisResolutionKind::EnforceEmergencyLaw => "upholding the emergency law",
        CrisisResolutionKind::OpenPublicStores => "opening the public stores",
        CrisisResolutionKind::BrokerCompromise => "the supervised compromise",
    }
}

fn keyboard_input(key: PhysicalKey) -> Option<DigitalInput> {
    match key {
        PhysicalKey::Code(KeyCode::ArrowUp | KeyCode::KeyW) => {
            Some(DigitalInput::Move(Direction::North))
        }
        PhysicalKey::Code(KeyCode::ArrowRight | KeyCode::KeyD) => {
            Some(DigitalInput::Move(Direction::East))
        }
        PhysicalKey::Code(KeyCode::ArrowDown | KeyCode::KeyS) => {
            Some(DigitalInput::Move(Direction::South))
        }
        PhysicalKey::Code(KeyCode::ArrowLeft | KeyCode::KeyA) => {
            Some(DigitalInput::Move(Direction::West))
        }
        PhysicalKey::Code(KeyCode::Enter | KeyCode::KeyE | KeyCode::Space) => {
            Some(DigitalInput::Button(GameplayButton::Primary))
        }
        PhysicalKey::Code(KeyCode::Escape | KeyCode::KeyQ) => {
            Some(DigitalInput::Button(GameplayButton::Back))
        }
        PhysicalKey::Code(KeyCode::KeyX | KeyCode::KeyL) => {
            Some(DigitalInput::Button(GameplayButton::Inspect))
        }
        PhysicalKey::Code(KeyCode::KeyJ) => Some(DigitalInput::Button(GameplayButton::Journal)),
        PhysicalKey::Code(KeyCode::KeyP) => Some(DigitalInput::Menu),
        _ => None,
    }
}

fn relative_steps(dx: i32, dy: i32) -> String {
    if dx == 0 && dy == 0 {
        return "HERE".to_string();
    }
    let mut parts = Vec::with_capacity(2);
    if dx != 0 {
        parts.push(format!("{}{}", dx.abs(), if dx > 0 { "E" } else { "W" }));
    }
    if dy != 0 {
        parts.push(format!("{}{}", dy.abs(), if dy > 0 { "S" } else { "N" }));
    }
    parts.join(" ")
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::North => "north",
        Direction::East => "east",
        Direction::South => "south",
        Direction::West => "west",
    }
}

fn terrain_name(terrain: TerrainKind) -> &'static str {
    match terrain {
        TerrainKind::Grass => "grass",
        TerrainKind::Forest => "forest",
        TerrainKind::Hills => "hills",
        TerrainKind::Mountain => "mountains",
        TerrainKind::Sand => "sand",
        TerrainKind::Snow => "snow",
        TerrainKind::Swamp => "swamp",
        TerrainKind::Dirt => "bare earth",
        TerrainKind::Road => "road",
        TerrainKind::Ocean => "sea",
        TerrainKind::Water => "water",
        TerrainKind::Bridge => "bridge",
        TerrainKind::StoneFloor => "stone floor",
        TerrainKind::Wall => "wall",
        TerrainKind::Farmland => "cultivated field",
        TerrainKind::Rubble => "collapsed rubble",
        TerrainKind::StairsUp => "stairs leading upward",
        TerrainKind::StairsDown => "stairs leading downward",
    }
}

fn regional_landscape_name(terrain: TerrainKind) -> &'static str {
    match terrain {
        TerrainKind::Grass => "Grasslands",
        TerrainKind::Forest => "Deep Forest",
        TerrainKind::Hills => "Highlands",
        TerrainKind::Mountain => "Mountain Range",
        TerrainKind::Sand => "Coast or Drylands",
        TerrainKind::Snow => "Tundra",
        TerrainKind::Swamp => "Wetlands",
        TerrainKind::Road => "Regional Road",
        TerrainKind::Ocean => "Open Sea",
        TerrainKind::Water => "River or Lake",
        TerrainKind::Bridge => "River Crossing",
        TerrainKind::StoneFloor | TerrainKind::StairsUp => "Settlement",
        TerrainKind::Rubble => "Ruined Road",
        TerrainKind::Dirt | TerrainKind::Farmland => "Cultivated Lands",
        TerrainKind::Wall | TerrainKind::StairsDown => "Regional Wilds",
    }
}

fn project_phase_name(phase: SettlementProjectPhase) -> &'static str {
    match phase {
        SettlementProjectPhase::Planned => "PLANNED",
        SettlementProjectPhase::Stalled => "STALLED",
        SettlementProjectPhase::Foundation => "FOUNDATIONS",
        SettlementProjectPhase::Structure => "UNDER CONSTRUCTION",
        SettlementProjectPhase::Completed => "OPERATING",
        SettlementProjectPhase::Damaged => "DAMAGED",
    }
}

#[cfg(test)]
fn calendar_month_due(turn: u64, next_month_turn: u64) -> bool {
    turn >= next_month_turn
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

fn compact_notification(text: &str, max_words: usize) -> String {
    let words = text.split_whitespace().collect::<Vec<_>>();
    let mut compact = words
        .iter()
        .take(max_words)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    compact = compact.trim_end_matches(['.', '!', '?']).to_string();
    if words.len() > max_words {
        compact.push('…');
    }
    compact
}

fn action_failure_text(reason: ActionFailure) -> String {
    match reason {
        ActionFailure::InvalidTarget => "There is no valid target.".to_string(),
        ActionFailure::OutOfRange => "The target is out of range.".to_string(),
        ActionFailure::LineBlocked => "Something blocks the shot.".to_string(),
        ActionFailure::NoWeapon => "No suitable weapon is equipped.".to_string(),
        ActionFailure::NoAmmunition => "You have no matching ammunition.".to_string(),
        ActionFailure::ItemNotCarried => "You are not carrying that item.".to_string(),
        ActionFailure::ItemCannotBeUsed => "That item cannot be used this way.".to_string(),
        ActionFailure::AlreadyAtFullHealth => "You are already at full health.".to_string(),
        ActionFailure::UnknownFormula => "You have not reconstructed that formula.".to_string(),
        ActionFailure::MissingReagent(material) => {
            format!("The formula requires {}.", material.name())
        }
        ActionFailure::MagicalConditionUnmet(condition) => {
            format!("The formula's {:?} condition is not satisfied.", condition)
        }
        ActionFailure::NoTransition => "There are no stairs or passage here.".to_string(),
        ActionFailure::QuestNotReady => "That quest cannot be completed here yet.".to_string(),
        ActionFailure::ExperimentFailed => {
            "The reagents react, but no stable formula emerges under these conditions.".to_string()
        }
        ActionFailure::ContainerLocked => "The container is locked.".to_string(),
        ActionFailure::WrongKey => "That key does not fit the lock.".to_string(),
        ActionFailure::ContainerFull => "The container cannot hold any more.".to_string(),
        ActionFailure::NotAContainer => "That is not a usable container.".to_string(),
        ActionFailure::AlreadySatisfied => "You do not need that right now.".to_string(),
        ActionFailure::InvalidQuantity => "That stack does not contain that many.".to_string(),
    }
}

fn occupation_name(occupation: Occupation) -> &'static str {
    match occupation {
        Occupation::Farmer => "farmer",
        Occupation::Miller => "miller",
        Occupation::Merchant => "merchant",
        Occupation::Guard => "guard",
        Occupation::Priest => "priest",
        Occupation::Healer => "healer",
        Occupation::Smith => "smith",
        Occupation::Laborer => "laborer",
        Occupation::Innkeeper => "innkeeper",
        Occupation::Official => "official",
    }
}

fn aid_method_name(method: AidResolutionKind) -> &'static str {
    match method {
        AidResolutionKind::ReleasedByConsent => "a supported appeal",
        AidResolutionKind::Purchased => "purchase",
        AidResolutionKind::TakenWithoutConsent => "theft",
        AidResolutionKind::AlternativeTreatment => "alternative treatment",
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut App::default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_maps_to_device_neutral_controls() {
        assert_eq!(
            keyboard_input(PhysicalKey::Code(KeyCode::KeyW)),
            Some(DigitalInput::Move(Direction::North))
        );
        assert_eq!(
            keyboard_input(PhysicalKey::Code(KeyCode::Enter)),
            Some(DigitalInput::Button(GameplayButton::Primary))
        );
        assert_eq!(
            keyboard_input(PhysicalKey::Code(KeyCode::KeyP)),
            Some(DigitalInput::Menu)
        );
    }

    #[test]
    fn desktop_prompts_show_keyboard_bindings_not_abstract_button_names() {
        let prompts = InputPrompts::desktop_keyboard();
        let legend = prompts.legend();

        assert_eq!(prompts.begin(), "[E / ENTER] BEGIN");
        assert_eq!(
            prompts.close_journal(),
            "W / S / UP / DOWN PAGE   [X / L] REGIONAL SITUATIONS   [J / ESC] CLOSE"
        );
        assert_eq!(
            prompts.region_help(),
            "W / S / UP / DOWN SITUATION   A / D RESPONSE   E / ENTER TRACK / RESOLVE   X / L JOURNAL   ESC CLOSE"
        );
        assert_eq!(
            prompts.conversation_help(),
            "W / S / UP / DOWN CHOOSE   E / ENTER ASK   ESC LEAVE"
        );
        assert_eq!(
            prompts.inventory_help(),
            "W / S / UP / DOWN CHOOSE   A / D AMOUNT   E / ENTER EQUIP / USE   X / L DROP   ESC / P CLOSE"
        );
        assert_eq!(
            prompts.container_help(),
            "W / S / UP / DOWN CHOOSE   A / D AMOUNT   X / L CONTENTS / PACK   E / ENTER MOVE   ESC CLOSE"
        );
        assert_eq!(
            prompts.targeting_help(),
            "WASD / ARROWS AIM   E / ENTER FIRE   ESC CANCEL"
        );
        assert_eq!(
            prompts.resolution_help(),
            "W / S / UP / DOWN CHOOSE   E / ENTER COMMIT   ESC LEAVE"
        );
        assert!(legend.contains("ACT / ATTACK / STAIRS  E / ENTER"));
        assert!(!legend.contains("PRIMARY"));
        assert!(!legend.contains("CONTROLLER"));
    }

    #[test]
    fn ranged_cursor_uses_tile_distance_in_all_directions() {
        let origin = GridPos::new(0, 0, 0);
        assert_eq!(ranged_grid_distance(origin, GridPos::new(5, 0, 0)), 5);
        assert_eq!(ranged_grid_distance(origin, GridPos::new(5, 5, 0)), 5);
    }

    #[test]
    fn routine_world_changes_do_not_interrupt_exploration() {
        assert!(!direct_world_alert(
            HistoricalEventKind::StrategicBalanceShifted
        ));
        assert!(!direct_world_alert(HistoricalEventKind::RegionalTrade));
        assert!(!direct_world_alert(
            HistoricalEventKind::RegionalPartyArrived
        ));
        assert!(direct_world_alert(HistoricalEventKind::RouteDisrupted));
        assert!(direct_world_alert(
            HistoricalEventKind::RegionalGoalProposed
        ));
    }

    #[test]
    fn calendar_and_notifications_are_player_scaled() {
        assert!(!calendar_month_due(
            LIVING_MONTH_TICKS - 1,
            LIVING_MONTH_TICKS
        ));
        assert!(calendar_month_due(LIVING_MONTH_TICKS, LIVING_MONTH_TICKS));
        assert_eq!(
            compact_notification(
                "A very long regional development with far too many words for the recent panel",
                6
            ),
            "A very long regional development with…"
        );
    }
}
