# Ultimate Fate

## Game Concept and Technical Direction

**Status:** Working concept  
**Working title:** *Ultimate Fate*  
**Tagline:** *Every world has a past. Every deed becomes its history.*

## Elevator Pitch

*Ultimate Fate* is a top-down, tile-based fantasy role-playing game combining the dense, socially coherent world of *Ultima IV–VII* with the generated history and persistent causality of *Dwarf Fortress*.

Each campaign creates a new world with its own rulers, families, wars, migrations, ruined places, political tensions, religious disputes, artifacts, secrets, current crisis, and internally coherent magical tradition. The underlying fantasy remains recognizable: swords are swords, magical effects such as healing, fire, sleep, protection, and travel remain understandable, people have homes and occupations, and towns have ordinary economic and political needs. The exact formula that produces a magical effect is something the player must discover anew in each world.

The novelty does not come from generating bizarre new laws of physics. It comes from discovering:

> What happened to this world before the player arrived, why is it in its current state, and what will happen now that the player is here?

The game uses a stable, handcrafted Ultima-like ruleset inhabited by a procedurally generated Dwarf Fortress-style history.

## Core Inspirations

### Ultima IV

- The player investigates what it means to live according to moral principles.
- Morality is demonstrated through behavior rather than a simple good/evil choice.
- Progress involves understanding the world, not only accumulating power.

### Ultima V

- Virtues and laws can be corrupted into instruments of oppression.
- Antagonistic institutions may have understandable motives.
- Lawful behavior and moral behavior are not necessarily the same.

### Ultima VI

- The apparent enemy may have a legitimate and comprehensible perspective.
- Historical misunderstanding can drive the central conflict.
- Discovery can radically change the player's understanding of the campaign.

### Ultima VII

- NPCs have homes, jobs, schedules, relationships, and material needs.
- Ordinary objects have understandable uses.
- Investigation emerges through conversations, documents, observation, and physical interaction.
- The world feels like a functioning place rather than a collection of levels.

### Dwarf Fortress

- People, settlements, factions, and artifacts have persistent histories.
- Events have long-term material and social consequences.
- Generated content is connected through causal chains.
- The world can produce situations that were not explicitly scripted.

## Design Principle

The game should simulate circumstances and consequences rather than directly generate stories.

```text
Randomness selects conditions.
Simulation determines consequences.
The player discovers and changes the resulting world.
```

The central procedural object is not the dungeon or world map. It is a **society under pressure**.

## Gameplay Breadth

Investigation, conversation, and historical discovery are major ways to understand
the world, but they are not the entire game. *Ultimate Fate* is a full Ultima-style
role-playing game whose activities include:

- Wilderness, settlement, ruin, and dungeon exploration
- Inventory management, equipment, containers, ownership, trade, and theft
- Melee combat with weapons, armor, reach, wounds, fatigue, morale, and surrender
- Ranged combat with bows, crossbows, thrown weapons, ammunition, cover, and line of sight
- Spell discovery, preparation, experimentation, and casting
- Companions, creatures, hostile encounters, shops, loot, and crafted objects
- Political, legal, economic, and historical consequences for player actions

The generated history should give these familiar activities context. Wars leave
ruins and veterans, shortages alter shops, outlawed magic changes access to
reagents, crafted weapons retain their provenance, and factions remember theft,
violence, aid, and destruction.

## Tone, Moral Complexity, and Mature History

The tonal rule is:

> **Brutal history, morally complicated people, restrained presentation.**

The world should be darker and more politically human than a mythic struggle
between inherently good and evil peoples. Its darkness comes from understandable
people making consequential choices under pressure, and from societies remembering,
concealing, justifying, and reacting to those choices over generations.

### No Inherently Evil Peoples

No ancestry, species, or culture is biologically destined to be evil. Humans,
orcs, and other peoples must all be capable of:

- Families, friendship, love, loyalty, and ordinary work
- Law, faith, art, humor, ritual, and political disagreement
- Compassion, cruelty, ambition, fear, and self-deception
- Internal factions with incompatible interests
- Conflicting interpretations of their own history

An orc raiding force is not evidence that all orcs are monsters. It is a political
and military group acting for particular reasons. Orc settlements should be as
socially legible and internally varied as human settlements.

### War, Raiding, and Mixed Ancestry

Wars, raids, occupations, migrations, and collapsed states may produce mature and
uncomfortable historical consequences:

- Death, captivity, displacement, famine, and destroyed livelihoods
- Occupation, collaboration, resistance, and cultural assimilation
- Consensual relationships across cultural lines
- Political marriages and relationships formed after migration or occupation
- Coerced parentage, represented only with restraint
- Children and later families of mixed ancestry
- Disputed parentage, legitimacy, inheritance, citizenship, and social status
- Prejudice, concealed ancestry, divided loyalties, and cultural blending

Mixed ancestry must never function as shorthand for violence or moral corruption.
It may originate in love, diplomacy, migration, shared communities, occupation, or
coercion. Characters of mixed ancestry are complete people with ordinary needs,
relationships, beliefs, ambitions, and agency. They are not merely consequences,
symbols, or clues produced by an atrocity.

The simulation should preserve enough structured context to explain ancestry and
its social consequences without generating explicit scenes. A raid may be part of
a person's family history, but that person must enter the world model through the
same identity, relationship, belief, and life systems as everyone else.

### Restraint and Content Boundaries

Sexual violence and harm to children may exist as implied historical atrocities
when they are causally important, but they are not procedural spectacle.

- Do not generate graphic descriptions.
- Do not turn sexual violence into a tactical action, reward, joke, or routine
  source of random flavor.
- Do not use children primarily to make an event feel more shocking.
- Do not make atrocity ubiquitous merely to establish a mature tone.
- Focus on survivors, descendants, institutions, beliefs, and long-term effects.
- Allow players to control the severity of sensitive historical content.

Content settings should govern both presentation and which sensitive event
templates may be generated. The least severe setting should preserve political and
historical coherence using displacement, adoption, disputed records, occupation,
and other non-explicit causes.

Darkness is meaningful when consequences endure. A rare atrocity that changes
families, laws, borders, beliefs, and relationships for generations is more
powerful than constant cruelty without social memory.

## Stable Foundation and Per-Campaign Variation

Approximately 80–90 percent of the game's conceptual vocabulary should remain stable. The remaining portion is rearranged through world generation and historical simulation.

### Stable Across Campaigns

- Controls and interface
- Combat and interaction rules
- Recognizable medieval-fantasy weapons
- Broad magical effect vocabulary and the method of magical experimentation
- Materials such as wood, stone, iron, steel, silver, and cloth
- Established creature and cultural archetypes
- Professions and ordinary economic activities
- The action vocabulary used by players and NPCs
- The method by which clues, ownership, laws, and relationships are communicated
- The moral themes explored by the game

### Generated Per Campaign

