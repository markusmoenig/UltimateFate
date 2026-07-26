# Ultimate Fate

> **Ultima meets Dwarf Fortress.**  
> A classic fantasy RPG set in a world with a real past, a life of its own, and
> an uncertain future.

*Ultimate Fate* combines the freedom and adventure of *Ultima IV–VII* with the
generated history, depth, and unexpected causality of *Dwarf Fortress*.

Explore a large world of towns, wilderness, ruins, and dungeons. Speak with
people who have homes, work, families, loyalties, and grudges. Fight with swords,
bows, and magic. Discover lost artifacts and learn spells whose reagents and
rituals are different in every campaign.

Behind the familiar RPG lies a living simulation. Wars destroy settlements.
Shortages change laws and prices. Refugees move along real roads. Factions build,
trade, fight, and compete for resources. Ruins, dungeons, and legendary objects
exist because something happened—not because a random-content table placed them
there.

## The Greater Struggle

Every world is caught in an epic struggle between the Free Realms and a generated
Dark Power.

The enemy may be a demonic host, a devouring cult, a tyrannical dominion, or an
ancient power rising beneath the world. It seeks advantage through conquest,
war, economics, politics, corruption, forbidden magic, and spiritual influence.
The forces opposing it are imperfect, divided, and sometimes unjust—but the
larger threat is real.

This is good against evil without making every person or people morally simple.
Orcs, humans, and other cultures contain families, workers, believers, heroes,
criminals, and rival factions. Mortal conflicts have historical and political
causes. The Dark Power can exploit those divisions, while the player's actions
can heal them—or make them worse.

A recovered artifact, reopened trade road, murdered leader, failed harvest, or
changed law may shift the wider struggle. Local adventures matter because they
become part of the fate of the world.

## A World That Remembers

Each campaign generates its own:

- Geography, rivers, seas, mountains, roads, and settlements
- Families, rulers, factions, laws, beliefs, and political disputes
- Wars, raids, migrations, shortages, construction, and destruction
- Artifacts, ruins, graves, records, legends, and disputed history
- Magical formulas, reagents, traditions, and forbidden knowledge

Events have structured causes and consequences. People know only what they
witnessed, learned, or were taught to believe. Official history can be wrong, and
physical evidence may tell a different story.

Your deeds enter that same history. The world does not freeze when the opening
story begins, and it does not exist solely for the player.

## Problems Instead of Checklists

Situations emerge from world state rather than isolated quest scripts.

If someone needs medicine, the medicine is a real object with an owner and a
physical custodian. A law may restrict it. A third person may have enough
influence to help. You could persuade, purchase, steal, find another treatment,
change the law, or walk away and let events continue without you.

The situation resolves because the world changed—not because an invisible quest
flag was checked.

## Classic RPG Adventure

The simulation supports the things that make classic role-playing games fun:

- Open exploration across settlements, wilderness, ruins, and deep dungeons
- Conversations, investigation, secrets, rumors, and historical discovery
- Inventory, equipment, ownership, trade, theft, and useful ordinary objects
- Melee weapons, ranged combat, armor, creatures, spells, and progression
- Distinct factions, companions, enemies, teachers, shops, temples, and guilds
- Multiple solutions with persistent social, legal, and material consequences

Depth should enrich the adventure rather than bury it beneath reports. Immediate
danger and the current lead remain visible; history, regional strategy,
relationships, inventory, and magical knowledge are available on demand.

## Current State

*Ultimate Fate* is in active development. The current playable foundation
includes:

- A deterministic 256×256 region with physical geography, settlements, and roads
- Twenty years of generated history before the player arrives
- A detailed town with named residents and daily schedules
- Factions, laws, construction, trade, migration, caravans, patrols, and raiders
- A history-born multi-level dungeon and persistent artifact
- Melee and ranged combat, inventory, progression, defeat, and healer recovery
- Per-world magic with discoverable formulas and physical reagents
- Regional situations and a systemic multi-solution medicine scenario
- Structured history, provenance, journals, deterministic saves, and replay
- Desktop, headless Game Lab, and Apple/Xcode Metal hosts

The visuals and amount of content are still provisional. Development is currently
focused on making the RPG rules and living-world simulation work together before
finalizing the art direction.

See [concept.md](concept.md) for the full vision and [todo.md](todo.md) for the
development roadmap.

## Run the Development Build

Rust 1.96 or newer is currently required.

```sh
cargo run -p ultimate-fate-desktop
```

- **Move:** WASD or arrow keys
- **Interact / confirm / attack:** E, Enter, or Space
- **Back / cancel:** Escape or Q
- **Inspect:** X or L
- **Journal:** J
- **Inventory / menu:** P

## Game Lab

Run and inspect the same simulation without opening a window:

```sh
cargo run -p ultimate-fate-lab --offline
```

Commands include `observe 12`, `world`, `objectives`, `explore 500`,
`aid consent`, `aid purchase`, `aid theft`, `aid alternative`, and `slice 2`.

## Development

```sh
cargo test --workspace --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
```

The Apple host uses an Xcode-owned `CAMetalLayer` through
[`ultimate_fate.h`](crates/client_apple/include/ultimate_fate.h).

## License

*Ultimate Fate* is licensed under the
[Mozilla Public License 2.0](LICENSE).
