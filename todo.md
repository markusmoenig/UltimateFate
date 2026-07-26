# Ultimate Fate — Development Roadmap

This roadmap begins from the current systemic foundation. The next work should
make the game broader, more legible, and more fun without creating special-case
quests or platform-owned game logic.

## Architectural Rules

Every new feature must follow these rules:

- `CampaignSession` remains the only campaign mutation boundary.
- Desktop, Apple, and Game Lab submit the same semantic commands.
- Renderers display semantic state; they never decide simulation outcomes.
- Situations observe authoritative state rather than owning completion flags.
- Important changes record structured causes, consequences, and participants.
- Generated prose presents facts but never creates facts.
- Distant activity uses aggregate simulation; nearby activity may materialize.
- One authoritative world clock drives light, schedules, seasons, and timed rules.
- New systems must be deterministic and replayable from a campaign save.
- UI shows urgent context first and exposes detail on demand.

## Priority 1 — One Fully Systemic Three-Party Situation

Status: **first playable vertical slice implemented**.

The current generated aid situation now provides:

- A generated patient, healer/custodian, advocate, causal crisis, active-law
  restriction, named medicine, and price.
- Physical custody and legal ownership that remain distinct through gifts,
  purchases, and theft.
- Supported appeal, purchase, witnessed theft, and alternative-treatment routes.
- Contextual conversation actions and a compact tracked lead in the desktop UI.
- Resolution observed from effective treatment in the patient's real inventory.
- A structured, causal historical aftermath recorded exactly once.
- Deterministic generation across audited seeds and exact save/replay reconstruction.
- Game Lab automation for all four routes through
  `aid <consent|purchase|theft|alternative>`.

Remaining extensions for this priority:

- Let non-player actors independently resolve, worsen, or exploit the situation.
- Add law-change, relocation, betrayal, and deliberate refusal/neglect outcomes.
- Generalize aid access into reusable commitments for other material needs.
- Connect payments to household and shop accounts when the full economy lands.

Build one situation comparable to:

> A displaced person needs medicine. A healer controls the medicine. A law or
> faction prevents access. A third party possesses information or leverage.

The exact people, factions, law, resource, and historical cause must come from
the generated world.

Required player approaches:

- Obtain and give the required item legitimately.
- Steal it and accept legal or social consequences.
- Persuade, threaten, bribe, or otherwise influence its custodian.
- Change or circumvent the relevant law.
- Find an alternative material solution.
- Help the affected person relocate.
- Betray or ignore one of the interested parties.

Acceptance criteria:

- At least three generated actors have incompatible needs.
- No approach directly sets a quest-complete flag.
- The situation resolves because material and social state changed.
- Ownership, witnesses, law, faction standing, and resource custody matter.
- The world may resolve or worsen the situation without the player.
- Resolution becomes history and produces a persistent aftermath.
- Game Lab can complete at least three materially distinct approaches.

## Priority 2 — Ordinary Ultima-Like World Interaction

### Objects and Containers

- Add semantic item categories for food, drink, clothing, armor, shields, tools,
  keys, books, records, valuables, currency, and mundane household objects.
- Add containers with capacity, contents, ownership, locks, and keys.
- Allow taking, placing, giving, dropping, equipping, reading, eating, drinking,
  and using objects through semantic commands.
- Preserve legal ownership separately from physical custody.
- Record theft only when applicable witnesses or later evidence can establish it.

### Doors, Buildings, and Access

- Add doors, open/closed state, locks, keys, breakage, and legal access.
- Give homes, shops, workplaces, temples, and civic buildings owners and hours.
- Add trespassing and restricted areas.
- Let NPC schedules operate doors rather than walking through abstract walls.

### Trade and Economy

- Add player wealth and ordinary currency.
- Give shops real inventories, prices, restocking needs, and suppliers.
- Make shortages, blocked roads, laws, and faction relations affect availability
  and prices.
- Support buying, selling, bartering, gifting, debt, and stolen-goods reactions.
- Ensure stock moves through the same resource and party systems as regional trade.