- Geography and settlement placement
- Families and ancestry
- Rulers and successions
- Wars, rebellions, disasters, and migrations
- Political alliances and grudges
- Local laws and customs
- Religious and cultural interpretations
- Trade routes and resource pressures
- The condition of settlements and buildings
- Artifact makers, owners, and provenance
- The world's hidden magical principles and valid formulas
- Spell reagents, preparations, conditions, side effects, and incompatibilities
- Magical traditions, discoveries, prohibitions, and mistaken theories
- Which magical knowledge survived or was lost
- Current crises and misunderstood historical events
- Secrets, rumors, and incorrect beliefs

The player should feel:

> I understand how to learn a world, but I do not yet understand this world.

## World Generation Pipeline

```text
Campaign seed
    ↓
Geography and resources
    ↓
Peoples, settlements, and factions
    ↓
Historical simulation
    ↓
Ruins, artifacts, borders, laws, and grievances
    ↓
Current political and material crisis
    ↓
Playable world
    ↓
Continuing simulation affected by the player
```

### Geography and Resources

Generate a restrained, recognizable fantasy environment:

- Elevation, rivers, coastlines, forests, and farmland
- Mineral and agricultural resources
- Roads, crossings, ports, and trade constraints
- Suitable settlement locations
- Dangerous or difficult regions

Geography should influence later history. Mountain settlements may be difficult to conquer, river towns may become trading centers, and resource scarcity may create dependency or conflict.

The first implemented atlas uses 256×256 semantic cells. It deterministically
generates elevation, temperature, moisture, ocean-connected seas, inland lakes,
river courses, coastlines, climate biomes, mountain barriers, and connected
landmasses. Settlement roles select physically suitable sites, while roads use
terrain-cost paths that can bridge rivers but cannot silently cross seas or
impassable mountains. Geography, settlement positions, and full route paths are
owned by historical world state; the renderer only projects them.

The implemented historical pipeline now creates that geography and its regional
settlements before prehistory. Each simulated year resolves twelve modular
regional economy, trade, party, conflict, goal, migration, and strategy cycles,
then resolves the annual local harvest, demography, and rumor cycle. Conflict and
migration operate at historically meaningful cadences rather than producing
monthly notification spam. Past road attacks leave inspectable semantic raid
sites whose journal records point back to their authoritative events. Living
parties traverse routes by atlas distance and are replenished through a modular
living-world pulse.

### Peoples, Settlements, and Factions

Generated groups should have:

- A homeland or current territory
- Material needs
- Cultural values
- Political structure
- Relationships with neighboring groups
- Historical memories
- Internal disagreements
- Recognizable architecture, clothing, and equipment drawn from authored content

No ordinary mortal faction should be assigned "evil" as its primary explanation.
Conflict among peoples should emerge from needs, incompatible beliefs, fear,
ambition, historical grievance, corruption, or distorted ideals.

### The Grand Struggle

The campaign has a larger Ultima-like purpose beyond its local situations. The
Free Realms contend with a generated Dark Power whose aim is genuinely
destructive: domination, spiritual corruption, demonic incursion, the extinction
of freedom, or another authored form of metaphysical evil.

This does not make mortal ancestry a moral alignment. The Dark Power may recruit,
deceive, coerce, arm, or corrupt human kingdoms, orc hosts, cults, mercenaries,
monsters, and institutions. Orc armies can be an iconic part of its military
threat while individual orcs and entire clans can resist, defect, remain neutral,
or join the Free Realms. The same is true of human states. Demons may be
intrinsically aligned with the Dark Power; mortal peoples retain agency.

The struggle is simulated across several strategic fronts:

- Territory and access to sites, roads, passes, depths, and resources
- Military power, fortification, recruitment, readiness, and casualties
- Economy, production, trade, supply, famine, and reconstruction
- Political legitimacy, alliances, law, unrest, and faction cohesion
- Spiritual hope, fear, corruption, worship, and resistance
- Magical knowledge, artifacts, sites of power, wards, and dangerous discoveries

Local events feed these fronts. Repairing a granary may stabilize a region's food
supply and deny the enemy recruits. Losing a watchhouse may expose a trade route.
Recovering an old seal may establish lawful ownership, restore an alliance, or
unlock a ward. The strategic state in turn creates local pressures, migrations,
raids, opportunities, quests, shortages, and dungeon occupants.

The player should never receive one disconnected "save the world" quest. They
enter a living campaign containing many simultaneous situations and choose which
fronts, settlements, people, and principles to support. The larger objective
provides direction; the generated causal world provides the actual play.

The implemented strategic layer represents the Free Realms and the generated
Dark Power as persistent actors rather than deriving an unexplained score from
annual statistics. Each actor has capacity, reserves, influence, a preferred
front, a current objective, a causal target, progress, and a last historical
event. Their opposed initiatives can materially restore or disrupt a route,
relieve or exploit a settlement, and consolidate influence. Strategic-front
values remain useful summaries, but they are consequences of those recorded
actions rather than the authority that invents them.

### Historical Simulation

Historical world generation should run at low resolution over decades or centuries.

During each historical interval:

1. Settlements produce and consume resources.
2. Populations grow, decline, and migrate.
3. People age, form families, inherit positions, and die.
4. Factions evaluate threats and opportunities.
5. Trade, cultural beliefs, and technologies spread.
6. Wars, disasters, reforms, and political transitions occur.
7. Important events are stored as structured records.
8. Consequences modify the physical and social world.

The generator should not simulate every meal or sword strike. Large historical events can be resolved statistically while preserving participants, causes, and consequences.

## Structured History

Important events must be data, not only generated prose.

```rust
struct HistoricalEvent {
    id: EventId,
    date: WorldDate,
    location: PlaceId,
    event_type: HistoricalEventType,
    participants: Vec<EntityId>,
    causes: Vec<EventId>,
    consequences: Vec<Consequence>,
    witnesses: Vec<EntityId>,
}
```

Example:

```text
Event: Northern rebellion defeated
Year: 183
Participants: Crown, Baron Ardin, Northern League
Consequences:
    - King killed during final siege
    - Young queen inherits the throne
    - Regent appointed
    - Rebel estates confiscated
    - Refugees establish district outside the capital
    - Northern fortress abandoned
    - Royal sword lost
```

In the playable era, that event can explain:

- A former rebel running an inn
- A guard captain whose father died in the siege
- A poor refugee district
- A noble family enriched by confiscated property
- Bandits descended from displaced soldiers
- A ruined fortress containing contradictory military records
- A queen who privately doubts the official account
- A named sword claimed by several groups

All of these elements should originate from the same causal chain.

## Truth, Knowledge, and Belief

The simulation must distinguish objective truth from what characters believe.

```text
Truth:
    The king died from poisoned wine during the siege.

Beliefs:
    The court believes a rebel assassin killed him.
    Northern rebels believe the regent arranged his death.
    A priest believes the king was punished for sacrilege.
    One witness knows the cup was switched accidentally.
```

