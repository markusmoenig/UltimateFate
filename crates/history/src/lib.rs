//! Deterministic historical and social simulation.
//!
//! The history system stores structured causes and consequences. Generated prose
//! is never authoritative: events, claims, beliefs, laws, ownership, resources,
//! relationships, and physical evidence remain inspectable data.

mod engine;
mod event;
mod ids;
mod model;
mod simulation;
mod world;

pub use engine::{
    AidResolutionKind, CrisisResolutionKind, CrisisResolutionOption, CrisisResolutionOutcome,
    HistoryEngine, MonthSummary, RegionalGoalOption, RegionalGoalOutcome, YearSummary,
};
pub use event::{
    Claim, ClaimAudience, ClaimOrigin, Consequence, EntityRef, EventDraft, EventPublicity,
    HistoricalEvent, HistoricalEventKind, Proposition, TruthValue,
};
pub use ids::{
    ClaimId, EventId, FactionId, FamilyId, GoalId, LawId, PartyId, PersonId, ProjectId, RouteId,
    SiteId, WorldItemId,
};
pub use model::{
    Belief, BeliefSource, Drive, Faction, Family, GrandStruggle, ItemCustodian, ItemProvenance,
    Knowledge, Law, LawKind, Occupation, Person, PhysicalEvidence, PhysicalEvidenceKind, Principle,
    RegionalGoal, RegionalGoalApproach, RegionalGoalKind, RegionalGoalStatus, RegionalParty,
    RegionalPartyKind, RegionalPartyStatus, RegionalRoute, RegionalSettlement, ResourceKind,
    SettlementProject, SettlementProjectKind, SettlementProjectPhase, SettlementRole,
    SignificantItem, SignificantItemKind, Site, StrategicActor, StrategicActorRole, StrategicFront,
    StrategicObjective, StrategicObjectiveKind, WorldDate,
};
pub use simulation::{
    ConflictSystem, ConstructionSystem, GoalSystem, GrandStrategySystem, ItemSystem,
    LogisticsSystem, MigrationSystem, PartySystem, PlanningSystem, RegionalEconomySystem,
    RegionalPlanningSystem, RegionalTrafficSystem, ScheduledIntent, SystemCadence, SystemId,
    TradeSystem, WorldIntent, WorldSimulator, WorldSystem, WorldView,
};
pub use world::{HistoricalWorld, WorldError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_town_starts_coherent() {
        let engine = HistoryEngine::seeded_town(7).expect("town should seed");
        let world = engine.world();

        assert_eq!(world.living_people().count(), 30);
        assert_eq!(world.families().len(), 3);
        assert_eq!(world.factions().len(), 3);
        assert!(world.validate().is_empty());
    }

    #[test]
    fn item_system_seeds_a_causal_object_with_fragmentary_magic_knowledge() {
        let mut engine = HistoryEngine::seeded_town(77).expect("town should seed");
        engine
            .begin_living_simulation()
            .expect("living systems should seed");
        let world = engine.world();
        let item = world
            .significant_items()
            .values()
            .next()
            .expect("strategic object");
        let formula = item.inscribed_formula.expect("inscribed formula");

        assert_eq!(item.object.0, item.id.0);
        assert!(world.rules().formula(formula).is_some());
        assert!(!item.provenance.is_empty());
        assert!(world.claims().values().any(|claim| matches!(
            claim.proposition,
            Proposition::FormulaProduces {
                formula: claimed,
                effect: ultimate_fate_content::MagicEffect::Heal,
            } if claimed == formula
        )));
        assert!(world.claims().values().any(|claim| matches!(
            claim.proposition,
            Proposition::FormulaRequires {
                formula: claimed,
                ..
            } if claimed == formula
        )));
        assert!(world.validate().is_empty());
    }

    #[test]
    fn twenty_year_replay_is_deterministic() {
        let mut first = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        let mut second = HistoryEngine::seeded_town(0x55aa).expect("town should seed");

        let first_years = first.simulate_years(20).expect("history should simulate");
        let second_years = second.simulate_years(20).expect("history should simulate");

        assert_eq!(first_years, second_years);
        assert_eq!(first.world(), second.world());
        assert!(first.world().validate().is_empty());
    }

    #[test]
    fn history_creates_crisis_rules_beliefs_and_physical_evidence() {
        let mut engine = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        engine.simulate_years(20).expect("history should simulate");
        let world = engine.world();

        assert!(
            world
                .events()
                .values()
                .any(|event| { event.kind == HistoricalEventKind::ShortageRecognized })
        );
        assert!(world.events().values().any(|event| {
            event.kind == HistoricalEventKind::LawEnacted && !event.causes.is_empty()
        }));
        assert!(
            world
                .claims()
                .values()
                .any(|claim| claim.truth == TruthValue::False)
        );
        assert!(world.knowledge().beliefs.values().any(|beliefs| {
            beliefs
                .values()
                .any(|belief| matches!(belief.source, BeliefSource::ToldBy(_)))
        }));
        assert!(
            world
                .sites()
                .values()
                .any(|site| site.physical_evidence.len() > 1)
        );
    }

    #[test]
    fn causal_inspection_walks_back_from_policy_to_harvest() {
        let mut engine = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        engine.simulate_years(20).expect("history should simulate");
        let world = engine.world();
        let law_event = world
            .events()
            .values()
            .find(|event| event.kind == HistoricalEventKind::LawEnacted)
            .expect("fixed seed should enact a law");
        let ancestors = world.causal_ancestors(law_event.id);

        assert!(ancestors.iter().any(|event| {
            world.events()[event].kind == HistoricalEventKind::ShortageRecognized
        }));
        assert!(
            ancestors
                .iter()
                .any(|event| { world.events()[event].kind == HistoricalEventKind::Harvest })
        );
    }

    #[test]
    fn player_crisis_resolutions_are_deterministic_structured_consequences() {
        let choices = [
            CrisisResolutionKind::EnforceEmergencyLaw,
            CrisisResolutionKind::OpenPublicStores,
            CrisisResolutionKind::BrokerCompromise,
        ];
        let mut distinct_outcomes = std::collections::BTreeSet::new();

        for choice in choices {
            let mut first = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
            let mut second = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
            first.simulate_years(20).expect("history should simulate");
            second.simulate_years(20).expect("history should simulate");
            let crisis = first
                .world()
                .events()
                .values()
                .rev()
                .find(|event| event.kind == HistoricalEventKind::ShortageRecognized)
                .map(|event| event.id)
                .expect("generated history should contain a crisis");

            let options = first
                .crisis_resolution_options(crisis)
                .expect("crisis should offer resolutions");
            assert_eq!(options.len(), 3);
            let first_outcome = first
                .resolve_crisis(crisis, choice)
                .expect("choice should resolve crisis");
            let second_outcome = second
                .resolve_crisis(crisis, choice)
                .expect("replayed choice should resolve crisis");

            assert_eq!(first_outcome, second_outcome);
            assert_eq!(first.world(), second.world());
            assert!(first.world().validate().is_empty());
            let event = &first.world().events()[&first_outcome.event];
            assert_eq!(event.kind, HistoricalEventKind::PlayerIntervention);
            assert_eq!(event.causes, vec![crisis]);
            assert!(!event.consequences.is_empty());
            assert!(matches!(
                first.resolve_crisis(crisis, choice),
                Err(WorldError::CrisisAlreadyResolved(event)) if event == crisis
            ));
            distinct_outcomes.insert((
                first_outcome.food_after,
                first_outcome.coin_after,
                first_outcome.active_laws,
            ));
        }

        assert_eq!(distinct_outcomes.len(), 3);
    }

    #[test]
    fn many_campaign_seeds_remain_structurally_coherent() {
        for seed in 0..64 {
            let mut engine = HistoryEngine::seeded_town(seed).expect("town should seed");
            engine
                .simulate_years(30)
                .expect("history should simulate for thirty years");
            let problems = engine.world().validate();
            assert!(
                problems.is_empty(),
                "seed {seed} violated invariants: {problems:?}"
            );
        }
    }

    #[test]
    fn campaign_seeds_create_distinct_causal_crisis_policies() {
        let mut policies = std::collections::BTreeSet::new();
        let mut authority_principles = std::collections::BTreeSet::new();

        for seed in 0..64 {
            let mut engine = HistoryEngine::seeded_town(seed).expect("town should seed");
            engine.simulate_years(20).expect("history should simulate");
            let site = &engine.world().sites()[&engine.primary_site()];
            let law = site
                .laws
                .values()
                .filter(|law| law.active)
                .min_by_key(|law| law.id)
                .expect("twenty years should produce an active crisis policy");
            policies.insert(law.kind);
            authority_principles.insert(engine.world().factions()[&law.authority].principle);
        }

        assert!(
            policies.len() >= 4,
            "seed corpus produced only {policies:?}"
        );
        assert!(
            authority_principles.len() >= 6,
            "seed corpus produced only {authority_principles:?}"
        );
    }

    #[test]
    fn clearing_a_history_born_dungeon_becomes_new_history() {
        let mut engine = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        engine.simulate_years(20).expect("history should simulate");
        let crisis = engine
            .world()
            .events()
            .values()
            .rev()
            .find(|event| event.kind == HistoricalEventKind::ShortageRecognized)
            .map(|event| event.id)
            .expect("generated history should contain a crisis");
        let site = engine.world().events()[&crisis].location;
        engine
            .begin_living_simulation()
            .expect("living simulation should seed strategic items");
        let item = engine
            .world()
            .significant_items()
            .values()
            .find(|item| item.provenance.iter().any(|entry| entry.event == crisis))
            .expect("crisis should produce a strategic item")
            .clone();
        let front_before = engine.world().struggle().balance(item.strategic_front);
        let territory_before = engine.world().struggle().balance(StrategicFront::Territory);
        let iron_before = engine.world().sites()[&site]
            .resources
            .get(&ResourceKind::Iron)
            .copied()
            .unwrap_or_default();

        let recovery = engine
            .record_item_recovered_by_player(item.id, "Ration Archive Depths")
            .expect("player recovery should be recorded");
        assert_eq!(
            engine.world().events()[&recovery].kind,
            HistoricalEventKind::ArtifactRecovered
        );
        assert_eq!(
            engine.world().significant_items()[&item.id].custodian,
            ItemCustodian::Player
        );
        let discovery = engine
            .record_formula_reconstructed_by_player(item.id)
            .expect("the carried inscription should be reconstructable");
        assert_eq!(
            engine.world().events()[&discovery].kind,
            HistoricalEventKind::FormulaReconstructed
        );
        assert_eq!(engine.world().events()[&discovery].causes, vec![recovery]);

        let first = engine
            .record_dungeon_cleared_with_item(crisis, "Ration Archive Depths", item.id)
            .expect("dungeon completion should be recorded");
        let replay = engine
            .record_dungeon_cleared_with_item(crisis, "Ration Archive Depths", item.id)
            .expect("recording is idempotent");

        assert_eq!(first, replay);
        let event = &engine.world().events()[&first];
        assert_eq!(event.kind, HistoricalEventKind::DungeonCleared);
        assert_eq!(event.causes, vec![crisis, discovery]);
        assert_eq!(
            engine.world().sites()[&site]
                .resources
                .get(&ResourceKind::Iron)
                .copied(),
            Some(iron_before + 6)
        );
        assert!(
            engine.world().sites()[&site]
                .physical_evidence
                .iter()
                .any(|evidence| evidence.description.contains(&item.name))
        );
        let recovered = &engine.world().significant_items()[&item.id];
        assert_eq!(recovered.custodian, ItemCustodian::Site(site));
        assert_eq!(
            recovered.provenance.last().map(|entry| entry.event),
            Some(first)
        );
        assert_eq!(
            engine.world().struggle().balance(item.strategic_front),
            front_before + 4
        );
        assert_eq!(
            engine.world().struggle().balance(StrategicFront::Territory),
            territory_before + 2
        );
        assert!(engine.world().validate().is_empty());
    }

    #[test]
    fn strategic_item_generation_is_deterministic_and_history_born() {
        let mut first = HistoryEngine::seeded_town(0xcafe).expect("town should seed");
        let mut replay = HistoryEngine::seeded_town(0xcafe).expect("town should seed");
        first.simulate_years(20).expect("history should simulate");
        replay.simulate_years(20).expect("history should simulate");

        first
            .begin_living_simulation()
            .expect("campaign systems should begin");
        replay
            .begin_living_simulation()
            .expect("campaign systems should replay");

        assert_eq!(
            first.world().significant_items(),
            replay.world().significant_items()
        );
        assert_eq!(first.world().significant_items().len(), 1);
        let item = first
            .world()
            .significant_items()
            .values()
            .next()
            .expect("seeded item");
        assert_eq!(item.custodian, ItemCustodian::Lost);
        assert!(
            item.provenance
                .iter()
                .all(|entry| first.world().events().contains_key(&entry.event))
        );
        assert!(first.world().validate().is_empty());
    }

    #[test]
    fn living_projects_spend_resources_build_suffer_damage_and_recover() {
        let mut first = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        let mut replay = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        first.simulate_years(20).expect("history should simulate");
        replay.simulate_years(20).expect("history should simulate");
        let project = first
            .begin_living_simulation()
            .expect("living simulation should begin");
        assert_eq!(
            replay.begin_living_simulation(),
            Ok(project),
            "project generation should replay"
        );
        assert_eq!(first.world().projects().len(), 3);
        assert_eq!(
            first
                .world()
                .projects()
                .values()
                .map(|project| project.sponsor)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3
        );
        assert_eq!(
            first
                .world()
                .projects()
                .values()
                .map(|project| project.kind)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3
        );
        assert!(first.world().projects().values().all(|project| {
            !project.workers.is_empty()
                && project.workers.iter().all(|worker| {
                    first.world().factions()[&project.sponsor]
                        .members
                        .contains(worker)
                })
        }));
        let site = first.primary_site();
        let sponsor = first.world().projects()[&project].sponsor;
        let timber_before = first.world().sites()[&site].resources[&ResourceKind::Timber];
        let treasury_before = first.world().factions()[&sponsor].treasury;

        let mut kinds = Vec::new();
        for _ in 0..18 {
            let first_month = first.advance_month().expect("month should advance");
            let replay_month = replay.advance_month().expect("month should replay");
            assert_eq!(first_month, replay_month);
            for event in first_month.events {
                kinds.push(first.world().events()[&event].kind);
            }
        }

        assert_eq!(first.world(), replay.world());
        assert!(kinds.contains(&HistoricalEventKind::ProjectStarted));
        assert!(kinds.contains(&HistoricalEventKind::ProjectStalled));
        assert!(kinds.contains(&HistoricalEventKind::SupplyShipment));
        assert!(kinds.contains(&HistoricalEventKind::ProjectCompleted));
        assert!(kinds.contains(&HistoricalEventKind::ProjectDamaged));
        assert!(kinds.contains(&HistoricalEventKind::ProjectRepaired));
        assert!(
            first.world().sites()[&site].resources[&ResourceKind::Timber] < timber_before,
            "construction and repair should consume timber"
        );
        assert!(
            first.world().factions()[&sponsor].treasury < treasury_before,
            "the sponsoring faction should pay for construction"
        );
        assert_eq!(
            first.world().projects()[&project].phase,
            SettlementProjectPhase::Completed
        );
        assert!(first.world().validate().is_empty());
    }

    #[test]
    fn living_settlements_remain_coherent_across_many_seeds_and_months() {
        for seed in 0..64 {
            let mut engine = HistoryEngine::seeded_town(seed).expect("town should seed");
            engine.simulate_years(20).expect("history should simulate");
            engine
                .begin_living_simulation()
                .expect("living simulation should begin");
            assert_eq!(engine.world().projects().len(), 3);
            let active_traffic = engine
                .world()
                .regional_parties()
                .values()
                .filter(|party| party.status == RegionalPartyStatus::Traveling)
                .count();
            assert!(
                active_traffic >= 3,
                "seed {seed} began with only {active_traffic} active regional parties"
            );
            for _ in 0..36 {
                engine.advance_month().expect("month should advance");
            }
            let problems = engine.world().validate();
            assert!(
                problems.is_empty(),
                "seed {seed} produced an incoherent living settlement: {problems:?}"
            );
            assert!(engine.world().projects().values().all(|project| {
                engine.world().sites().contains_key(&project.site)
                    && engine.world().factions().contains_key(&project.sponsor)
                    && engine.world().events().contains_key(&project.last_event)
            }));
        }
    }

    #[test]
    fn world_system_registration_order_does_not_change_intent_order() {
        let mut engine = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        engine.simulate_years(20).expect("history should simulate");
        engine
            .begin_living_simulation()
            .expect("living simulation should begin");
        let normal = WorldSimulator::with_systems(vec![
            Box::new(RegionalEconomySystem),
            Box::new(PartySystem),
            Box::new(TradeSystem),
            Box::new(LogisticsSystem),
            Box::new(ConstructionSystem),
            Box::new(ConflictSystem),
            Box::new(GoalSystem),
            Box::new(MigrationSystem),
            Box::new(GrandStrategySystem),
        ]);
        let reversed = WorldSimulator::with_systems(vec![
            Box::new(GrandStrategySystem),
            Box::new(MigrationSystem),
            Box::new(GoalSystem),
            Box::new(ConflictSystem),
            Box::new(ConstructionSystem),
            Box::new(LogisticsSystem),
            Box::new(TradeSystem),
            Box::new(PartySystem),
            Box::new(RegionalEconomySystem),
        ]);

        assert_eq!(
            normal.intents(
                SystemCadence::Monthly,
                engine.world(),
                engine.primary_site(),
                engine.foundation_event()
            ),
            reversed.intents(
                SystemCadence::Monthly,
                engine.world(),
                engine.primary_site(),
                engine.foundation_event()
            )
        );
    }

    #[test]
    fn modules_can_be_isolated_and_local_events_move_strategic_fronts() {
        let mut engine = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        engine.simulate_years(20).expect("history should simulate");
        engine
            .begin_living_simulation()
            .expect("living simulation should begin");
        let logistics_only = WorldSimulator::with_systems(vec![Box::new(LogisticsSystem)]);
        assert!(
            logistics_only
                .intents(
                    SystemCadence::Monthly,
                    engine.world(),
                    engine.primary_site(),
                    engine.foundation_event()
                )
                .iter()
                .all(|scheduled| scheduled.system == SystemId::Logistics)
        );

        let initial = engine.world().struggle().clone();
        for _ in 0..12 {
            engine.advance_month().expect("month should advance");
        }
        assert_ne!(engine.world().struggle(), &initial);
        assert!(engine.world().struggle().last_event.is_some());
        assert!(
            engine
                .world()
                .struggle()
                .actors
                .values()
                .all(|actor| actor.objective.is_some() && actor.last_event.is_some())
        );
        assert!(engine.world().events().values().any(|event| {
            event
                .consequences
                .iter()
                .any(|consequence| matches!(consequence, Consequence::ShiftStrategicFront { .. }))
        }));
        let strategic_event = engine
            .world()
            .struggle()
            .last_event
            .expect("strategy event");
        assert!(
            engine.world().events()[&strategic_event]
                .consequences
                .iter()
                .any(|consequence| matches!(
                    consequence,
                    Consequence::SetRouteDisrupted { .. } | Consequence::ChangeResource { .. }
                ))
        );
    }

    #[test]
    fn regional_systems_create_deterministic_trade_conflict_and_migration() {
        let mut first = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        let mut replay = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        for engine in [&mut first, &mut replay] {
            engine.simulate_years(20).expect("history should simulate");
            engine
                .begin_living_simulation()
                .expect("regional simulation should begin");
        }

        assert!((5..=8).contains(&first.world().regional_settlements().len()));
        assert_eq!(
            first.world().regional_settlements(),
            replay.world().regional_settlements()
        );
        assert_eq!(first.world().routes(), replay.world().routes());
        assert_eq!(first.world().atlas().width(), 256);
        assert_eq!(first.world().atlas(), replay.world().atlas());
        assert!(
            first
                .world()
                .regional_settlements()
                .values()
                .all(|settlement| {
                    first
                        .world()
                        .atlas()
                        .cell(settlement.position)
                        .is_some_and(|cell| cell.is_passable_land())
                })
        );
        assert!(first.world().routes().values().all(|route| {
            first
                .world()
                .regional_settlements()
                .contains_key(&route.first)
                && first
                    .world()
                    .regional_settlements()
                    .contains_key(&route.second)
                && route.path.first().copied()
                    == Some(first.world().regional_settlements()[&route.first].position)
                && route.path.last().copied()
                    == Some(first.world().regional_settlements()[&route.second].position)
        }));

        for _ in 0..48 {
            assert_eq!(
                first.advance_month(),
                replay.advance_month(),
                "monthly regional resolution should replay"
            );
        }
        assert_eq!(first.world(), replay.world());
        let kinds = first
            .world()
            .events()
            .values()
            .map(|event| event.kind)
            .collect::<Vec<_>>();
        assert!(kinds.contains(&HistoricalEventKind::RegionalShortage));
        assert!(kinds.contains(&HistoricalEventKind::RegionalTrade));
        assert!(kinds.contains(&HistoricalEventKind::Migration));
        assert!(kinds.contains(&HistoricalEventKind::RouteDisrupted));
        assert!(kinds.contains(&HistoricalEventKind::RouteReopened));

        let migration = first
            .world()
            .events()
            .values()
            .find(|event| event.kind == HistoricalEventKind::Migration)
            .expect("regional shortage should cause migration");
        let migration_ancestors = first.world().causal_ancestors(migration.id);
        let migration_ancestor_kinds = migration_ancestors
            .iter()
            .map(|ancestor| first.world().events()[ancestor].kind)
            .collect::<Vec<_>>();
        assert!(
            migration_ancestors.iter().any(|ancestor| {
                first.world().events()[ancestor].kind == HistoricalEventKind::RegionalShortage
            }),
            "migration ancestry was {migration_ancestor_kinds:?}"
        );
        assert!(first.world().validate().is_empty());
    }

    #[test]
    fn regional_problems_generate_resolvable_faction_contracts() {
        let mut first = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        let mut replay = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        for engine in [&mut first, &mut replay] {
            engine.simulate_years(20).expect("history should simulate");
            engine
                .begin_living_simulation()
                .expect("regional simulation should begin");
        }

        let mut saw_route = first
            .world()
            .regional_goals()
            .values()
            .any(|goal| matches!(goal.kind, RegionalGoalKind::SecureRoute(_)));
        let mut saw_relief = first
            .world()
            .regional_goals()
            .values()
            .any(|goal| matches!(goal.kind, RegionalGoalKind::RelieveShortage(_)));
        let mut resolved_events = Vec::new();
        for _ in 0..72 {
            assert_eq!(first.advance_month(), replay.advance_month());
            let mut goals = first.world().regional_goals().values();
            let open = goals
                .clone()
                .find(|goal| {
                    goal.status == RegionalGoalStatus::Open
                        && match goal.kind {
                            RegionalGoalKind::SecureRoute(_) => !saw_route,
                            RegionalGoalKind::RelieveShortage(_) => !saw_relief,
                        }
                })
                .or_else(|| goals.find(|goal| goal.status == RegionalGoalStatus::Open))
                .cloned();
            let Some(goal) = open else {
                continue;
            };
            match goal.kind {
                RegionalGoalKind::SecureRoute(_) => saw_route = true,
                RegionalGoalKind::RelieveShortage(_) => saw_relief = true,
            }
            let options = first
                .regional_goal_options(goal.id)
                .expect("open contract should have responses");
            assert_eq!(options.len(), 3);
            assert_eq!(
                options,
                replay
                    .regional_goal_options(goal.id)
                    .expect("responses should replay")
            );
            let approach = options[0].approach;
            if let RegionalGoalKind::SecureRoute(route) = goal.kind
                && approach == RegionalGoalApproach::RestoreByForce
            {
                let first_raiders = first.active_route_raiders(route);
                let replay_raiders = replay.active_route_raiders(route);
                assert_eq!(first_raiders, replay_raiders);
                if !first_raiders.is_empty() {
                    assert_eq!(
                        first.resolve_regional_goal(goal.id, approach),
                        Err(WorldError::RegionalGoalRequiresCombat(goal.id))
                    );
                    for raider in first_raiders {
                        assert_eq!(
                            first
                                .defeat_regional_party(raider)
                                .expect("force must defeat the raiding party"),
                            replay
                                .defeat_regional_party(raider)
                                .expect("combat precondition should replay")
                        );
                    }
                }
            }
            let first_outcome = first
                .resolve_regional_goal(goal.id, approach)
                .expect("player response should resolve");
            let replay_outcome = replay
                .resolve_regional_goal(goal.id, approach)
                .expect("player response should replay");
            assert_eq!(first_outcome, replay_outcome);
            assert!(
                first.world().events()[&first_outcome.event]
                    .causes
                    .contains(&goal.cause)
            );
            resolved_events.push(first_outcome.event);
            if saw_route && saw_relief {
                break;
            }
        }

        assert!(
            saw_route,
            "route disruption should create a security contract"
        );
        assert!(saw_relief, "shortage should create a relief contract");
        assert!(!resolved_events.is_empty());
        assert_eq!(first.world(), replay.world());
        assert!(first.world().validate().is_empty());
    }

    #[test]
    fn regional_parties_carry_real_cargo_and_arrive_deterministically() {
        let mut first = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        let mut replay = HistoryEngine::seeded_town(0x55aa).expect("town should seed");
        for engine in [&mut first, &mut replay] {
            engine.simulate_years(20).expect("history should simulate");
            engine
                .begin_living_simulation()
                .expect("regional simulation should begin");
        }

        let party = (0..96)
            .find_map(|_| {
                assert_eq!(first.advance_month(), replay.advance_month());
                first
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
        assert_eq!(first.world(), replay.world());

        let record = first.world().regional_parties()[&party].clone();
        let RegionalPartyKind::TradeCaravan { resource, amount } = record.kind else {
            unreachable!("selected party is a caravan");
        };
        let destination_before = first.world().sites()[&record.destination].resources[&resource];

        assert_eq!(
            first.advance_regional_parties(400),
            replay.advance_regional_parties(400)
        );
        assert_eq!(
            first.world().sites()[&record.destination].resources[&resource],
            destination_before,
            "cargo must remain with the moving party"
        );
        assert_eq!(first.world().regional_parties()[&party].progress, 400);
        let arriving_cargo = first
            .world()
            .regional_parties()
            .values()
            .filter(|candidate| {
                candidate.status == RegionalPartyStatus::Traveling
                    && candidate.destination == record.destination
                    && candidate.progress.saturating_add(600) >= 1_000
            })
            .filter_map(|candidate| match candidate.kind {
                RegionalPartyKind::TradeCaravan {
                    resource: carried,
                    amount,
                } if carried == resource => Some(amount),
                _ => None,
            })
            .sum::<i64>();
        assert!(arriving_cargo >= amount);

        let arrivals = first
            .advance_regional_parties(600)
            .expect("caravan should arrive");
        assert_eq!(
            arrivals,
            replay
                .advance_regional_parties(600)
                .expect("replay caravan should arrive")
        );
        assert_eq!(
            first.world().sites()[&record.destination].resources[&resource],
            destination_before + arriving_cargo
        );
        assert_eq!(
            first.world().regional_parties()[&party].status,
            RegionalPartyStatus::Traveling
        );
        assert_eq!(
            first.world().regional_parties()[&party].kind,
            RegionalPartyKind::ReturningCaravan
        );
        assert!(arrivals.iter().any(|event| {
            first.world().events()[event].kind == HistoricalEventKind::RegionalPartyArrived
                && first.world().events()[event]
                    .causes
                    .contains(&record.last_event)
        }));
        assert_eq!(first.world(), replay.world());
        assert!(first.world().validate().is_empty());
    }
}
