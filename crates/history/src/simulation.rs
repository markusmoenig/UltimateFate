//! Modular, deterministic world-simulation scheduling.
//!
//! Systems observe immutable world state and emit intents. They never call one
//! another or mutate authoritative state directly. The history engine resolves
//! the ordered intents atomically and records resulting state changes in the
//! causal ledger.

use crate::{
    HistoricalWorld, PartyId, ProjectId, RegionalGoalKind, RegionalGoalStatus, RegionalPartyStatus,
    ResourceKind, SettlementProjectPhase,
    ids::{EventId, RouteId, SiteId},
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SystemId {
    Planning,
    Items,
    RegionalPlanning,
    RegionalTraffic,
    Economy,
    Parties,
    Trade,
    Logistics,
    Construction,
    Conflict,
    Goals,
    Migration,
    GrandStrategy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemCadence {
    CampaignStart,
    LivingPulse,
    Monthly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldIntent {
    PlanSettlement,
    SeedStrategicItems,
    SeedRegion,
    SeedRegionalTraffic,
    AdvanceRegionalEconomy(SiteId),
    AdvanceRegionalParty {
        party: PartyId,
        step: u16,
    },
    MoveRegionalTrade {
        route: RouteId,
        from: SiteId,
        to: SiteId,
        resource: ResourceKind,
        amount: i64,
    },
    ImportProjectSupplies(ProjectId),
    MaintainProject(ProjectId),
    ApplyProjectPressure(ProjectId),
    ApplyRegionalPressure(RouteId),
    ProposeRouteGoal(RouteId),
    ProposeReliefGoal(SiteId),
    MigrateRegionalPopulation {
        from: SiteId,
        to: SiteId,
        amount: u32,
    },
    AssessGrandStrategy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledIntent {
    pub system: SystemId,
    pub intent: WorldIntent,
}

pub struct WorldView<'a> {
    pub world: &'a HistoricalWorld,
    pub primary_site: SiteId,
    pub foundation_event: EventId,
}

pub trait WorldSystem: Send + Sync {
    fn id(&self) -> SystemId;
    fn cadence(&self) -> SystemCadence;
    fn runs_at(&self, cadence: SystemCadence) -> bool {
        self.cadence() == cadence
    }
    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>);
}

pub struct WorldSimulator {
    systems: Vec<Box<dyn WorldSystem>>,
}

impl Default for WorldSimulator {
    fn default() -> Self {
        Self::with_systems(vec![
            Box::new(PlanningSystem),
            Box::new(ItemSystem),
            Box::new(RegionalPlanningSystem),
            Box::new(RegionalTrafficSystem),
            Box::new(RegionalEconomySystem),
            Box::new(PartySystem),
            Box::new(TradeSystem),
            Box::new(LogisticsSystem),
            Box::new(ConstructionSystem),
            Box::new(ConflictSystem),
            Box::new(GoalSystem),
            Box::new(MigrationSystem),
            Box::new(GrandStrategySystem),
        ])
    }
}

impl WorldSimulator {
    pub fn with_systems(systems: Vec<Box<dyn WorldSystem>>) -> Self {
        Self { systems }
    }

    pub fn registered_systems(&self) -> Vec<SystemId> {
        self.systems.iter().map(|system| system.id()).collect()
    }

    pub fn intents(
        &self,
        cadence: SystemCadence,
        world: &HistoricalWorld,
        primary_site: SiteId,
        foundation_event: EventId,
    ) -> Vec<ScheduledIntent> {
        let view = WorldView {
            world,
            primary_site,
            foundation_event,
        };
        let mut scheduled = Vec::new();
        for system in &self.systems {
            if !system.runs_at(cadence) {
                continue;
            }
            let mut intents = Vec::new();
            system.evaluate(&view, &mut intents);
            scheduled.extend(intents.into_iter().map(|intent| ScheduledIntent {
                system: system.id(),
                intent,
            }));
        }
        scheduled.sort_by_key(|scheduled| {
            (
                system_priority(scheduled.system),
                intent_subject(scheduled.intent),
            )
        });
        scheduled
    }
}

fn system_priority(system: SystemId) -> u8 {
    match system {
        SystemId::Planning => 0,
        SystemId::Items => 1,
        SystemId::RegionalPlanning => 2,
        SystemId::RegionalTraffic => 3,
        SystemId::Economy => 5,
        SystemId::Parties => 7,
        SystemId::Trade => 8,
        SystemId::Logistics => 10,
        SystemId::Construction => 20,
        SystemId::Conflict => 30,
        SystemId::Goals => 32,
        SystemId::Migration => 35,
        SystemId::GrandStrategy => 40,
    }
}

fn intent_subject(intent: WorldIntent) -> u64 {
    match intent {
        WorldIntent::PlanSettlement
        | WorldIntent::SeedStrategicItems
        | WorldIntent::SeedRegion
        | WorldIntent::SeedRegionalTraffic
        | WorldIntent::AssessGrandStrategy => 0,
        WorldIntent::AdvanceRegionalEconomy(site) => site.0,
        WorldIntent::AdvanceRegionalParty { party, .. } => party.0,
        WorldIntent::MoveRegionalTrade { route, .. }
        | WorldIntent::ApplyRegionalPressure(route)
        | WorldIntent::ProposeRouteGoal(route) => route.0,
        WorldIntent::ProposeReliefGoal(site) => site.0,
        WorldIntent::MigrateRegionalPopulation { from, to, .. } => from.0.rotate_left(32) ^ to.0,
        WorldIntent::ImportProjectSupplies(project)
        | WorldIntent::MaintainProject(project)
        | WorldIntent::ApplyProjectPressure(project) => project.0,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlanningSystem;

impl WorldSystem for PlanningSystem {
    fn id(&self) -> SystemId {
        SystemId::Planning
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::CampaignStart
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        if view.world.projects().is_empty() {
            output.push(WorldIntent::PlanSettlement);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LogisticsSystem;

#[derive(Clone, Copy, Debug)]
pub struct ItemSystem;

impl WorldSystem for ItemSystem {
    fn id(&self) -> SystemId {
        SystemId::Items
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::CampaignStart
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        if view.world.significant_items().is_empty() {
            output.push(WorldIntent::SeedStrategicItems);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RegionalPlanningSystem;

impl WorldSystem for RegionalPlanningSystem {
    fn id(&self) -> SystemId {
        SystemId::RegionalPlanning
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::CampaignStart
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        if view.world.regional_settlements().is_empty() {
            output.push(WorldIntent::SeedRegion);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RegionalEconomySystem;

#[derive(Clone, Copy, Debug)]
pub struct RegionalTrafficSystem;

impl WorldSystem for RegionalTrafficSystem {
    fn id(&self) -> SystemId {
        SystemId::RegionalTraffic
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::CampaignStart
    }

    fn runs_at(&self, cadence: SystemCadence) -> bool {
        matches!(
            cadence,
            SystemCadence::CampaignStart | SystemCadence::LivingPulse
        )
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        let region_exists =
            !view.world.regional_settlements().is_empty() && !view.world.routes().is_empty();
        let active_traffic = view
            .world
            .regional_parties()
            .values()
            .filter(|party| party.status == RegionalPartyStatus::Traveling)
            .count();
        if region_exists && active_traffic < 3 {
            output.push(WorldIntent::SeedRegionalTraffic);
        }
    }
}

impl WorldSystem for RegionalEconomySystem {
    fn id(&self) -> SystemId {
        SystemId::Economy
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::Monthly
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        output.extend(
            view.world
                .regional_settlements()
                .keys()
                .copied()
                .map(WorldIntent::AdvanceRegionalEconomy),
        );
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TradeSystem;

#[derive(Clone, Copy, Debug)]
pub struct PartySystem;

impl WorldSystem for PartySystem {
    fn id(&self) -> SystemId {
        SystemId::Parties
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::Monthly
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        output.extend(
            view.world
                .regional_parties()
                .values()
                .filter(|party| party.status == RegionalPartyStatus::Traveling)
                .map(|party| WorldIntent::AdvanceRegionalParty {
                    party: party.id,
                    step: 1_000,
                }),
        );
    }
}

impl WorldSystem for TradeSystem {
    fn id(&self) -> SystemId {
        SystemId::Trade
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::Monthly
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        for route in view
            .world
            .routes()
            .values()
            .filter(|route| !route.disrupted)
        {
            let first_months = food_reserve_months(view.world, route.first);
            let second_months = food_reserve_months(view.world, route.second);
            let direction = if first_months >= 4 && second_months <= 2 {
                Some((route.first, route.second))
            } else if second_months >= 4 && first_months <= 2 {
                Some((route.second, route.first))
            } else {
                None
            };
            if let Some((from, to)) = direction {
                let source = &view.world.sites()[&from];
                let source_consumption = view.world.regional_settlements()[&from]
                    .monthly_consumption
                    .get(&ResourceKind::Food)
                    .copied()
                    .unwrap_or(1)
                    .max(1);
                let available = source.resources[&ResourceKind::Food]
                    .saturating_sub(source_consumption.saturating_mul(3));
                let amount = available.clamp(0, 30);
                if amount > 0 {
                    output.push(WorldIntent::MoveRegionalTrade {
                        route: route.id,
                        from,
                        to,
                        resource: ResourceKind::Food,
                        amount,
                    });
                }
            }
        }
    }
}

impl WorldSystem for LogisticsSystem {
    fn id(&self) -> SystemId {
        SystemId::Logistics
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::Monthly
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        output.extend(view.world.projects().values().filter_map(|project| {
            matches!(
                project.phase,
                SettlementProjectPhase::Stalled | SettlementProjectPhase::Damaged
            )
            .then_some(WorldIntent::ImportProjectSupplies(project.id))
        }));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MigrationSystem;

impl WorldSystem for MigrationSystem {
    fn id(&self) -> SystemId {
        SystemId::Migration
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::Monthly
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        // Migration is a major demographic consequence, not a monthly ambient
        // transaction. One annual departure per affected settlement preserves
        // causality without producing endless near-identical refugee columns.
        if view.world.date.month != 6 {
            return;
        }
        for settlement in view
            .world
            .regional_settlements()
            .values()
            .filter(|settlement| settlement.shortage && settlement.population > 20)
        {
            let destination = view
                .world
                .routes()
                .values()
                .filter(|route| route.connects(settlement.site) && !route.disrupted)
                .filter_map(|route| route.other_end(settlement.site))
                .max_by_key(|site| (food_reserve_months(view.world, *site), *site));
            if let Some(to) = destination
                && food_reserve_months(view.world, to) >= 3
            {
                output.push(WorldIntent::MigrateRegionalPopulation {
                    from: settlement.site,
                    to,
                    amount: (settlement.population / 20).clamp(1, 12),
                });
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ConstructionSystem;

impl WorldSystem for ConstructionSystem {
    fn id(&self) -> SystemId {
        SystemId::Construction
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::Monthly
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        output.extend(view.world.projects().values().filter_map(|project| {
            (project.phase != SettlementProjectPhase::Completed)
                .then_some(WorldIntent::MaintainProject(project.id))
        }));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ConflictSystem;

impl WorldSystem for ConflictSystem {
    fn id(&self) -> SystemId {
        SystemId::Conflict
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::Monthly
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        output.extend(view.world.projects().values().filter_map(|project| {
            (project.phase == SettlementProjectPhase::Completed)
                .then_some(WorldIntent::ApplyProjectPressure(project.id))
        }));
        output.extend(view.world.routes().values().filter_map(|route| {
            if route.disrupted {
                Some(WorldIntent::ApplyRegionalPressure(route.id))
            } else {
                let shortage = [route.first, route.second].into_iter().any(|site| {
                    view.world
                        .regional_settlements()
                        .get(&site)
                        .is_some_and(|settlement| settlement.shortage)
                });
                let darkness = (-view
                    .world
                    .struggle()
                    .balance(crate::StrategicFront::Military))
                .max(0) as u64;
                let roll = regional_roll(view.world, route.id);
                // Danger is a monthly hazard, not a direct percentage. Keep
                // disruptions historically significant and prevent a negative
                // military balance from becoming an irreversible feedback loop.
                let monthly_risk = 1_u64
                    .saturating_add(u64::from(route.danger) / 25)
                    .saturating_add(darkness / 50)
                    .saturating_add(u64::from(shortage))
                    .min(4);
                (roll < monthly_risk).then_some(WorldIntent::ApplyRegionalPressure(route.id))
            }
        }));
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GoalSystem;

impl WorldSystem for GoalSystem {
    fn id(&self) -> SystemId {
        SystemId::Goals
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::Monthly
    }

    fn evaluate(&self, view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        for route in view.world.routes().values().filter(|route| route.disrupted) {
            let already_open = view.world.regional_goals().values().any(|goal| {
                goal.status == RegionalGoalStatus::Open
                    && goal.kind == RegionalGoalKind::SecureRoute(route.id)
            });
            if !already_open {
                output.push(WorldIntent::ProposeRouteGoal(route.id));
            }
        }
        for settlement in view
            .world
            .regional_settlements()
            .values()
            .filter(|settlement| settlement.shortage)
        {
            let already_open = view.world.regional_goals().values().any(|goal| {
                goal.status == RegionalGoalStatus::Open
                    && goal.kind == RegionalGoalKind::RelieveShortage(settlement.site)
            });
            if !already_open {
                output.push(WorldIntent::ProposeReliefGoal(settlement.site));
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GrandStrategySystem;

impl WorldSystem for GrandStrategySystem {
    fn id(&self) -> SystemId {
        SystemId::GrandStrategy
    }

    fn cadence(&self) -> SystemCadence {
        SystemCadence::Monthly
    }

    fn evaluate(&self, _view: &WorldView<'_>, output: &mut Vec<WorldIntent>) {
        output.push(WorldIntent::AssessGrandStrategy);
    }
}

fn food_reserve_months(world: &HistoricalWorld, site: SiteId) -> i64 {
    let food = world.sites()[&site]
        .resources
        .get(&ResourceKind::Food)
        .copied()
        .unwrap_or_default();
    let consumption = world.regional_settlements()[&site]
        .monthly_consumption
        .get(&ResourceKind::Food)
        .copied()
        .unwrap_or(1)
        .max(1);
    food / consumption
}

fn regional_roll(world: &HistoricalWorld, route: RouteId) -> u64 {
    let month = world.date.year as i64 as u64 * 12 + u64::from(world.date.month);
    let mut value = world.campaign_seed ^ route.0.rotate_left(17) ^ month.rotate_left(31);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value.wrapping_mul(0x94d0_49bb_1331_11eb) % 100
}