Each NPC should track:

- Facts they personally witnessed
- Information they were told
- The source of that information
- Trust in the source
- Confidence in the belief
- Whether they are willing to reveal it
- Personal interpretation of the event

Information should spread imperfectly. Reputation should be local and social rather than a single global score.

## NPC Model

Important NPCs require:

- Identity and ancestry
- Home and workplace
- Daily and weekly schedule
- Occupation and skills
- Family and social relationships
- Material needs
- Political, cultural, and religious affiliations
- Personal goals
- Known facts and beliefs
- Loyalties, debts, fears, and grievances
- Memories of player behavior

NPC behavior should be goal-driven rather than entirely scripted.

```text
score(action) =
    expected_food_gain × hunger
  + expected_safety_gain × fear
  + expected_status_gain × ambition
  + loyalty_effect
  + ideological_effect
  - risk
  - travel_cost
```

Useful drives include:

- Survival
- Wealth
- Status
- Revenge
- Curiosity
- Faith
- Family
- Loyalty
- Freedom
- Justice
- Ideology

Relationships such as parenthood, friendship, rivalry, debt, mentorship, and political obligation are major story-generating systems.

The current vertical slice materializes generated residents as authoritative
simulation entities rather than fixed renderer markers. Each resident has a
distinct work and home position and a deterministic 240-turn routine, but the
routine is only one input to behavior. Persistent hunger, fatigue, fear, and
isolation compete through goals scored from the resident's generated drives and
the current shortage, unrest, and nearby threat. A resident may therefore leave
work to seek food, safety, rest, or company, path to the relevant semantic
place, and change their needs according to whether the action succeeds.
Conversation, inspection, desktop, Apple, and Game Lab read the authoritative
entity and agent state. Household inventories, relationships, and jobs that
transfer named resources remain later extensions of this same model.

The three founding institutions retain recognizable civic roles, but their
public doctrines are generated per campaign and recorded in the foundation
event. The governing institution's doctrine and the material severity of a later
shortage determine its response. That response in turn selects the active law,
construction priority, significant item, dungeon identity, faction dispute, and
available resolution. Different seeds therefore vary through one causal chain
rather than by reskinning an unrelated fixed quest.

## Problems Instead of Fixed Quests

NPCs should possess problems, goals, and information rather than only fixed quest scripts.

Example:

```text
Mara needs medicine for her son.
The healers possess medicine.
The healers treat only registered citizens.
Mara's citizenship was revoked after her husband rebelled.
Mara knows where the remaining rebels are hiding.
```

Possible player responses include:

- Buy or steal the medicine
- Forge citizenship records
- Persuade or threaten a healer
- Find an alternative treatment
- Betray the rebels
- Change the law
- Help Mara leave the city
- Ignore the situation

The situation ends when the social and material state reaches a new condition, not necessarily when a predefined objective is checked off.

Good generated situations should generally involve at least three interested parties. This creates a network of competing needs rather than a binary choice.

### Quests as a Player-Facing Contract

The game still needs readable Ultima-like quests. A quest is the interface that
tells the player what commitment they have made; it is not the source of the
underlying situation.

```text
Historical event
    ↓
Material consequence: ruin, sealed archive, displaced keeper, artifact
    ↓
Present need held by a generated person or faction
    ↓
Quest contract: recover, protect, investigate, escort, destroy, negotiate
    ↓
Systemic resolution
    ↓
New historical event and persistent aftermath
```

Quest titles, locations, participants, opposition, objects, and motives should be
bound to generated world data. Objective forms may be authored and stable, but a
quest must not merely substitute campaign names into an unrelated fixed plot.
When possible, the same situation should admit several resolutions and record
which one actually occurred.

Regional simulation may promote a need into a persistent faction-sponsored
contract. A blocked road can create a security contract; a real reserve shortfall
can create a relief contract. The contract retains its causal event, sponsor,
target route or settlement, available approaches, status, and final resolution.
If the world solves the underlying problem before the player acts, the contract
closes rather than remaining as a stale quest.

Contracts are not resolved from an abstract list. Tracking one directs the player
through a regional travel layer to its affected road or settlement, where the
available intervention can be carried out. Regional geography therefore connects
simulation state to play while detailed local maps remain loaded on demand.

Force-based road contracts additionally require physical resolution. The route
cannot be reported clear while its generated raiding party remains active. The
player must find and defeat that authoritative party; its defeat grants normal
combat progression, becomes history, removes the party from the map, and only
then permits the selected intervention to reopen the route.

Regional movement is authoritative simulation state, not renderer animation.
Caravans, refugee columns, patrols, and raiding bands are persistent parties with
named leaders where appropriate, affiliation, route, origin, destination,
progress, cargo or population, purpose, status, and causal history. Resources
leave with a caravan and reach the destination only if it arrives; caravans make
return journeys rather than disappearing. Refugees remain physically in transit,
patrols can establish road camps, and raiders can be encountered and defeated.
The renderer merely projects these parties onto whichever regional view is active.

### Dungeons as Historical Cross-Sections

Dungeons are not detached combat levels. Their existence and contents should be
material consequences of history: buried settlements, wartime tunnels, seized
storehouses, forbidden temples, collapsed mines, sealed archives, tombs, and
fortifications.

Each depth can expose an older layer of the same causal chain. Architecture,
inscriptions, occupants, hazards, loot, and artifacts should answer questions
about who built the place, who later used it, why it was abandoned or sealed, and
why it matters now. Clearing, looting, flooding, opening, or claiming a dungeon
changes present ownership and becomes new history.

The simulation owns x/y/z positions, passability, transitions, occupants, items,
and consequences. A renderer may show one floor as overhead tiles today and the
same data with cutaway isometric geometry later.

Cross-map gates are collision transitions across every input platform. Deliberate
local transitions such as dungeon stairs still use the mapped action button.

### Significant Items as Causal Entities

Important items are authoritative world entities, not names generated for a loot
table. A significant item records:

- Kind, material identity, creation date, and historical origin
- Current location, owner or custodian, and whether it is lost
- A provenance chain of transfers, thefts, discoveries, uses, and restorations
- The strategic front and institutions affected by its possession
- Claims, laws, rituals, formulas, offices, or sites that recognize it

A dungeon quest may materialize a lost item from the historical ledger, but it
does not create the item. Recovering it appends custody and discovery records,
creates a historical event, changes the relevant strategic fronts, and permits
later systems to react. The item may subsequently be stolen, traded, inherited,
used in a ritual, displayed as proof, destroyed, or captured by the Dark Power.

### Continuing Material History

World generation does not stop when play begins. Institutions, households, and
other agents continue proposing and pursuing projects in response to needs.
Construction requires accountable funding, materials, labor, location, and time.
Buildings and infrastructure pass through planned, supplied, foundation,
construction, operating, damaged, abandoned, ruined, repurposed, and repaired
states as appropriate.

