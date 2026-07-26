use std::error::Error;

use ultimate_fate_history::{HistoryEngine, ResourceKind};

fn main() -> Result<(), Box<dyn Error>> {
    let seed = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0x55aa);
    let years = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);

    let mut engine = HistoryEngine::seeded_town(seed)?;
    let summaries = engine.simulate_years(years)?;
    let world = engine.world();
    let site = &world.sites()[&engine.primary_site()];

    println!("{} — seed {seed}, year {}", site.name, world.date.year);
    println!(
        "{} living people, {} families, {} factions, {} events",
        world.living_people().count(),
        world.families().len(),
        world.factions().len(),
        world.events().len()
    );
    println!(
        "Food reserve: {}",
        site.resources
            .get(&ResourceKind::Food)
            .copied()
            .unwrap_or_default()
    );
    println!(
        "Active laws: {}",
        site.laws.values().filter(|law| law.active).count()
    );
    println!("Physical evidence: {}", site.physical_evidence.len());

    println!("\nFactions");
    for faction in world.factions().values() {
        let leader = &world.people()[&faction.leader];
        let surname = &world.families()[&leader.family].surname;
        println!(
            "- {} ({:?}): led by {} {}, relations {:?}",
            faction.name, faction.principle, leader.given_name, surname, faction.relations
        );
    }

    println!("\nAnnual state");
    for year in &summaries {
        println!(
            "- Year {:>2}: population {:>2}, food {:>4}, {} events",
            year.year, year.population, year.food, year.events_created
        );
    }

    println!("\nRecent structured events");
    for event in world.events().values().rev().take(15).rev() {
        println!(
            "- [{} / {:?}] {} (causes: {:?})",
            event.id, event.kind, event.summary, event.causes
        );
    }

    println!("\nContested or false claims");
    for claim in world
        .claims()
        .values()
        .filter(|claim| claim.truth != ultimate_fate_history::TruthValue::True)
    {
        println!(
            "- [{:?}] {}",
            claim.truth,
            world.describe_claim(&claim.proposition)
        );
    }

    let problems = world.validate();
    if problems.is_empty() {
        println!("\nInvariant check: OK");
    } else {
        println!("\nInvariant problems:");
        for problem in problems {
            println!("- {problem}");
        }
    }

    Ok(())
}