Acceptance criteria:

- A shop cannot sell an item it does not possess.
- Destroyed or blocked supply changes visible stock.
- An NPC can recognize its own stolen property.
- A player can solve a material problem without using a quest-specific action.

## Priority 3 — Dynamic Time, Seasons, and Ambient Conditions

Replace the current coarse local schedule tick with an authoritative player-scale
calendar that coexists with the monthly strategic simulation.

### World Clock

- Track year, season, month, day, and time of day in authoritative campaign state.
- Define explicit conversion rules between player turns, idle heartbeats, rest,
  travel, and strategic months; ordinary walking must not consume implausible years.
- Expose semantic phases such as dawn, day, dusk, night, and deep night while
  retaining enough precision for appointments, shop hours, patrol changes, and spells.
- Pause the clock when gameplay is paused or a modal view requires it.
- Save, load, replay, and Game Lab must reproduce the clock exactly.

### Daylight and Seasons

- Derive sunrise, sunset, ambient light, and night length from season and region.
- Let seasons affect temperature, weather likelihood, harvests, river state,
  travel difficulty, food demand, clothing needs, and creature activity.
- Make darkness affect sight, stealth, ranged accuracy, navigation, crime,
  artificial-light use, and encounter danger.
- Keep simulation rules independent from rendering; renderers receive semantic
  ambient conditions and choose how to depict darkness, shadows, lamps, and weather.

### NPC Schedules

- Give residents daily and weekly schedules for sleep, work, meals, worship,
  leisure, markets, patrols, and travel.
- Let schedules consult doors, building hours, local laws, weather, daylight,
  danger, occupation, household responsibilities, and cultural customs.
- Allow needs and urgent goals to override routine: a healer answers an emergency,
  a frightened resident stays home, and a thief may prefer darkness.
- Transition materialized residents and aggregate settlement activity through the
  same schedule rules without double-counting work or consumption.

### Dramatic and Historical Effects

- Represent eclipses, supernatural darkness, blood moons, ash clouds, magical
  storms, unusually long winters, festivals, curfews, sieges, and blackouts as
  bounded world conditions with a cause, scope, duration, and mechanical effects.
- Allow historical events and strategic systems to create, prolong, mitigate, or
  end these conditions; narrative text may describe them but cannot switch them on.
- Let NPC behavior, factions, trade, agriculture, magic, and creatures react to
  the condition rather than treating it as a renderer-only story filter.
- Record exceptional conditions and their consequences in history without filling
  the journal with routine sunrises and sunsets.

Acceptance criteria:

- A player can observe shops opening, workers changing activity, lamps appearing,
  and residents returning home across one day.
- Winter nights are meaningfully longer than summer nights and affect behavior.
- Waiting until a stated appointment reaches the same state in desktop and Game Lab.
- A generated eclipse or unnatural night changes both visuals and simulation, has
  an inspectable cause, ends according to world state, and survives save/load.
- A thirty-day unattended simulation remains deterministic and does not produce
  notification or journal spam from routine clock transitions.

## Priority 4 — Deeper NPC Life and Agency

Extend resident agents beyond hunger, fatigue, fear, and isolation.

### Persistent Personal State

- Household membership and shared resources.
- Wealth, employment, skills, health, injuries, and legal status.
- Friendships, family ties, rivalries, debts, mentorships, and obligations.
- Memories of witnessed player behavior.
- Local trust and reputation rather than one global morality score.

### Goal Planning

- Score actions from needs, drives, relationships, ideology, risk, and travel cost.
- Allow multi-step plans such as acquiring supplies before beginning work.
- Let NPCs request help when blocked, but also seek non-player solutions.
- Give occupations real inputs and outputs at settlement resolution.
- Promote important failures or successes into historical events.

### Material Consequences

- Residents consume food and medicine at a scale consistent with aggregate rates.
- Workers contribute to shops, projects, services, and infrastructure.
- Injury, arrest, displacement, unemployment, or death changes households and jobs.
- Materialized and aggregate representations must conserve equivalent resources.