Significant phase changes are historical events. Their material consequences alter
the authoritative semantic map rather than swapping decorative background art.
Distant projects may advance statistically; projects near the player expand into
work sites, stored supplies, workers, obstructions, and damage. Both resolutions
must consume and produce equivalent world resources.

Destruction follows the same rule. Fire, warfare, sabotage, neglect, weather, and
resource failure require causes and leave ownership changes, rubble, casualties,
displacement, shortages, memories, physical evidence, and possible reconstruction.
The world should never destroy structures merely to provide visual variety.

### Progression

Levels provide a familiar long-term Ultima structure without replacing social and
historical advancement. Combat, exploration, discoveries, completed commitments,
and major changes to the world can grant experience. Level gains improve a small
stable set of capabilities such as health and attack skill; equipment, learned
magic, reputation, allies, legal standing, and knowledge remain equally important
forms of progression.

Progression should therefore have several causal tracks rather than one generic
power number:

- **Capability:** modest level-based health and combat reliability
- **Practice:** mastery earned by actually using martial, magical, survival, or
  social methods
- **Knowledge:** confirmed history, formula fragments, languages, maps, and
  material properties
- **Access:** teachers, guilds, temples, libraries, legal permissions, workshops,
  and trade networks
- **Standing:** reputation and obligations with particular people, settlements,
  factions, and cultures
- **Material power:** equipment, reagents, wealth, companions, bases, and custody
  of historically recognized objects
- **World change:** roads reopened, institutions reformed, enemies displaced,
  alliances created, and strategic fronts moved

No single track should substitute for the others. Repeatedly killing minor
creatures cannot teach an unknown ritual, manufacture political trust, reveal a
buried language, or make a stolen crown legitimate. Conversely, knowledge and
standing can create non-combat routes through situations that raw level cannot.
Rewards are issued by typed causes—combat victory, discovery, fulfilled
commitment, instruction, or world change—so later balance work can cap or weight
each source without rewriting quests.

The implemented character record now separates martial, ranged, magical,
social, exploration, fulfilled-commitment, discovery, and world-change tracks.
Levels and general experience remain, but only the corresponding action advances
each practice or causal track. Item custody is also distinct from legal
ownership: gifts transfer ownership, while taking an adjacent resident's
property marks the item stolen and changes standing with the resident's faction.

## Moral and Political Generation

The world may use a small authored vocabulary of principles:

- Compassion
- Truth
- Duty
- Freedom
- Sacrifice
- Justice
- Responsibility
- Courage
- Stewardship

Each campaign can generate how cultures and institutions interpret these principles.

Example:

```text
Principle: Stewardship

Druids:
    Natural land should remain untouched.

City government:
    Nature should be controlled for public benefit.

Farmers:
    Land belongs to those who maintain it.

Regent:
    Stewardship justifies compulsory public labor.
```

Dynamic rules should primarily be social and institutional:

- Magic is prohibited inside a kingdom.
- Citizens must observe a curfew.
- A temple refuses healing to excommunicated people.
- Weapons must be surrendered at a city gate.
- A guild controls reagent sales.
- Refugees cannot own property.
- Grave robbing is tolerated in one culture and severely punished in another.
- Dueling is a legal method of resolving accusations.

These rules change gameplay without changing the recognizable nature of the fantasy world.

## Campaign Crisis Structure

A generated campaign should ideally contain:

1. A society that previously functioned.
2. A crisis that disrupted its equilibrium.
3. A ruler or institution responding to the crisis.
4. A response that solves one problem while creating another.
5. An outsider or minority group blamed for the damage.
6. A concealed or misunderstood historical truth.
7. Several possible resolutions with different moral costs.

Example:

```text
A magical winter is destroying the harvest.
The regent introduces compulsory food sharing, saving thousands.
Local governors classify dissenters as hoarders and seize their homes.
An underground people are blamed for disrupting the seasons.
They actually sealed an ancient mechanism recently reopened by humans.
Restarting it would restore summer while flooding three underground settlements.
```

This single premise can generate laws, resistance groups, shortages, dungeons, investigations, political arguments, and a consequential ending.

## Inventory, Equipment, and Combat

Items are semantic simulation objects rather than renderer-specific sprites.
Inventory and equipment should support:

- Nested containers and physical locations
- Weight, size, condition, quality, and durability
- Materials and authored item forms
- Ownership, theft, trade value, legality, and cultural significance
- Equipment slots, handedness, ammunition, and consumable resources
- Makers, previous owners, inscriptions, enchantments, and historical provenance

The same sword may be represented by an overhead icon, an isometric sprite, or a
3D model without changing its simulation definition.

Combat should support three connected forms:

### Melee

- Swords, axes, spears, clubs, shields, unarmed attacks, and improvised weapons
- Reach, facing, positioning, armor, blocking, and weapon properties
- Health, wounds, bleeding, pain, fatigue, morale, incapacitation, and surrender
- Nonlethal force where the weapon and situation permit it

### Ranged

- Bows, crossbows, thrown weapons, and other authored ranged forms
- Ammunition, range, line of sight, cover, obstruction, and projectile travel
- Recoverable or damaged ammunition where useful
- Environmental interaction, including doors, fire, fragile objects, and terrain

### Magic

- Prepared or learned formulas that resolve through the world's generated magical rules
- Targeting, range, area, duration, resistance, and environmental effects
- Reagents, tools, conditions, risks, and social or legal consequences

Combat resolution should remain deterministic from authoritative state plus the
campaign's random streams. The renderer presents attacks and effects but does not
decide their outcome. Violence should also enter the social simulation: witnesses,
ownership, law, faction relations, injury, death, fear, and reputation all retain
their consequences.

The current playable core gives hostile actors authoritative turns after every
time-advancing player command or idle heartbeat. They detect the player within a
bounded radius, path around semantic terrain and living actors, pursue, and
attack. Adjacent groups share one attack initiative per turn during this early
balance phase. Defeat does not leave the client paused or require restarting:
the player is restored beside the nearest materialized healer. Future death
penalties may affect money, time, injuries, equipment, reputation, or world
events, but the recovery destination and survival state remain simulation facts.

## Weapons and Artifacts

Weapons remain conventional and legible:

- Swords
- Axes
- Spears
- Bows and crossbows
- Shields and armor
- Silver or blessed weapons against appropriate creatures
- A limited set of authored magical properties

A generated weapon is assembled from:

```text
Form
+ material
+ quality
+ cultural technique
+ maker
+ ownership history
+ historical significance
+ optional authored enchantment
```

Example:

```text
The Sword of Ardin

Type: Steel longsword
Maker: Royal smith Elian Voss
Original owner: Captain Ardin
History:
    - Issued to the royal guard
    - Stolen during the northern rebellion
    - Carried during the final siege
    - Buried with the rebel commander
Property:
    - Authored bonus against undead
Social meaning:
    - Loyalists consider it stolen royal property
    - Rebels consider it a symbol of liberation
    - The commander's family wants it returned
```

