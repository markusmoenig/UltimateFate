use std::error::Error;

use ultimate_fate_history::HistoryEngine;
use ultimate_fate_text::CampaignStart;

fn main() -> Result<(), Box<dyn Error>> {
    let seed = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0x55aa);
    let years = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);

    let mut history = HistoryEngine::seeded_town(seed)?;
    history.simulate_years(years)?;
    let start = CampaignStart::for_outsider(history.world(), history.primary_site())?;

    println!("{}", start.briefing.rendered_text);
    println!("What you might do first");
    for hook in &start.hooks {
        println!("- {}: {}", hook.title, hook.description);
    }

    println!("\nJournal");
    for entry in &start.journal.entries {
        println!("{}\n{}", entry.title, entry.body);
    }

    Ok(())
}