Acceptance criteria:

- NPCs continue pursuing goals while the player waits.
- Two NPCs with different drives react differently to the same pressure.
- Removing a worker, supplier, or household resource creates understandable effects.
- Save/load reproduces agent decisions exactly.

## Priority 5 — Player-Facing Situations and Commitments

- Replace the remaining fixed dungeon quest presentation with a projected
  situation/commitment contract.
- Add commitment forms for recovery, protection, escort, investigation,
  negotiation, relief, sabotage, construction, and historical research.
- Allow the player to accept, decline, abandon, renegotiate, or fulfill commitments.
- Track sponsors, beneficiaries, opposition, cause, target, promised reward, and
  actual outcome.
- Close stale commitments automatically when the world changes.
- Keep resolved commitments in the journal rather than the active UI.

Acceptance criteria:

- A contract never remains active after its underlying problem disappears.
- Multiple contracts can refer to the same world problem from different factions.
- The UI shows one tracked lead and a small urgent list, not the whole history.

## Priority 6 — Progression That Reflects How the Player Acts

### Capability and Practice

- Define thresholds and benefits for martial, ranged, magical, social, and
  exploration practice.
- Prevent harmless repetition from becoming the optimal training method.
- Add teachers, training costs, equipment requirements, and practice limits.
- Keep level gains modest relative to equipment, knowledge, access, and standing.

### Knowledge and Access

- Track languages, maps, historical facts, formula fragments, material properties,
  and cultural knowledge.
- Add guild, temple, library, workshop, court, and faction access.
- Require appropriate knowledge or social access for advanced actions.

### Standing and World Change

- Expand faction standing into person-, settlement-, and culture-specific memory.
- Track obligations, titles, citizenship, criminal status, and recognized custody.
- Reward major world changes without turning them into generic grindable XP.

Acceptance criteria:

- Combat practice cannot teach magic or create political trust.
- Knowledge and standing unlock non-combat solutions.
- Stolen custody is not treated as legitimate ownership.
- Progression sources are visible and explainable to the player.

## Priority 7 — Per-World Magic as a Game

### Formula Breadth

Add recognizable effect families:

- Cure
- Light
- Sleep
- Unlock
- Protection
- Dispel
- Teleportation
- Resurrection

Each effect remains authored and legible, while its reagents, preparation,
condition, risks, surviving records, and cultural interpretation vary by world.

### Discovery

- Add partial inscriptions, rumors, teachers, laboratory notes, false theories,
  and oral traditions.
- Let experiments choose reagents, tools, preparation, target, and environment.
- Consume resources and produce informative evidence on failed experiments.
- Distinguish rumor, hypothesis, tested result, and confirmed formula.
- Allow NPC researchers and factions to discover or suppress magical knowledge.

### Consequences

- Add magical laws, licenses, forbidden materials, witnesses, and faction reactions.
- Make reagents part of trade, geography, ownership, and scarcity.
- Let magical discoveries change strategic objectives and artifact claims.

Acceptance criteria:

- Every generated formula is discoverable through at least two information paths.
- A player can infer a formula without reading the complete answer.
- Experimentation never consults UI-only state.
- The same seed always produces identical magical truth and experiment outcomes.

## Priority 8 — Combat and Equipment Depth

### Melee

- Weapon reach, speed, damage type, stamina, wounds, armor, shields, morale,
  surrender, and nonlethal outcomes.
- Conventional weapon families with generated material, quality, maker, and history.

### Ranged

- Crossbows, thrown weapons, recoverable ammunition, cover, obstruction, and
  projectile interaction with terrain and objects.

### Creatures and Groups

- Faction-aware hostility rather than universal player hostility.
- Patrol, flee, guard, ambush, hunt, surrender, and reinforcement goals.
- Injury, death, loot, witnesses, and property damage enter social history.

Acceptance criteria:

- Enemies pursue understandable goals beyond walking toward the player.
- Combat outcomes affect factions, households, custody, and the regional world.
- Equipment provenance can matter socially as well as statistically.

## Priority 9 — World Breadth and Historical Depth

- Generate multiple detailed settlements from regional roles and history.
- Materialize wilderness sites, farms, shrines, camps, ruins, caves, mines, and forts.
- Generate multiple dungeons from distinct causal events.
- Add settlement founding, conquest, occupation, collapse, rebuilding, and ownership
  changes that alter semantic maps.
- Expand families, ancestry, succession, migration, cultural blending, and disputed
  citizenship.
- Let artifacts, laws, beliefs, and grievances move between settlements.
- Add strategic magical and spiritual activity alongside economy and military action.

Acceptance criteria:

- A player can travel through a large world without every site feeling like Rathmere.
- Geography materially affects trade, war, settlement roles, and access.
- Every major ruin, dungeon, law, and recognized artifact has a causal history.
- Distant changes materialize consistently when the player arrives.

## Priority 10 — UI and Platform Completion

### Progressive Disclosure

- Keep status, immediate threat, tracked lead, and recent messages visible.
- Put inventory, character progression, magic notebook, relationships, contracts,
  regional strategy, and history in separate on-demand views.
- Show current platform bindings rather than abstract action names.
- Avoid scroll-dependent gameplay on Apple TV or touch-only devices.
- Present time and ambient conditions compactly; do not turn ordinary clock
  transitions into messages or journal entries.

### Contextual Interaction

- Build a context action menu from available semantic commands.
- Add player-facing flows for giving, taking, trading, experimenting, targeting,
  conversation topics, and commitment choices.
- Support keyboard, mouse, controller, touch, and focus-based Apple TV navigation.

### Saves

- Let each platform select an appropriate save location.
- Add campaign slots, autosave policy, manual save/load, metadata, and corruption
  recovery around the existing versioned save representation.
- Add migration tests before changing the save format.

Acceptance criteria:

- Core gameplay is possible without a full keyboard.
- No required information is clipped or dependent on awkward scrolling.
- A save created by one host reconstructs the same campaign state in another host.

## Priority 11 — Continuous Playtest and Balance

Extend Game Lab metrics to track:

- Time between meaningful decisions.
- Situation creation, worsening, world resolution, and player resolution.
- NPC goal diversity and failed plans.
- Shop availability and resource conservation.
- Theft, witnesses, standing changes, and legal consequences.
- Discovery rate for historical facts and magical formulas.
- Combat lethality, recovery, ammunition use, and progression sources.
- Active-objective count, journal growth, and notification volume.
- Strategic actor objectives and material impact.
- Time spent in each day phase, schedule compliance and overrides, artificial-light
  use, and clock/calendar pacing.

Maintain deterministic seed suites for:

- Peaceful and prosperous regions.
- Severe shortages.
- Long route disruptions.
- Dark Power strategic advantage.
- Strong defending coalition.
- Conflicting laws and faction beliefs.
- Scarce magical reagents.
- Multiple valid non-combat solutions.

Every major milestone should include:

1. Unit tests for rules and invariants.
2. Multi-seed structural validation.
3. Deterministic replay and save/load comparison.
4. Headless semantic playthroughs.
5. A live desktop observation pass.
6. A UI overload and journal-volume audit.

## Recommended Next Milestone

Implement the three-party medicine/access situation first, together with the
minimum ordinary-world features it requires:

1. A medicine item with legal owner and physical custodian.
2. A generated patient, custodian, and third interested party.
3. A law or faction rule controlling access.
4. Give, steal, persuade, purchase, and alternative-treatment routes.
5. Witness and standing consequences.
6. State-observed resolution and historical aftermath.
7. A compact player-facing situation view.
8. Three deterministic Game Lab solutions.

This milestone exercises NPC agency, ownership, law, trade, conversation,
progression, situations, history, UI, and save replay through one connected piece
of play.