Its uniqueness comes as much from provenance and recognition as from combat statistics.

## Per-World Magic

Magic should combine a stable, legible vocabulary of possible effects with a
hidden ruleset generated once for each campaign. The player can understand what
healing, fire, sleep, protection, unlocking, or teleportation mean without knowing
how this particular world produces them.

Recognizable effect families may include:

- Heal
- Cure
- Light
- Sleep
- Unlock
- Fireball
- Protection
- Dispel
- Teleport
- Resurrection

The formulas that produce those effects are not universal. The campaign seed
determines a coherent magical system containing:

- Which magical principles exist, such as heat, memory, distance, decay, blood,
  weather, names, light, sympathy, or spirits
- Which combinations of principles and procedures create valid effects
- Required reagents, quantities, preparation methods, tools, gestures, spoken
  forms, locations, times, weather, or other conditions
- Substitutions, catalysts, incompatibilities, side effects, and dangerous failures
- Which plants, minerals, creature products, crafted substances, or artifacts act
  as reagents in this world
- Who discovered, preserved, controlled, outlawed, distorted, or lost each part
  of the system

The rules are generated at world creation and remain authoritative and
deterministic. The game must not randomly decide whether the same correctly
performed formula works on each attempt. Given equivalent state and conditions,
the formula has equivalent behavior.

An illustrative difference between worlds:

```text
Campaign A:
    Flame requires ashroot, powdered copper, and direct sunlight.
    Healing is taught by temples and uses river pearl and clean linen.
    Resurrection was lost when its monastery archive burned.

Campaign B:
    Flame requires salamander oil, an existing fire, and the spoken name of its fuel.
    Healing is controlled by a private guild and uses marsh reed and silver salts.
    Resurrection survives, but its rare reagent is politically controlled.
```

### Magical Discovery

The player should discover a world's magical rules through several channels:

- Books, inscriptions, laboratory notes, recipes, songs, and fragmented records
- Teachers, practitioners, family traditions, cults, guilds, and religious institutions
- Historical magical events and the physical evidence they left behind
- Examination of enchanted objects and magical creatures
- Observation of allies or enemies casting
- Trade in reagents, formulas, rumors, and counterfeit knowledge
- Controlled experimentation with substances, procedures, and conditions
- Combining partial formulas preserved by rival or isolated traditions

NPC magical knowledge uses the same truth, belief, and source model as ordinary
history. A practitioner may know a working formula, possess only one step, repeat
a faction's doctrine, conceal a dangerous condition, or sincerely believe an
incorrect theory. Dialogue and books therefore reveal claims about magic rather
than bypassing the simulation with guaranteed answers.

### Experimentation and the Magical Notebook

Experimentation should be meaningful without demanding tedious external
note-taking. The journal automatically maintains a magical notebook that records:

- Confirmed effects, reagents, procedures, and conditions
- Observed correlations and unresolved variables
- Partial formulas and their sources
- Contradictory accounts
- Failed experiments and known dangerous combinations
- Which conclusions are observed facts and which remain theories

The interface should help repeat a previously attempted procedure exactly and
clearly show which variable the player is changing. Early experimentation can use
safe or weak effects, while powerful principles introduce greater material,
physical, political, or supernatural risk.

The system should avoid arbitrary procedural noise. Generated reagents need
ecological and economic context: they grow somewhere, come from a creature or
material, can be cultivated, harvested, traded, monopolized, substituted, stolen,
or depleted. Magical discovery should connect exploration, botany, creatures,
crafting, history, trade, law, and conversation.

The player should eventually feel:

> I did not merely unlock this world's spell list. I learned how this world works.

### Implemented Causal Content Baseline

The first implementation now separates stable vocabulary from campaign truth in
`game_content`. Authored item forms, materials, effect families, principles, and
material sources remain legible across worlds. `WorldRules::generate(seed)`
selects deterministic, validated formula rules. History owns those rules and
records claims about them separately from objective truth, allowing witnesses to
know a complete fact, preserve only a reagent fragment, or eventually hold a
false theory without changing how the formula actually works.

The initial archaeological slice proves the cross-system contract:

```text
Seeded world rule
    ↓
historical crisis creates and loses one inscribed object
    ↓
history preserves fragmented claims about the object and formula
    ↓
world generation materializes that exact object in a causal dungeon
    ↓
its custodian carries the exact generated reagents
    ↓
player recovery changes custody and provenance
    ↓
study confirms the formula in the magical notebook
    ↓
ritual condition + physical reagents produce the stable effect
    ↓
reconstruction and hand-in become new history
```

Healing and kindling flame are intentionally only the first two effect families,
not a claim that the content breadth is complete. The important foundation is
that adding another effect, formula source, item form, reagent economy, or
knowledge fragment extends shared rules rather than creating a quest-specific
exception.

Players can also discover a formula experimentally without first finding its
complete inscription. An experiment names two carried reagent items, consumes
them, checks their material combination and the current environmental condition
against the campaign's hidden rules, and either records a failed reaction or
learns the matching formula. This gives fragmented historical claims and reagent
trade practical value while keeping the deterministic rules—not prose or a
quest flag—as authority.

## Material Consequences

History must leave physical evidence:

- Ruins
- Graves
- Abandoned roads
- Altered borders
- Damaged infrastructure
- Family heirlooms
- Written records
- Named weapons
- Refugee districts
- Religious monuments
- Changed wildlife populations

Every important entity should ideally answer:

- Where did this come from?
- Who created it?
- Who previously owned it?
- Why is it here?
- What changed because of it?

## Ordinary World Simulation

Mundane systems are essential to the Ultima feeling:

- Doors have owners, locks, and keys.
- Food is produced, transported, sold, and consumed.
- Shops depend on supplies.
- Written records contain actual world information.
- Containers retain ownership.
- Fire interacts consistently with materials.
- Poison can enter food or water.
- NPCs notice trespassing, theft, violence, and suspicious behavior.
- Wells, bridges, mills, ferries, and roads have real purposes.

Not every system needs extreme granularity. It needs sufficient consistency for the player to reason about consequences.

## Simulation Resolution

The world should simulate at different resolutions:

- **Near the player:** individual movement, interactions, objects, and conversations
- **Within the current settlement:** schedules and resource transfers
- **Distant settlements:** daily or weekly statistical updates
- **Historical generation:** monthly or yearly faction-level simulation

Important events are promoted into detailed records. Routine activity remains aggregated.

This is the primary performance strategy. The solution should not depend on updating every person every rendered frame.

## Time and Interaction Model

The initial design should use a fixed-tick world:

- Logical movement occurs on a grid.
- Movement may be animated smoothly between grid positions.
- NPC schedules follow the world clock.
- Conversations and detailed menus pause the world.
- A `Wait` command deliberately advances time.
- During ordinary exploration, a short wall-clock timeout emits `Wait` when the
  player has supplied no action; a player action resets that timeout.
- Combat may pause while the player chooses a target, item, or spell.

The current prototype uses a 600 ms exploration heartbeat. It preserves
grid-turn causality while preventing a standing player, friendly schedule, or
nearby enemy from freezing indefinitely.

## Input Model

The simulation receives semantic commands, never platform key codes.

```rust
enum GameCommand {
    Move(Direction),
    Select(GridPosition),
    Interact(EntityId),
    Inspect(EntityId),
    Talk(EntityId),
    Attack(EntityId),
    UseItem(ItemId),
    PrepareFormula(FormulaId),
    CastFormula(FormulaId, Target),
    AttemptExperiment(ExperimentId),
    Wait,
    OpenInventory,
    OpenMap,
    Pause,
}
```

### Desktop

- Arrow keys or WASD
- Mouse selection and interaction
- Keyboard shortcuts
- Optional gamepad support

### iPhone and iPad

- Tap a location to move
- Tap a character or object to select it
- Context-sensitive action menu
- Long press to inspect
- Bottom action bar
- Optional virtual directional pad
- Camera dragging and zooming
- Automatic pause while choosing complex actions

Touch input, keyboard input, mouse input, and gamepad input all translate into the same `GameCommand` values.

## Initial Visual Direction

The first renderer should use a simple Ultima IV or Dwarf Fortress-style overhead presentation:

- Square logical grid
- 2D tile atlas
- Clear symbols and sprites
- Minimal occlusion
- Fast iteration
- Easy debugging
- Suitable for AI-assisted asset creation

The simple renderer is not a permanent restriction. The simulation and presentation architecture must support future projection and art experiments.

## Technical Direction

### Language and Rendering

- Rust
- WGPU
- `winit` desktop host
- Thin Swift/Xcode host for iOS and iPadOS
- SceneVM-inspired rendering architecture, simplified for this specific game
- No dependency on a general-purpose game editor

The project should be a narrowly purpose-built game, not a new general RPG creation system or Eldiron replacement.

### Suggested Workspace

```text
crates/
├── game_core       # Deterministic simulation and commands
├── game_session    # Authoritative campaign transaction, consequences, and saves
├── worldgen        # Geography and settlement generation
├── history         # People, factions, and historical simulation
├── game_content    # Items, magical effects, reagent traits, occupations, and event definitions
├── game_present    # Semantic presentation snapshots
├── game_render     # WGPU tiles, sprites, text, effects, and game UI
├── game_audio      # Music and effect abstraction
├── client_desktop  # winit host
└── client_apple    # Rust library consumed by Xcode
```

The precise crate split should remain flexible during early development. Avoid fragmentation until boundaries prove useful.

Every host owns one `CampaignSession`; it is the transaction boundary joining
local simulation, historical world, campaign journal, player knowledge,
resident agents, regional parties, and calendar advancement. Desktop, Apple,
and Game Lab submit the same semantic commands and cannot independently decide
what becomes history. Versioned saves contain the campaign seed, history
horizon, and semantic command log. Loading reconstructs the complete
deterministic state by replay, so renderer or asset changes do not become part of
the save authority.

### Modular World Simulator

The living world is coordinated by one deterministic simulator rather than a set
of feature-specific update loops. Individual systems observe an immutable world
view and propose typed intents. A central transaction resolver orders those
intents, validates their preconditions, applies authoritative consequences, and
records important changes in the causal ledger.

```text
Immutable world view
    ↓
Planning / economy / logistics / construction / conflict /
population / politics / items / magic / grand strategy systems
    ↓
Typed intents in deterministic order
    ↓
Central validation and resolution
    ↓
State changes + historical events + provenance
    ↓
Next world view
```

Systems do not call one another or directly rewrite another system's state.
Context activates them: war may enable conscription and fortification behavior;
famine may activate migration, rationing, trade, and unrest; discovery of a
grimoire may activate magical research and competing claims. Their interaction
occurs through shared authoritative state and recorded consequences.

The same system may resolve at different levels of detail. A nearby battle can
expand into individual combatants, wounds, items, and terrain damage. A distant
campaign can resolve statistically while still producing compatible casualties,
custody changes, resource losses, borders, ruins, and causal events. Moving toward
or away from the player changes resolution, not the underlying rules.

## Platform Boundary

The platform host owns:

- Native window or view
- WGPU surface creation
- Resize and display-scale changes
- Native input collection
- Redraw scheduling
- Application lifecycle
- Platform save locations
- Native text entry where necessary

The renderer receives:

- WGPU device
- WGPU queue
- Surface configuration or frame target
- Logical drawable size
- Semantic presentation snapshot

The game core never depends on `winit`, Xcode, Swift, UIKit, or Metal-specific application behavior.

The same pattern used by SceneVM and DenrimNoise can allow a WGPU device and surface to be hosted by either `winit` or an Xcode application.

## Renderer

The first overhead renderer is intentionally small.

```rust
struct TileInstance {
    position: [i16; 2],
    tile_id: u16,
    layer: u8,
    flags: u8,
    tint: [u8; 4],
}
```

It requires:

- Texture atlases
- Static or chunked terrain instance buffers
- Dynamic object and character instance buffers
- Orthographic camera
- Animation frame selection
- Bitmap or signed-distance-field text
- Rectangles and nine-slice panels
- Optional fog, lighting, and weather overlays

The renderer should batch by texture and layer. It should not create a general scene graph for every stone, chair, or NPC.

## Abstract Presentation Model

The simulation must not know what an entity looks like or whether it is rendered as an overhead tile, isometric sprite, or 3D model.

```rust
struct VisibleEntity {
    id: EntityId,
    position: GridPos,
    facing: Direction,
    appearance: Appearance,
    state: VisualState,
}

enum Appearance {
    Terrain(TerrainKind),
    Wall {
        material: MaterialKind,
        damage: DamageState,
    },
    Door {
        material: MaterialKind,
    },
    Character(CharacterAppearance),
    Creature(CreatureKind),
    Item(ItemAppearance),
}
```

The simulation should never store an atlas index such as `tile_id = 473`.

### Presentation Pipeline

```text
Simulation
    ↓
Semantic presentation snapshot
    ↓
Projection
    ↓
Art pack
    ↓
Resolved drawing commands
    ↓
WGPU renderer
```

### Projection

Projection determines:

- Grid-to-screen coordinates
- Screen-to-grid picking
- Depth sorting
- Camera behavior
- Visible region
- Occlusion rules

```rust
trait Projection {
    fn world_to_screen(&self, position: GridPos) -> Vec2;
    fn screen_to_world(&self, position: Vec2) -> GridPos;
    fn sort_key(&self, entity: &VisibleEntity) -> i64;
}
```

Initial implementation:

- Square overhead projection

Possible future implementations:

- Ultima VI-style oblique presentation
- UO-style fixed isometric or axonometric presentation
- Locked-camera 3D presentation

### Art Pack

An art pack translates semantic appearance into assets.

```rust
trait ArtPack {
    fn resolve(
        &self,
        appearance: &Appearance,
        state: &VisualState,
        facing: Direction,
    ) -> ResolvedVisual;
}
```

One pack could map a guard to a square overhead tile. Another could map the same guard to an isometric animation. A 3D pack could resolve it to a mesh, material, and animation.

### Resolved Drawing Commands

```rust
struct SpriteCommand {
    texture: TextureId,
    source_rect: Rect,
    position: Vec2,
    anchor: Vec2,
    depth: i64,
    tint: Color,
}
```

The WGPU layer only sorts, batches, and renders commands. It does not need to understand guards, walls, virtues, or history.

## Future-Proof World Geometry

Even if the initial renderer is flat, the world should retain information a later isometric renderer may need:

- X, Y, and Z position
- Facing direction
- Object footprint
- Physical height
- Attachment slots
- Door and container states
- Wall connectivity
- Roof membership
- Sight blocking
- Movement blocking
- Support and stacking relationships

```rust
struct GridPos {
    x: i32,
    y: i32,
    z: i16,
}

struct Footprint {
    width: u8,
    depth: u8,
    height: u8,
}
```

The core stores meaningful physical information. The art pack determines its pixel representation.

## Terrain and Connection Rendering

The world stores semantic terrain:

```text
Terrain at (10, 12): grass
Terrain at (11, 12): dirt road
Wall at (14, 18): stone
```

It does not store:

```text
Grass north-east corner tile #17
Stone wall T-junction tile #84
```

The presentation layer derives:

- Terrain edges and corners
- Road connections
- Shorelines
- Wall junctions
- Fence connections
- Roof sections
- Shadows

Each projection and art pack may use different transition rules without changing the saved world.

## Stable Save Data

Save files contain stable semantic identifiers:

```text
terrain.grass
structure.wall.stone
furniture.chair.wooden
creature.orc
weapon.sword.long
```

They must not contain atlas coordinates or renderer-specific filenames.

This enables:

- Replacing the complete art style
- Higher-resolution assets
- Projection changes
- Mod support
- Missing-asset fallbacks
- Loading older saves after asset reorganization

Art manifests map semantic identifiers to presentation assets:

```text
"structure.wall.stone" {
    overhead: "stone_wall_01"
    isometric: "stone_wall_iso_01"
    fallback: "generic_wall"
}
```

## Future Isometric Presentation

If AI-assisted art generation becomes reliable enough to produce a coherent UO-style asset set, the game can add:

1. An isometric projection implementation
2. An isometric art manifest
3. Generated or pre-rendered sprite atlases
4. Asset anchors, footprints, and occlusion metadata
5. Roof hiding and depth sorting

The history generator, combat, NPC simulation, world data, and save files remain unchanged.

Possible simultaneous renderers include:

- ASCII or debug renderer
- Colored tile renderer
- Pixel-art overhead renderer
- UO-style isometric renderer
- Locked-camera 3D renderer

All display the same running world.

Inside the simulation, objects should be called terrain, structures, characters, creatures, and items. A tile is only one possible visual representation.

## Game UI

A small game-specific immediate-mode UI should be sufficient:

- Rectangles and panels
- Text
- Icons
- Buttons
- Lists and scrollable text
- Inventory grids
- Dialogue choices
- Tooltips
- Touch hit regions

Native UI is only necessary where the platform provides a clear advantage, such as the iOS on-screen keyboard for naming a character or save.

### Progressive Disclosure

Simulation depth must not become permanent interface noise.

- The exploration view shows the current lead, immediate danger, and at most one
  highest-priority regional situation.
- Stable routes, routine production, completed projects, and per-front statistics
  stay hidden until requested.
- The journal uses bounded pages suitable for keyboards, controllers, remotes,
  and touch rather than requiring a precise scrollbar.
- An on-demand regional view lists competing contracts and reveals the selected
  situation's sponsor, causal event, material state, and possible responses.
- Moving regional parties remain visible on the map but do not claim permanent
  sidebar sections. Approaching and inspecting one reveals its leader, purpose,
  route, cargo or strength, and historical cause.
- The current map layer and bearing to its regional connection remain visible as
  one compact navigation cue; changing maps must never depend on remembering an
  unseen exit.
- Routine production, trade journeys, travel arrivals, and gradual strategic
  drift remain in causal history without entering the player's journal. Major
  developments are aggregated into one monthly journal entry; only immediately
  actionable changes create a compact notification.
- Idle wall-clock time never advances the world calendar. Ordinary movement uses
  a deliberately slow player-scale clock; explicit rest, long travel, and future
  downtime actions are the appropriate ways to advance substantial time.
- Resolving a situation returns the player to a concise priority view; the full
  result remains available as history.

## AI's Role in Development

AI can assist with:

- System architecture
- Rust implementation
- World and history generation
- Simulation tests
- Save/load validation
- Seed fuzzing
- Renderer development
- Asset pipeline tooling
- Content definitions
- Writing and dialogue templates
- Debugging interfaces
- Concept art and visual exploration
- Initial tile, portrait, texture, and icon generation

Rust provides a strong AI development loop:

```text
Implement feature
    ↓
Compile
    ↓
Run deterministic tests
    ↓
Run fixed campaign seeds
    ↓
Launch desktop client
    ↓
Capture and inspect frames
    ↓
Correct behavior or presentation
```

The development build provides a Game Lab protocol for this loop. A persistent
headless session and an opt-in local bridge to the actual desktop process accept
the same semantic commands and return structured observations. The observer
includes the visible semantic viewport, nearby entities and landmarks, current
UI messages, player and calendar state, world summaries, and quantitative
experience metrics. Automated explorers can replay fixed seeds and measure
blocked movement, empty travel, meaningful-decision frequency, combat frequency,
terrain diversity, nearby activity, history growth, and journal volume.

The protocol also exposes structured local objectives and an end-to-end campaign
probe. The probe uses ordinary simulation commands and authoritative state: it
must locate moving contacts, gather physical evidence, question members of
different generated factions, traverse real stairs, survive combat, take custody
of the history-born item, reconstruct its inscription, satisfy the seed's
environmental ritual condition, consume its physical reagents, perform it, return
the object to its quest giver, resolve the crisis, and find a resident who can
react to the aftermath. It cannot mark those stages complete by editing quest
flags directly. This makes a successful replay evidence that the systems connect
into play, not merely that each subsystem has a passing unit test.

This interface exists to let development evaluate play rather than merely prove
that isolated systems are internally consistent. It is disabled in ordinary
desktop runs and is not part of the player-facing network architecture.

AI should not be the runtime authority over game state. The core world remains deterministic, testable code.

An optional language model may later turn structured facts into more natural prose, but it must not independently decide:

- Whether a character is alive
- Who owns an object
- What occurred historically
- Whether a law exists
- What consequences an action caused

The simulation supplies facts. Language generation may only present them.

## AI and Visual Assets

The game does not require a different tileset for each campaign. Generated campaigns reuse a stable visual library while changing placement, ownership, population, and condition.

AI is suitable for:

- Style exploration
- Concept art
- Portraits
- Object and costume drafts
- Texture drafts
- Palette variants
- Damaged, burned, ruined, or overgrown variants
- Asset organization and validation

AI is currently less dependable for producing hundreds of mutually consistent isometric tiles with exact:

- Projection
- Lighting
- Scale
- Connectivity
- Transparent boundaries
- Animation
- Anchors
- Occlusion

If a UO-like style is pursued later, a constrained 3D-to-2D pipeline may be preferable:

```text
Fixed modular 3D model
    ↓
Fixed camera, scale, and lighting
    ↓
AI-assisted materials or styling
    ↓
Automated multi-direction rendering
    ↓
Anchor and dimension validation
    ↓
Sprite atlas
```

The abstract presentation system allows this decision to be deferred.

## Testing Strategy

Procedural simulation requires aggressive automated testing.

For every generated world, verify:

- The player can obtain food and basic equipment.
- Important regions are reachable.
- Critical resource dependencies are not circular.
- Settlements can survive under ordinary conditions.
- Generated social rules do not disable core gameplay.
- Essential knowledge is discoverable.
- There are multiple plausible responses to the main crisis.
- No rule produces an infinite event or reproduction loop.
- No ancestry or culture receives an inherent good or evil disposition.
- Mixed-ancestry families can arise through several historical contexts and are
  not automatically associated with coercion, stigma, or antagonism.
- Sensitive-event generation obeys the selected content-severity policy.
- Player-facing procedural prose never turns sexual violence or harm to children
  into graphic description, humor, reward, or incidental flavor.
- Save/load produces equivalent state.
- Replaying a seed and command sequence produces the same outcome.

Testing tools should include:

- Fixed regression seeds
- Random seed fuzzing
- Headless historical simulation
- Event log inspection
- Causal graph inspection
- NPC schedule visualization
- Resource flow visualization
- Relationship and belief inspection
- Deterministic command replays
- Renderer snapshot tests
- Headless and live-session Game Lab observations
- Automated exploration with quiet-stretch and decision-density thresholds
- End-to-end campaign probes across a seed corpus and every crisis resolution
- Seed-corpus checks for distinct governing doctrines, policies, artifacts, and
  dungeon themes

When a generated outcome is surprising, the developer should be able to inspect why it happened.

## First Vertical Slice

Do not begin with a continent. Build one detailed town inside a small,
statistically simulated region.

### World

- One 64×64 town and its immediate surroundings
- 25–40 named inhabitants
- Homes and workplaces
- One inn
- One temple or healer
- One market
- One government building
- One farm or resource producer
- One nearby ruin
- Four to seven surrounding settlements resolved at regional detail
- A connected road and trade network

### Simulation

- Seven-day schedules
- Families and relationships
- Ownership and theft
- Food production, transport, sale, and consumption
- Regional production, shortages, trade disruption, recovery, and migration
- Three factions
- One civic principle
- One generated shortage or crisis
- One oppressive but defensible law
- One blamed outsider group
- One concealed historical cause
- Twenty years of generated local history
- One three-level ruin whose strata and artifact derive from that history

### Player Interaction

- Movement
- Inspection
- Conversation
- Inventory
- A generated quest contract and journal objective
- Stairs and floor-aware exploration
- Experience, level, health, and attack progression
- Doors and containers
- Purchasing and theft
- Familiar weapons and one or two spells
- Several systemic ways to change the crisis
- A visible aftermath

### Presentation

- Colored or simple authored overhead tiles
- Desktop keyboard and mouse input
- Abstract command layer
- Basic scalable game UI
- Save/load

### Validation

The town should be able to simulate thirty days without the player. Its population, shortages, relationships, and political behavior should remain explainable.

The core succeeds when it produces arguments, discoveries, and consequences the designer did not explicitly script.

## Development Priorities

1. Deterministic headless simulation
2. Structured historical events and provenance
3. One generated town
4. Debugging and causal inspection tools
5. Simple overhead WGPU renderer
6. Player movement and interaction
7. NPC schedules and knowledge
8. Conflict and crisis generator
9. History-derived quests, dungeons, artifacts, and progression
10. Save/load and deterministic replay
11. Desktop vertical slice
12. Touch input and Apple host
13. Expanded content and world scale
14. Alternative projection and art experiments

## Non-Goals for the Initial Project

- A general-purpose RPG creator
- A complete replacement for Eldiron
- Arbitrary generated laws of physics
- Runtime AI controlling authoritative game state
- Fully generated art unique to every campaign
- Dwarf Fortress-level simulation of every minor physical process
- A continent-sized first milestone
- A large general-purpose scene graph
- An isometric renderer before the simulation proves interesting

## Key Risks

### Generated incoherence

Mitigation:

- Generate within authored structures.
- Preserve explicit causes and consequences.
- Validate campaign prerequisites.

### Simulation without meaningful gameplay

Mitigation:

- Convert systemic state into discoverable problems.
- Ensure player verbs can affect relationships, resources, laws, and beliefs.

### Excessive scope

Mitigation:

- Build one town.
- Aggregate distant and historical activity.
- Do not create a general engine or editor.

### Unexplainable emergent behavior

Mitigation:

- Record decision inputs.
- Preserve event provenance.
- Build causal inspection tools early.

### Art bottleneck

Mitigation:

- Begin with simple overhead tiles.
- Keep appearance semantic and renderer-independent.
- Reuse one modular art library across campaigns.
- Defer isometric presentation until the asset pipeline is viable.

## Naming Note

*Ultimate Fate* is a strong working title because:

- "Ultimate" subtly evokes *Ultima*.
- "Fate" reflects generated history and player consequences.
- It sounds like a classic fantasy role-playing game.

Before commercial announcement, the title should receive proper storefront, domain, and trademark clearance.

## Final Vision

The player enters a familiar fantasy world whose details they do not yet understand.

The weapons and spells are recognizable. The towns have shops, temples, homes, laws, and working people. Beneath that familiarity is a generated web of ancestry, ownership, injustice, memory, misinformation, and historical consequence.

The player does not complete a sequence of disconnected generated quests. They enter a society under pressure, learn how it became this way, choose which people and principles to support, and become part of the world's continuing history.

At the end of a campaign, different groups may remember the player differently:

> The southern histories remember you as the liberator of Carin. Monastic records blame you for the famine that followed. Among the river people, your name became a word meaning "one who keeps an impossible promise."
