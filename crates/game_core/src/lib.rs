//! Deterministic, renderer-independent game simulation.
//!
//! This crate deliberately has no windowing, GPU, asset, or platform dependencies.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use ultimate_fate_content::{
    FormulaCondition, FormulaId, MagicEffect, MaterialKind, ObjectId, WorldRules,
};

pub const CHUNK_SIZE_XY: i32 = 32;
pub const CHUNK_SIZE_Z: i32 = 8;
const HOSTILE_NOTICE_RANGE: i32 = 12;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct MapId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EntityId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ItemId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct QuestId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct GridPos {
    pub x: i32,
    pub y: i32,
    pub z: i16,
}

impl GridPos {
    pub const fn new(x: i32, y: i32, z: i16) -> Self {
        Self { x, y, z }
    }

    pub fn offset(self, dx: i32, dy: i32, dz: i16) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct WorldPosition {
    pub map: MapId,
    pub grid: GridPos,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LocalPos {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

impl ChunkCoord {
    pub fn containing(position: GridPos) -> (Self, LocalPos) {
        let z = i32::from(position.z);
        (
            Self {
                x: position.x.div_euclid(CHUNK_SIZE_XY),
                y: position.y.div_euclid(CHUNK_SIZE_XY),
                z: z.div_euclid(CHUNK_SIZE_Z),
            },
            LocalPos {
                x: position.x.rem_euclid(CHUNK_SIZE_XY) as u8,
                y: position.y.rem_euclid(CHUNK_SIZE_XY) as u8,
                z: z.rem_euclid(CHUNK_SIZE_Z) as u8,
            },
        )
    }

    pub fn resolve(self, local: LocalPos) -> GridPos {
        GridPos::new(
            self.x * CHUNK_SIZE_XY + i32::from(local.x),
            self.y * CHUNK_SIZE_XY + i32::from(local.y),
            (self.z * CHUNK_SIZE_Z + i32::from(local.z)) as i16,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum TerrainKind {
    #[default]
    Grass,
    Forest,
    Hills,
    Mountain,
    Sand,
    Snow,
    Swamp,
    Dirt,
    Road,
    Ocean,
    Water,
    Bridge,
    StoneFloor,
    Wall,
    Farmland,
    Rubble,
    StairsUp,
    StairsDown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TerrainCell {
    pub terrain: TerrainKind,
    pub movement_blocked: bool,
    pub sight_blocked: bool,
}

impl TerrainCell {
    pub const fn new(terrain: TerrainKind) -> Self {
        Self {
            terrain,
            movement_blocked: matches!(
                terrain,
                TerrainKind::Ocean | TerrainKind::Water | TerrainKind::Mountain | TerrainKind::Wall
            ),
            sight_blocked: matches!(terrain, TerrainKind::Wall),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Chunk {
    cells: BTreeMap<LocalPos, TerrainCell>,
}

impl Chunk {
    pub fn cell(&self, position: LocalPos) -> Option<&TerrainCell> {
        self.cells.get(&position)
    }

    pub fn set_cell(&mut self, position: LocalPos, cell: TerrainCell) {
        self.cells.insert(position, cell);
    }

    pub fn cells(&self) -> impl Iterator<Item = (LocalPos, &TerrainCell)> {
        self.cells.iter().map(|(position, cell)| (*position, cell))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldMap {
    pub id: MapId,
    chunks: BTreeMap<ChunkCoord, Chunk>,
}

impl WorldMap {
    pub fn new(id: MapId) -> Self {
        Self {
            id,
            chunks: BTreeMap::new(),
        }
    }

    pub fn cell(&self, position: GridPos) -> Option<&TerrainCell> {
        let (chunk, local) = ChunkCoord::containing(position);
        self.chunks.get(&chunk)?.cell(local)
    }

    pub fn set_cell(&mut self, position: GridPos, cell: TerrainCell) {
        let (chunk, local) = ChunkCoord::containing(position);
        self.chunks.entry(chunk).or_default().set_cell(local, cell);
    }

    pub fn cells(&self) -> impl Iterator<Item = (GridPos, &TerrainCell)> {
        self.chunks.iter().flat_map(|(chunk_coord, chunk)| {
            chunk
                .cells()
                .map(|(local, cell)| (chunk_coord.resolve(local), cell))
        })
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Direction {
    North,
    East,
    #[default]
    South,
    West,
}

impl Direction {
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    pub const fn delta(self) -> (i32, i32) {
        match self {
            Self::North => (0, -1),
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityKind {
    Player,
    Character,
    Creature,
    Item,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub position: WorldPosition,
    pub facing: Direction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmmunitionKind {
    Arrow,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BookSubject {
    LocalHistory,
    Law,
    Trade,
    NaturalLore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemKind {
    MeleeWeapon {
        damage: i16,
    },
    RangedWeapon {
        damage: i16,
        range: u8,
        ammunition: AmmunitionKind,
    },
    Ammunition {
        kind: AmmunitionKind,
    },
    Consumable {
        healing: i16,
    },
    Food {
        nourishment: u8,
    },
    Drink {
        hydration: u8,
    },
    Book {
        subject: BookSubject,
    },
    Key {
        lock_code: u64,
    },
    Tool,
    Reagent {
        material: MaterialKind,
    },
    InscribedArtifact {
        object: ObjectId,
        formula: FormulaId,
    },
    Artifact,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Item {
    pub id: ItemId,
    pub name: String,
    pub kind: ItemKind,
    pub quantity: u16,
    pub weight_grams: u32,
    pub quality: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inventory {
    pub owner: EntityId,
    pub items: BTreeSet<ItemId>,
    pub equipped_melee: Option<ItemId>,
    pub equipped_ranged: Option<ItemId>,
}

impl Inventory {
    fn new(owner: EntityId) -> Self {
        Self {
            owner,
            items: BTreeSet::new(),
            equipped_melee: None,
            equipped_ranged: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Container {
    pub entity: EntityId,
    pub name: String,
    pub owner: EntityId,
    pub capacity_grams: u32,
    pub lock_code: Option<u64>,
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlayerNeeds {
    pub hunger: u8,
    pub thirst: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Combatant {
    pub entity: EntityId,
    pub health: i16,
    pub max_health: i16,
    pub armor: i16,
    pub hostile_to_player: bool,
    pub experience_reward: u32,
}

impl Combatant {
    pub fn is_alive(self) -> bool {
        self.health > 0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LandmarkKind {
    Shop,
    Granary,
    CouncilHall,
    GuildHall,
    Inn,
    Temple,
    Infirmary,
    Smithy,
    Mill,
    Farm,
    Residence,
    Memorial,
    Ruin,
    TownSquare,
    RiverDock,
    Gate,
    DungeonEntrance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Landmark {
    pub name: String,
    pub kind: LandmarkKind,
    pub position: WorldPosition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionKind {
    Descend,
    Ascend,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    pub from: WorldPosition,
    pub to: WorldPosition,
    pub kind: TransitionKind,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestStatus {
    Active,
    ReadyToTurnIn,
    Completed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestObjectiveKind {
    Defeat(EntityId),
    Recover(ItemId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuestObjective {
    pub description: String,
    pub kind: QuestObjectiveKind,
    pub completed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Quest {
    pub id: QuestId,
    pub title: String,
    pub description: String,
    pub giver: EntityId,
    pub status: QuestStatus,
    pub objectives: Vec<QuestObjective>,
    pub reward_experience: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterProgression {
    pub level: u16,
    pub experience: u32,
    pub attack_bonus: i16,
    pub discoveries: u16,
    pub arcane_lore: u16,
    pub martial_practice: u16,
    pub ranged_practice: u16,
    pub magical_practice: u16,
    pub social_practice: u16,
    pub exploration: u16,
    pub fulfilled_commitments: u16,
    pub world_changes: u16,
}

impl Default for CharacterProgression {
    fn default() -> Self {
        Self {
            level: 1,
            experience: 0,
            attack_bonus: 0,
            discoveries: 0,
            arcane_lore: 0,
            martial_practice: 0,
            ranged_practice: 0,
            magical_practice: 0,
            social_practice: 0,
            exploration: 0,
            fulfilled_commitments: 0,
            world_changes: 0,
        }
    }
}

impl CharacterProgression {
    pub fn experience_for_next_level(self) -> u32 {
        u32::from(self.level) * 100
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameCommand {
    Move(Direction),
    Attack(EntityId),
    FireAt(EntityId),
    Equip(ItemId),
    UseItem(ItemId),
    Study(ItemId),
    Give {
        item: ItemId,
        to: EntityId,
    },
    GiveQuantity {
        item: ItemId,
        to: EntityId,
        quantity: u16,
    },
    Take {
        item: ItemId,
        from: EntityId,
    },
    TakeQuantity {
        item: ItemId,
        from: EntityId,
        quantity: u16,
    },
    OpenContainer(EntityId),
    UnlockContainer {
        container: EntityId,
        key: ItemId,
    },
    Place {
        item: ItemId,
        container: EntityId,
    },
    PlaceQuantity {
        item: ItemId,
        container: EntityId,
        quantity: u16,
    },
    Drop(ItemId),
    DropQuantity {
        item: ItemId,
        quantity: u16,
    },
    Read(ItemId),
    Eat(ItemId),
    Drink(ItemId),
    Experiment {
        first: ItemId,
        second: ItemId,
    },
    Cast {
        formula: FormulaId,
        target: Option<EntityId>,
    },
    Traverse,
    TurnInQuest(QuestId),
    Wait,
    Pause,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatMethod {
    Melee,
    Ranged,
    Magic,
    Retaliation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionFailure {
    InvalidTarget,
    OutOfRange,
    LineBlocked,
    NoWeapon,
    NoAmmunition,
    ItemNotCarried,
    ItemCannotBeUsed,
    AlreadyAtFullHealth,
    UnknownFormula,
    MissingReagent(MaterialKind),
    MagicalConditionUnmet(FormulaCondition),
    NoTransition,
    QuestNotReady,
    ExperimentFailed,
    ContainerLocked,
    WrongKey,
    ContainerFull,
    NotAContainer,
    AlreadySatisfied,
    InvalidQuantity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimulationEvent {
    Damaged {
        attacker: EntityId,
        target: EntityId,
        amount: i16,
        remaining_health: i16,
        method: CombatMethod,
    },
    Defeated {
        entity: EntityId,
        by: EntityId,
    },
    ItemEquipped {
        owner: EntityId,
        item: ItemId,
    },
    ItemTransferred {
        item: ItemId,
        from: EntityId,
        to: EntityId,
    },
    ItemQuantityTransferred {
        source: ItemId,
        item: ItemId,
        quantity: u16,
        from: EntityId,
        to: EntityId,
    },
    ItemConsumed {
        owner: EntityId,
        item: ItemId,
        remaining: u16,
    },
    ContainerOpened {
        container: EntityId,
    },
    ContainerUnlocked {
        container: EntityId,
        key: ItemId,
    },
    ItemDropped {
        item: ItemId,
        holder: EntityId,
        position: WorldPosition,
    },
    ItemRead {
        item: ItemId,
        subject: BookSubject,
        newly_learned: bool,
    },
    NeedsChanged {
        hunger: u8,
        thirst: u8,
    },
    FormulaLearned {
        formula: FormulaId,
        source: ItemId,
    },
    SpellCast {
        caster: EntityId,
        formula: FormulaId,
        effect: MagicEffect,
    },
    Healed {
        entity: EntityId,
        amount: i16,
        health: i16,
    },
    RevivedAtHealer {
        entity: EntityId,
        healer: EntityId,
        health: i16,
        destination: WorldPosition,
    },
    Traversed {
        kind: TransitionKind,
        destination: WorldPosition,
    },
    ExperienceGained {
        amount: u32,
        total: u32,
    },
    LevelGained {
        level: u16,
        max_health: i16,
        attack_bonus: i16,
    },
    QuestAdvanced {
        quest: QuestId,
        objective: usize,
    },
    QuestReadyToTurnIn {
        quest: QuestId,
    },
    QuestCompleted {
        quest: QuestId,
    },
    ActionFailed(ActionFailure),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandOutcome {
    pub advanced_time: bool,
    pub changed_world: bool,
    pub events: Vec<SimulationEvent>,
}

/// A named deterministic random stream. Subsystems should use separate stream IDs so
/// adding a random choice to terrain generation cannot alter family generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamId(pub u64);

impl StreamId {
    pub const TERRAIN: Self = Self(0x0054_4552_5241_494e);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomStream {
    state: u64,
}

impl RandomStream {
    pub fn new(campaign_seed: u64, stream: StreamId) -> Self {
        Self {
            state: mix64(campaign_seed ^ stream.0),
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mix64(self.state)
    }

    pub fn one_in(&mut self, denominator: u64) -> bool {
        denominator != 0 && self.next_u64().is_multiple_of(denominator)
    }
}

const fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Simulation {
    pub campaign_seed: u64,
    pub tick: u64,
    pub paused: bool,
    maps: BTreeMap<MapId, WorldMap>,
    entities: BTreeMap<EntityId, Entity>,
    landmarks: Vec<Landmark>,
    items: BTreeMap<ItemId, Item>,
    legal_owners: BTreeMap<ItemId, EntityId>,
    stolen_items: BTreeSet<ItemId>,
    inventories: BTreeMap<EntityId, Inventory>,
    containers: BTreeMap<EntityId, Container>,
    combatants: BTreeMap<EntityId, Combatant>,
    transitions: BTreeMap<WorldPosition, Transition>,
    quests: BTreeMap<QuestId, Quest>,
    healers: BTreeSet<EntityId>,
    progression: CharacterProgression,
    rules: WorldRules,
    known_formulas: BTreeSet<FormulaId>,
    read_items: BTreeSet<ItemId>,
    player_needs: PlayerNeeds,
    next_loose_item_entity: u64,
    next_split_item_id: u64,
    visited_maps: BTreeSet<MapId>,
    player: EntityId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulationBuildError {
    MissingPlayer(EntityId),
    PlayerEntityRequired(EntityId),
    EntityOnDifferentMap(EntityId),
    LandmarkOnDifferentMap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameplayBuildError {
    MissingEntity(EntityId),
    DuplicateEntity(EntityId),
    EntityOnMissingMap(EntityId),
    EntityOnBlockedCell(EntityId),
    DuplicateMap(MapId),
    LandmarkOnMissingMap,
    DuplicateItem(ItemId),
    MissingItem(ItemId),
    InvalidQuantity { item: ItemId, quantity: u16 },
    ItemNotCarried { item: ItemId, owner: EntityId },
    InvalidContainer(EntityId),
    ContainerFull(EntityId),
    InvalidCombatant(EntityId),
    InvalidTransition(WorldPosition),
    DuplicateTransition(WorldPosition),
    DuplicateQuest(QuestId),
}

impl Simulation {
    pub fn from_map(
        campaign_seed: u64,
        map: WorldMap,
        entities: impl IntoIterator<Item = Entity>,
        landmarks: Vec<Landmark>,
        player: EntityId,
    ) -> Result<Self, SimulationBuildError> {
        Self::from_map_with_rules(
            campaign_seed,
            WorldRules::generate(campaign_seed),
            map,
            entities,
            landmarks,
            player,
        )
    }

    pub fn from_map_with_rules(
        campaign_seed: u64,
        rules: WorldRules,
        map: WorldMap,
        entities: impl IntoIterator<Item = Entity>,
        landmarks: Vec<Landmark>,
        player: EntityId,
    ) -> Result<Self, SimulationBuildError> {
        let map_id = map.id;
        let entities = entities
            .into_iter()
            .map(|entity| (entity.id, entity))
            .collect::<BTreeMap<_, _>>();
        let player_entity = entities
            .get(&player)
            .ok_or(SimulationBuildError::MissingPlayer(player))?;
        if player_entity.kind != EntityKind::Player {
            return Err(SimulationBuildError::PlayerEntityRequired(player));
        }
        if let Some(entity) = entities
            .values()
            .find(|entity| entity.position.map != map_id)
        {
            return Err(SimulationBuildError::EntityOnDifferentMap(entity.id));
        }
        if landmarks
            .iter()
            .any(|landmark| landmark.position.map != map_id)
        {
            return Err(SimulationBuildError::LandmarkOnDifferentMap);
        }

        Ok(Self {
            campaign_seed,
            tick: 0,
            paused: false,
            maps: BTreeMap::from([(map_id, map)]),
            entities,
            landmarks,
            items: BTreeMap::new(),
            legal_owners: BTreeMap::new(),
            stolen_items: BTreeSet::new(),
            inventories: BTreeMap::new(),
            containers: BTreeMap::new(),
            combatants: BTreeMap::new(),
            transitions: BTreeMap::new(),
            quests: BTreeMap::new(),
            healers: BTreeSet::new(),
            progression: CharacterProgression::default(),
            rules,
            known_formulas: BTreeSet::new(),
            read_items: BTreeSet::new(),
            player_needs: PlayerNeeds {
                hunger: 20,
                thirst: 15,
            },
            next_loose_item_entity: 0xd000_0000_0000_0000,
            next_split_item_id: 0xe000_0000_0000_0000,
            visited_maps: BTreeSet::from([map_id]),
            player,
        })
    }

    /// Creates a deterministic test town. This is only bootstrap content; later the
    /// history-aware world generator will produce the same semantic map structures.
    pub fn demo(campaign_seed: u64) -> Self {
        let map_id = MapId(1);
        let mut map = WorldMap::new(map_id);
        let mut terrain_rng = RandomStream::new(campaign_seed, StreamId::TERRAIN);

        for y in -36..=36 {
            for x in -48..=48 {
                let mut terrain = if (-1..=1).contains(&x) || (-1..=1).contains(&y) {
                    TerrainKind::Road
                } else if terrain_rng.one_in(41) {
                    TerrainKind::Dirt
                } else {
                    TerrainKind::Grass
                };

                if (14..=15).contains(&x) {
                    terrain = if (-1..=1).contains(&y) {
                        TerrainKind::Bridge
                    } else {
                        TerrainKind::Water
                    };
                }

                map.set_cell(GridPos::new(x, y, 0), TerrainCell::new(terrain));
            }
        }

        paint_rect(
            &mut map,
            -3,
            -3,
            3,
            3,
            TerrainCell::new(TerrainKind::StoneFloor),
        );
        paint_rect(
            &mut map,
            -31,
            -16,
            -23,
            -7,
            TerrainCell::new(TerrainKind::Farmland),
        );
        paint_rect(
            &mut map,
            -31,
            8,
            -23,
            17,
            TerrainCell::new(TerrainKind::Farmland),
        );
        paint_building(&mut map, -16, 3, -11, 8, GridPos::new(-13, 3, 0));
        paint_building(&mut map, -9, -12, -3, -6, GridPos::new(-6, -6, 0));
        paint_building(&mut map, 3, -12, 9, -6, GridPos::new(6, -6, 0));
        paint_building(&mut map, -9, 6, -3, 12, GridPos::new(-6, 6, 0));
        paint_building(&mut map, 3, 6, 9, 12, GridPos::new(6, 6, 0));
        paint_rect(
            &mut map,
            17,
            -3,
            21,
            3,
            TerrainCell::new(TerrainKind::StoneFloor),
        );

        let player = EntityId(1);
        let player_entity = Entity {
            id: player,
            kind: EntityKind::Player,
            position: WorldPosition {
                map: map_id,
                grid: GridPos::new(-20, 0, 0),
            },
            facing: Direction::East,
        };
        let characters = [
            (2, GridPos::new(-13, 3, 0)),
            (3, GridPos::new(-6, -6, 0)),
            (4, GridPos::new(6, -6, 0)),
            (5, GridPos::new(-6, 6, 0)),
            (6, GridPos::new(6, 6, 0)),
            (7, GridPos::new(19, 1, 0)),
            (8, GridPos::new(-1, 2, 0)),
            (9, GridPos::new(2, -1, 0)),
        ];
        let mut entities = BTreeMap::from([(player, player_entity)]);
        entities.extend(characters.map(|(id, grid)| {
            let id = EntityId(id);
            (
                id,
                Entity {
                    id,
                    kind: EntityKind::Character,
                    position: WorldPosition { map: map_id, grid },
                    facing: Direction::South,
                },
            )
        }));
        let landmarks = [
            ("Trading House", LandmarkKind::Shop, -13, 3),
            ("Common Granary", LandmarkKind::Granary, -6, -6),
            ("Civic Council", LandmarkKind::CouncilHall, 6, -6),
            ("Hearth Guild", LandmarkKind::GuildHall, -6, 6),
            ("The Wayfarer Inn", LandmarkKind::Inn, 6, 6),
            ("Market Square", LandmarkKind::TownSquare, 0, 0),
            ("Free River Dock", LandmarkKind::RiverDock, 19, 1),
        ]
        .map(|(name, kind, x, y)| Landmark {
            name: name.to_string(),
            kind,
            position: WorldPosition {
                map: map_id,
                grid: GridPos::new(x, y, 0),
            },
        })
        .to_vec();

        Self {
            campaign_seed,
            tick: 0,
            paused: false,
            maps: BTreeMap::from([(map_id, map)]),
            entities,
            landmarks,
            items: BTreeMap::new(),
            legal_owners: BTreeMap::new(),
            stolen_items: BTreeSet::new(),
            inventories: BTreeMap::new(),
            containers: BTreeMap::new(),
            combatants: BTreeMap::new(),
            transitions: BTreeMap::new(),
            quests: BTreeMap::new(),
            healers: BTreeSet::new(),
            progression: CharacterProgression::default(),
            rules: WorldRules::generate(campaign_seed),
            known_formulas: BTreeSet::new(),
            read_items: BTreeSet::new(),
            player_needs: PlayerNeeds {
                hunger: 20,
                thirst: 15,
            },
            next_loose_item_entity: 0xd000_0000_0000_0000,
            next_split_item_id: 0xe000_0000_0000_0000,
            visited_maps: BTreeSet::from([map_id]),
            player,
        }
    }

    pub fn map(&self, id: MapId) -> Option<&WorldMap> {
        self.maps.get(&id)
    }

    pub fn add_map(
        &mut self,
        map: WorldMap,
        landmarks: impl IntoIterator<Item = Landmark>,
    ) -> Result<(), GameplayBuildError> {
        if self.maps.contains_key(&map.id) {
            return Err(GameplayBuildError::DuplicateMap(map.id));
        }
        let map_id = map.id;
        let landmarks = landmarks.into_iter().collect::<Vec<_>>();
        if landmarks
            .iter()
            .any(|landmark| landmark.position.map != map_id)
        {
            return Err(GameplayBuildError::LandmarkOnMissingMap);
        }
        self.maps.insert(map_id, map);
        self.landmarks.extend(landmarks);
        Ok(())
    }

    pub fn add_entity(&mut self, entity: Entity) -> Result<(), GameplayBuildError> {
        if self.entities.contains_key(&entity.id) {
            return Err(GameplayBuildError::DuplicateEntity(entity.id));
        }
        let valid_position = self
            .maps
            .get(&entity.position.map)
            .ok_or(GameplayBuildError::EntityOnMissingMap(entity.id))?
            .cell(entity.position.grid)
            .is_some_and(|cell| !cell.movement_blocked);
        if !valid_position {
            return Err(GameplayBuildError::EntityOnBlockedCell(entity.id));
        }
        self.entities.insert(entity.id, entity);
        Ok(())
    }

    pub fn move_entity(&mut self, entity: EntityId, position: WorldPosition) -> bool {
        let can_enter = self
            .maps
            .get(&position.map)
            .and_then(|map| map.cell(position.grid))
            .is_some_and(|cell| !cell.movement_blocked);
        if !can_enter {
            return false;
        }
        let Some(entity) = self.entities.get_mut(&entity) else {
            return false;
        };
        let dx = position.grid.x - entity.position.grid.x;
        let dy = position.grid.y - entity.position.grid.y;
        entity.facing = if dx.abs() >= dy.abs() {
            if dx >= 0 {
                Direction::East
            } else {
                Direction::West
            }
        } else if dy >= 0 {
            Direction::South
        } else {
            Direction::North
        };
        entity.position = position;
        true
    }

    pub fn remove_entity(&mut self, entity: EntityId) -> bool {
        if entity == self.player || self.entities.remove(&entity).is_none() {
            return false;
        }
        self.healers.remove(&entity);
        self.combatants.remove(&entity);
        self.containers.remove(&entity);
        self.inventories.remove(&entity);
        true
    }

    pub fn set_terrain_cell(&mut self, map: MapId, position: GridPos, cell: TerrainCell) -> bool {
        if cell.movement_blocked
            && self.entities.values().any(|entity| {
                entity.position.map == map
                    && entity.position.grid == position
                    && self
                        .combatants
                        .get(&entity.id)
                        .is_none_or(|combatant| combatant.is_alive())
            })
        {
            return false;
        }
        let Some(map) = self.maps.get_mut(&map) else {
            return false;
        };
        map.set_cell(position, cell);
        true
    }

    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values().filter(|entity| {
            self.combatants
                .get(&entity.id)
                .is_none_or(|combatant| combatant.is_alive())
        })
    }

    pub fn landmarks(&self) -> impl Iterator<Item = &Landmark> {
        self.landmarks.iter()
    }

    pub fn transitions(&self) -> impl Iterator<Item = &Transition> {
        self.transitions.values()
    }

    pub fn transition_at(&self, position: WorldPosition) -> Option<&Transition> {
        self.transitions.get(&position)
    }

    pub fn quests(&self) -> impl Iterator<Item = &Quest> {
        self.quests.values()
    }

    pub fn quest(&self, id: QuestId) -> Option<&Quest> {
        self.quests.get(&id)
    }

    pub fn progression(&self) -> CharacterProgression {
        self.progression
    }

    pub fn record_social_practice(&mut self) {
        self.progression.social_practice = self.progression.social_practice.saturating_add(1);
    }

    pub fn record_world_change(&mut self) {
        self.progression.world_changes = self.progression.world_changes.saturating_add(1);
    }

    pub fn rules(&self) -> &WorldRules {
        &self.rules
    }

    pub fn known_formulas(&self) -> &BTreeSet<FormulaId> {
        &self.known_formulas
    }

    pub fn player(&self) -> &Entity {
        &self.entities[&self.player]
    }

    pub fn player_id(&self) -> EntityId {
        self.player
    }

    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }

    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.get(&id)
    }

    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.items.values()
    }

    pub fn legal_owner(&self, item: ItemId) -> Option<EntityId> {
        self.legal_owners.get(&item).copied()
    }

    /// Transfers legal title independently from physical custody.
    ///
    /// The current legal owner must authorize the transfer. This keeps gifts and
    /// purchases distinct from theft: merely carrying an object never proves
    /// ownership.
    pub fn transfer_legal_ownership(&mut self, item: ItemId, from: EntityId, to: EntityId) -> bool {
        if !self.items.contains_key(&item)
            || !self.entities.contains_key(&from)
            || !self.entities.contains_key(&to)
            || self.legal_owner(item) != Some(from)
        {
            return false;
        }
        self.legal_owners.insert(item, to);
        self.stolen_items.remove(&item);
        true
    }

    pub fn is_stolen(&self, item: ItemId) -> bool {
        self.stolen_items.contains(&item)
    }

    pub fn inventory(&self, owner: EntityId) -> Option<&Inventory> {
        self.inventories.get(&owner)
    }

    pub fn player_inventory(&self) -> Option<&Inventory> {
        self.inventory(self.player)
    }

    pub fn containers(&self) -> impl Iterator<Item = &Container> {
        self.containers.values()
    }

    pub fn container(&self, entity: EntityId) -> Option<&Container> {
        self.containers.get(&entity)
    }

    pub fn player_needs(&self) -> PlayerNeeds {
        self.player_needs
    }

    pub fn container_weight(&self, entity: EntityId) -> Option<u32> {
        let inventory = self.inventory(entity);
        self.container(entity)?;
        Some(
            inventory
                .into_iter()
                .flat_map(|inventory| inventory.items.iter())
                .filter_map(|item| self.item(*item))
                .fold(0_u32, |total, item| total.saturating_add(item.weight_grams)),
        )
    }

    pub fn add_container(&mut self, container: Container) -> Result<(), GameplayBuildError> {
        let Some(entity) = self.entities.get(&container.entity) else {
            return Err(GameplayBuildError::MissingEntity(container.entity));
        };
        if entity.kind != EntityKind::Item
            || container.capacity_grams == 0
            || (container.locked && container.lock_code.is_none())
            || !self.entities.contains_key(&container.owner)
            || self.containers.contains_key(&container.entity)
        {
            return Err(GameplayBuildError::InvalidContainer(container.entity));
        }
        self.inventories
            .entry(container.entity)
            .or_insert_with(|| Inventory::new(container.entity));
        self.containers.insert(container.entity, container);
        Ok(())
    }

    pub fn combatant(&self, entity: EntityId) -> Option<&Combatant> {
        self.combatants.get(&entity)
    }

    pub fn player_combatant(&self) -> Option<&Combatant> {
        self.combatant(self.player)
    }

    pub fn add_combatant(&mut self, combatant: Combatant) -> Result<(), GameplayBuildError> {
        if !self.entities.contains_key(&combatant.entity) {
            return Err(GameplayBuildError::MissingEntity(combatant.entity));
        }
        if combatant.max_health <= 0
            || combatant.health <= 0
            || combatant.health > combatant.max_health
            || combatant.armor < 0
        {
            return Err(GameplayBuildError::InvalidCombatant(combatant.entity));
        }
        self.combatants.insert(combatant.entity, combatant);
        Ok(())
    }

    pub fn register_healer(&mut self, entity: EntityId) -> Result<(), GameplayBuildError> {
        if !self.entities.contains_key(&entity) {
            return Err(GameplayBuildError::MissingEntity(entity));
        }
        self.healers.insert(entity);
        Ok(())
    }

    pub fn add_transition(&mut self, transition: Transition) -> Result<(), GameplayBuildError> {
        if self.transitions.contains_key(&transition.from) {
            return Err(GameplayBuildError::DuplicateTransition(transition.from));
        }
        let valid = [transition.from, transition.to]
            .into_iter()
            .all(|position| {
                self.map(position.map)
                    .and_then(|map| map.cell(position.grid))
                    .is_some_and(|cell| !cell.movement_blocked)
            });
        if !valid {
            return Err(GameplayBuildError::InvalidTransition(transition.from));
        }
        self.transitions.insert(transition.from, transition);
        Ok(())
    }

    pub fn add_quest(&mut self, quest: Quest) -> Result<(), GameplayBuildError> {
        if self.quests.contains_key(&quest.id) {
            return Err(GameplayBuildError::DuplicateQuest(quest.id));
        }
        if !self.entities.contains_key(&quest.giver) {
            return Err(GameplayBuildError::MissingEntity(quest.giver));
        }
        for objective in &quest.objectives {
            match objective.kind {
                QuestObjectiveKind::Defeat(entity) if !self.entities.contains_key(&entity) => {
                    return Err(GameplayBuildError::MissingEntity(entity));
                }
                QuestObjectiveKind::Recover(item) if !self.items.contains_key(&item) => {
                    return Err(GameplayBuildError::MissingItem(item));
                }
                QuestObjectiveKind::Defeat(_) | QuestObjectiveKind::Recover(_) => {}
            }
        }
        self.quests.insert(quest.id, quest);
        Ok(())
    }

    pub fn give_item(&mut self, owner: EntityId, item: Item) -> Result<(), GameplayBuildError> {
        self.give_item_with_owner(owner, owner, item)
    }

    pub fn give_item_with_owner(
        &mut self,
        custodian: EntityId,
        legal_owner: EntityId,
        item: Item,
    ) -> Result<(), GameplayBuildError> {
        if !self.entities.contains_key(&custodian) {
            return Err(GameplayBuildError::MissingEntity(custodian));
        }
        if !self.entities.contains_key(&legal_owner) {
            return Err(GameplayBuildError::MissingEntity(legal_owner));
        }
        if self.items.contains_key(&item.id) {
            return Err(GameplayBuildError::DuplicateItem(item.id));
        }
        if let Some(container) = self.container(custodian) {
            let current = self.container_weight(custodian).unwrap_or_default();
            if current.saturating_add(item.weight_grams) > container.capacity_grams {
                return Err(GameplayBuildError::InvalidContainer(custodian));
            }
        }
        let item_id = item.id;
        self.items.insert(item_id, item);
        self.legal_owners.insert(item_id, legal_owner);
        self.inventories
            .entry(custodian)
            .or_insert_with(|| Inventory::new(custodian))
            .items
            .insert(item_id);
        Ok(())
    }

    pub fn transfer_item(
        &mut self,
        item: ItemId,
        from: EntityId,
        to: EntityId,
    ) -> Result<SimulationEvent, GameplayBuildError> {
        if !self.entities.contains_key(&to) {
            return Err(GameplayBuildError::MissingEntity(to));
        }
        if !self.items.contains_key(&item) {
            return Err(GameplayBuildError::MissingItem(item));
        }
        if let Some(container) = self.container(to) {
            let current = self.container_weight(to).unwrap_or_default();
            if current.saturating_add(self.items[&item].weight_grams) > container.capacity_grams {
                return Err(GameplayBuildError::ContainerFull(to));
            }
        }
        let from_inventory = self
            .inventories
            .get_mut(&from)
            .ok_or(GameplayBuildError::ItemNotCarried { item, owner: from })?;
        if !from_inventory.items.remove(&item) {
            return Err(GameplayBuildError::ItemNotCarried { item, owner: from });
        }
        if from_inventory.equipped_melee == Some(item) {
            from_inventory.equipped_melee = None;
        }
        if from_inventory.equipped_ranged == Some(item) {
            from_inventory.equipped_ranged = None;
        }
        self.inventories
            .entry(to)
            .or_insert_with(|| Inventory::new(to))
            .items
            .insert(item);
        Ok(SimulationEvent::ItemTransferred { item, from, to })
    }

    /// Transfers part of a divisible stack while preserving the source stack's
    /// legal title and stolen status. A partial transfer receives a deterministic
    /// item identity so command replay reconstructs exactly the same inventories.
    pub fn transfer_item_quantity(
        &mut self,
        item: ItemId,
        from: EntityId,
        to: EntityId,
        quantity: u16,
    ) -> Result<SimulationEvent, GameplayBuildError> {
        if !self.entities.contains_key(&to) {
            return Err(GameplayBuildError::MissingEntity(to));
        }
        let Some(source) = self.items.get(&item).cloned() else {
            return Err(GameplayBuildError::MissingItem(item));
        };
        if quantity == 0 || quantity > source.quantity {
            return Err(GameplayBuildError::InvalidQuantity { item, quantity });
        }
        if !self
            .inventories
            .get(&from)
            .is_some_and(|inventory| inventory.items.contains(&item))
        {
            return Err(GameplayBuildError::ItemNotCarried { item, owner: from });
        }
        let moved_weight = stack_weight_for_quantity(&source, quantity);
        if let Some(container) = self.container(to) {
            let current = self.container_weight(to).unwrap_or_default();
            if current.saturating_add(moved_weight) > container.capacity_grams {
                return Err(GameplayBuildError::ContainerFull(to));
            }
        }

        let moved_item = if quantity == source.quantity {
            self.inventories
                .get_mut(&from)
                .expect("source inventory checked above")
                .items
                .remove(&item);
            let from_inventory = self
                .inventories
                .get_mut(&from)
                .expect("source inventory checked above");
            if from_inventory.equipped_melee == Some(item) {
                from_inventory.equipped_melee = None;
            }
            if from_inventory.equipped_ranged == Some(item) {
                from_inventory.equipped_ranged = None;
            }
            item
        } else {
            let moved_item = self.allocate_split_item_id();
            let mut split = source;
            split.id = moved_item;
            split.quantity = quantity;
            split.weight_grams = moved_weight;
            let source = self
                .items
                .get_mut(&item)
                .expect("source item checked above");
            source.quantity -= quantity;
            source.weight_grams = source.weight_grams.saturating_sub(moved_weight);
            self.items.insert(moved_item, split);
            if let Some(owner) = self.legal_owner(item) {
                self.legal_owners.insert(moved_item, owner);
            }
            if self.is_stolen(item) {
                self.stolen_items.insert(moved_item);
            }
            moved_item
        };
        self.inventories
            .entry(to)
            .or_insert_with(|| Inventory::new(to))
            .items
            .insert(moved_item);
        Ok(SimulationEvent::ItemQuantityTransferred {
            source: item,
            item: moved_item,
            quantity,
            from,
            to,
        })
    }

    /// Coalesces a compatible divisible stack already held by `custodian`.
    /// The lowest item identity survives, keeping repeated transfers stable and
    /// preventing inventories from filling with one-unit fragments.
    pub fn merge_compatible_stack(&mut self, custodian: EntityId, item: ItemId) -> ItemId {
        let Some(item_state) = self.items.get(&item).cloned() else {
            return item;
        };
        if !item_kind_is_stackable(item_state.kind)
            || !self
                .inventory(custodian)
                .is_some_and(|inventory| inventory.items.contains(&item))
        {
            return item;
        }
        let legal_owner = self.legal_owner(item);
        let stolen = self.is_stolen(item);
        let compatible = self
            .inventory(custodian)
            .into_iter()
            .flat_map(|inventory| inventory.items.iter().copied())
            .filter(|candidate| *candidate != item)
            .filter(|candidate| {
                self.items.get(candidate).is_some_and(|candidate_state| {
                    item_kind_is_stackable(candidate_state.kind)
                        && candidate_state.name == item_state.name
                        && candidate_state.kind == item_state.kind
                        && candidate_state.quality == item_state.quality
                        && self.legal_owner(*candidate) == legal_owner
                        && self.is_stolen(*candidate) == stolen
                })
            })
            .min();
        let Some(other) = compatible else {
            return item;
        };
        let (survivor, consumed) = if item < other {
            (item, other)
        } else {
            (other, item)
        };
        let consumed_state = self
            .items
            .remove(&consumed)
            .expect("compatible stack exists");
        let survivor_state = self
            .items
            .get_mut(&survivor)
            .expect("compatible survivor exists");
        survivor_state.quantity = survivor_state
            .quantity
            .saturating_add(consumed_state.quantity);
        survivor_state.weight_grams = survivor_state
            .weight_grams
            .saturating_add(consumed_state.weight_grams);
        self.inventories
            .get_mut(&custodian)
            .expect("custodian inventory checked above")
            .items
            .remove(&consumed);
        self.legal_owners.remove(&consumed);
        self.stolen_items.remove(&consumed);
        self.read_items.remove(&consumed);
        survivor
    }

    fn allocate_split_item_id(&mut self) -> ItemId {
        loop {
            let candidate = ItemId(self.next_split_item_id);
            self.next_split_item_id = self.next_split_item_id.saturating_add(1);
            if !self.items.contains_key(&candidate) {
                return candidate;
            }
        }
    }

    pub fn hostile_in_melee_range(&self) -> Option<EntityId> {
        let player = self.player().position;
        self.living_hostiles()
            .filter(|entity| {
                entity.position.map == player.map && entity.position.grid.z == player.grid.z
            })
            .filter_map(|entity| {
                let distance = grid_distance(player.grid, entity.position.grid);
                (distance <= 1).then_some((distance, entity.id))
            })
            .min()
            .map(|(_, entity)| entity)
    }

    pub fn hostile_in_ranged_line(&self) -> Option<EntityId> {
        let inventory = self.player_inventory()?;
        let weapon = self.item(inventory.equipped_ranged?)?;
        let ItemKind::RangedWeapon { range, .. } = weapon.kind else {
            return None;
        };
        let player = self.player().position;
        self.living_hostiles()
            .filter(|entity| {
                entity.position.map == player.map && entity.position.grid.z == player.grid.z
            })
            .filter_map(|entity| {
                let distance = ranged_distance(player.grid, entity.position.grid);
                (distance <= i32::from(range) && self.clear_line(player, entity.position))
                    .then_some((distance, entity.id))
            })
            .min()
            .map(|(_, entity)| entity)
    }

    fn living_hostiles(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values().filter(|entity| {
            self.combatants
                .get(&entity.id)
                .is_some_and(|combatant| combatant.hostile_to_player && combatant.is_alive())
        })
    }

    pub fn equipped_ranged_range(&self, owner: EntityId) -> Option<u8> {
        let inventory = self.inventory(owner)?;
        let item = self.item(inventory.equipped_ranged?)?;
        match item.kind {
            ItemKind::RangedWeapon { range, .. } => Some(range),
            _ => None,
        }
    }

    pub fn check_ranged_attack(
        &self,
        attacker: EntityId,
        target: EntityId,
    ) -> Result<(), ActionFailure> {
        self.ranged_attack_plan(attacker, target).map(|_| ())
    }

    pub fn apply_command(&mut self, command: GameCommand) -> CommandOutcome {
        if command == GameCommand::Pause {
            self.paused = !self.paused;
            return CommandOutcome {
                advanced_time: false,
                changed_world: true,
                events: Vec::new(),
            };
        }

        if self.paused
            && !matches!(
                command,
                GameCommand::Equip(_)
                    | GameCommand::UseItem(_)
                    | GameCommand::Study(_)
                    | GameCommand::Give { .. }
                    | GameCommand::GiveQuantity { .. }
                    | GameCommand::Take { .. }
                    | GameCommand::TakeQuantity { .. }
                    | GameCommand::OpenContainer(_)
                    | GameCommand::UnlockContainer { .. }
                    | GameCommand::Place { .. }
                    | GameCommand::PlaceQuantity { .. }
                    | GameCommand::Drop(_)
                    | GameCommand::DropQuantity { .. }
                    | GameCommand::Read(_)
                    | GameCommand::Eat(_)
                    | GameCommand::Drink(_)
                    | GameCommand::Experiment { .. }
            )
        {
            return CommandOutcome::default();
        }

        let mut outcome = match command {
            GameCommand::Move(direction) => self.resolve_move(direction),
            GameCommand::Attack(target) => self.resolve_melee(self.player, target),
            GameCommand::FireAt(target) => self.resolve_ranged(self.player, target),
            GameCommand::Equip(item) => self.resolve_equip(self.player, item),
            GameCommand::UseItem(item) => self.resolve_use_item(self.player, item),
            GameCommand::Study(item) => self.resolve_study(item),
            GameCommand::Give { item, to } => {
                self.resolve_give(item, to, self.stack_quantity(item))
            }
            GameCommand::GiveQuantity { item, to, quantity } => {
                self.resolve_give(item, to, quantity)
            }
            GameCommand::Take { item, from } => {
                self.resolve_take(item, from, self.stack_quantity(item))
            }
            GameCommand::TakeQuantity {
                item,
                from,
                quantity,
            } => self.resolve_take(item, from, quantity),
            GameCommand::OpenContainer(container) => self.resolve_open_container(container),
            GameCommand::UnlockContainer { container, key } => {
                self.resolve_unlock_container(container, key)
            }
            GameCommand::Place { item, container } => {
                self.resolve_place(item, container, self.stack_quantity(item))
            }
            GameCommand::PlaceQuantity {
                item,
                container,
                quantity,
            } => self.resolve_place(item, container, quantity),
            GameCommand::Drop(item) => self.resolve_drop(item, self.stack_quantity(item)),
            GameCommand::DropQuantity { item, quantity } => self.resolve_drop(item, quantity),
            GameCommand::Read(item) => self.resolve_read(item),
            GameCommand::Eat(item) => self.resolve_eat(item),
            GameCommand::Drink(item) => self.resolve_drink(item),
            GameCommand::Experiment { first, second } => self.resolve_experiment(first, second),
            GameCommand::Cast { formula, target } => self.resolve_cast(formula, target),
            GameCommand::Traverse => self.resolve_traverse(),
            GameCommand::TurnInQuest(quest) => self.resolve_quest_turn_in(quest),
            GameCommand::Wait => {
                self.tick += 1;
                CommandOutcome {
                    advanced_time: true,
                    changed_world: false,
                    events: Vec::new(),
                }
            }
            GameCommand::Pause => unreachable!("pause handled above"),
        };
        if outcome.advanced_time {
            self.advance_player_needs(&mut outcome.events);
            outcome.changed_world |= self.advance_hostiles(&mut outcome.events);
            self.revive_player_at_nearest_healer(&mut outcome.events);
        }
        outcome
    }

    fn resolve_move(&mut self, direction: Direction) -> CommandOutcome {
        let current = self.player().position;
        let (dx, dy) = direction.delta();
        let destination = current.grid.offset(dx, dy, 0);
        let blocking_hostile = {
            self.living_hostiles()
                .find(|entity| {
                    entity.position.map == current.map && entity.position.grid == destination
                })
                .map(|entity| entity.id)
        };
        if let Some(target) = blocking_hostile {
            self.entities
                .get_mut(&self.player)
                .expect("player entity must exist")
                .facing = direction;
            return self.resolve_melee(self.player, target);
        }
        let can_enter = self
            .map(current.map)
            .and_then(|map| map.cell(destination))
            .is_some_and(|cell| !cell.movement_blocked);

        if !can_enter {
            return CommandOutcome::default();
        }

        let automatic_transition = self
            .transitions
            .get(&WorldPosition {
                map: current.map,
                grid: destination,
            })
            .filter(|transition| transition.from.map != transition.to.map)
            .cloned();
        let player = self
            .entities
            .get_mut(&self.player)
            .expect("player entity must exist");
        player.position = automatic_transition.as_ref().map_or(
            WorldPosition {
                map: current.map,
                grid: destination,
            },
            |transition| transition.to,
        );
        player.facing = direction;
        let player_map = player.position.map;
        self.tick += 1;
        let mut events = automatic_transition
            .map(|transition| {
                vec![SimulationEvent::Traversed {
                    kind: transition.kind,
                    destination: transition.to,
                }]
            })
            .unwrap_or_default();
        self.record_map_discovery(player_map, &mut events);
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events,
        }
    }

    fn resolve_traverse(&mut self) -> CommandOutcome {
        let current = self.player().position;
        let Some(transition) = self.transitions.get(&current).cloned() else {
            return failed(ActionFailure::NoTransition);
        };
        self.entities
            .get_mut(&self.player)
            .expect("player entity must exist")
            .position = transition.to;
        self.tick += 1;
        let mut events = vec![SimulationEvent::Traversed {
            kind: transition.kind,
            destination: transition.to,
        }];
        self.record_map_discovery(transition.to.map, &mut events);
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events,
        }
    }

    fn resolve_quest_turn_in(&mut self, quest_id: QuestId) -> CommandOutcome {
        let Some(quest) = self.quests.get(&quest_id) else {
            return failed(ActionFailure::QuestNotReady);
        };
        let giver_id = quest.giver;
        let recovered_items = quest
            .objectives
            .iter()
            .filter_map(|objective| match objective.kind {
                QuestObjectiveKind::Recover(item) => Some(item),
                QuestObjectiveKind::Defeat(_) => None,
            })
            .collect::<Vec<_>>();
        let player = self.player().position;
        let Some(giver) = self.entities.get(&giver_id).map(|entity| entity.position) else {
            return failed(ActionFailure::QuestNotReady);
        };
        if quest.status != QuestStatus::ReadyToTurnIn
            || player.map != giver.map
            || player.grid.z != giver.grid.z
            || grid_distance(player.grid, giver.grid) > 1
        {
            return failed(ActionFailure::QuestNotReady);
        }
        let reward = quest.reward_experience;
        self.quests
            .get_mut(&quest_id)
            .expect("quest checked above")
            .status = QuestStatus::Completed;
        self.progression.fulfilled_commitments =
            self.progression.fulfilled_commitments.saturating_add(1);
        let mut events = vec![SimulationEvent::QuestCompleted { quest: quest_id }];
        for item in recovered_items {
            if self
                .inventory(self.player)
                .is_some_and(|inventory| inventory.items.contains(&item))
                && let Ok(event) = self.transfer_item(item, self.player, giver_id)
            {
                events.push(event);
            }
        }
        self.grant_experience(reward, &mut events);
        CommandOutcome {
            advanced_time: false,
            changed_world: true,
            events,
        }
    }

    fn resolve_equip(&mut self, owner: EntityId, item: ItemId) -> CommandOutcome {
        let Some(inventory) = self.inventories.get(&owner) else {
            return failed(ActionFailure::ItemNotCarried);
        };
        if !inventory.items.contains(&item) {
            return failed(ActionFailure::ItemNotCarried);
        }
        let Some(item_kind) = self.items.get(&item).map(|item| item.kind) else {
            return failed(ActionFailure::ItemNotCarried);
        };
        let inventory = self
            .inventories
            .get_mut(&owner)
            .expect("inventory checked above");
        match item_kind {
            ItemKind::MeleeWeapon { .. } => inventory.equipped_melee = Some(item),
            ItemKind::RangedWeapon { .. } => inventory.equipped_ranged = Some(item),
            ItemKind::Ammunition { .. }
            | ItemKind::Consumable { .. }
            | ItemKind::Food { .. }
            | ItemKind::Drink { .. }
            | ItemKind::Book { .. }
            | ItemKind::Key { .. }
            | ItemKind::Tool
            | ItemKind::Reagent { .. }
            | ItemKind::InscribedArtifact { .. }
            | ItemKind::Artifact => {
                return failed(ActionFailure::ItemCannotBeUsed);
            }
        }
        CommandOutcome {
            advanced_time: false,
            changed_world: true,
            events: vec![SimulationEvent::ItemEquipped { owner, item }],
        }
    }

    fn resolve_use_item(&mut self, owner: EntityId, item: ItemId) -> CommandOutcome {
        let carried = self
            .inventories
            .get(&owner)
            .is_some_and(|inventory| inventory.items.contains(&item));
        if !carried {
            return failed(ActionFailure::ItemNotCarried);
        }
        let Some(ItemKind::Consumable { healing }) = self.items.get(&item).map(|item| item.kind)
        else {
            return failed(ActionFailure::ItemCannotBeUsed);
        };
        if self.items[&item].quantity == 0 {
            return failed(ActionFailure::ItemCannotBeUsed);
        }
        let Some(combatant) = self.combatants.get_mut(&owner) else {
            return failed(ActionFailure::InvalidTarget);
        };
        if combatant.health >= combatant.max_health {
            return failed(ActionFailure::AlreadyAtFullHealth);
        }
        let previous = combatant.health;
        combatant.health = (combatant.health + healing).min(combatant.max_health);
        let healed = combatant.health - previous;
        let item = self.items.get_mut(&item).expect("item checked above");
        consume_one_unit(item);
        CommandOutcome {
            advanced_time: false,
            changed_world: true,
            events: vec![
                SimulationEvent::Healed {
                    entity: owner,
                    amount: healed,
                    health: combatant.health,
                },
                SimulationEvent::ItemConsumed {
                    owner,
                    item: item.id,
                    remaining: item.quantity,
                },
            ],
        }
    }

    fn resolve_study(&mut self, item: ItemId) -> CommandOutcome {
        let carried = self
            .player_inventory()
            .is_some_and(|inventory| inventory.items.contains(&item));
        if !carried {
            return failed(ActionFailure::ItemNotCarried);
        }
        let Some(ItemKind::InscribedArtifact { formula, .. }) =
            self.items.get(&item).map(|item| item.kind)
        else {
            return failed(ActionFailure::ItemCannotBeUsed);
        };
        if !self.known_formulas.insert(formula) {
            return CommandOutcome::default();
        }
        self.progression.discoveries = self.progression.discoveries.saturating_add(1);
        self.progression.arcane_lore = self.progression.arcane_lore.saturating_add(1);
        let mut events = vec![SimulationEvent::FormulaLearned {
            formula,
            source: item,
        }];
        self.grant_experience(30, &mut events);
        CommandOutcome {
            advanced_time: false,
            changed_world: true,
            events,
        }
    }

    fn resolve_give(&mut self, item: ItemId, to: EntityId, quantity: u16) -> CommandOutcome {
        if !self.entities_are_adjacent(self.player, to) {
            return failed(ActionFailure::OutOfRange);
        }
        let player_holds_title =
            self.legal_owner(item) == Some(self.player) && !self.is_stolen(item);
        let mut event = match self.transfer_item_quantity(item, self.player, to, quantity) {
            Ok(event) => event,
            Err(GameplayBuildError::InvalidQuantity { .. }) => {
                return failed(ActionFailure::InvalidQuantity);
            }
            Err(GameplayBuildError::ItemNotCarried { .. }) => {
                return failed(ActionFailure::ItemNotCarried);
            }
            Err(_) => return failed(ActionFailure::InvalidTarget),
        };
        let moved_item = quantity_transfer_item(&event);
        if player_holds_title {
            self.transfer_legal_ownership(moved_item, self.player, to);
        } else if self.legal_owner(moved_item) == Some(to) {
            self.stolen_items.remove(&moved_item);
        }
        let moved_item = self.merge_compatible_stack(to, moved_item);
        retarget_quantity_event(&mut event, moved_item);
        self.tick = self.tick.saturating_add(1);
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events: vec![event],
        }
    }

    fn resolve_take(&mut self, item: ItemId, from: EntityId, quantity: u16) -> CommandOutcome {
        if !self.entities_are_adjacent(self.player, from) {
            return failed(ActionFailure::OutOfRange);
        }
        if self
            .container(from)
            .is_some_and(|container| container.locked)
        {
            return failed(ActionFailure::ContainerLocked);
        }
        let mut event = match self.transfer_item_quantity(item, from, self.player, quantity) {
            Ok(event) => event,
            Err(GameplayBuildError::InvalidQuantity { .. }) => {
                return failed(ActionFailure::InvalidQuantity);
            }
            Err(GameplayBuildError::ItemNotCarried { .. }) => {
                return failed(ActionFailure::ItemNotCarried);
            }
            Err(_) => return failed(ActionFailure::InvalidTarget),
        };
        let moved_item = quantity_transfer_item(&event);
        if self.legal_owner(moved_item) != Some(self.player) {
            self.stolen_items.insert(moved_item);
        }
        let moved_item = self.merge_compatible_stack(self.player, moved_item);
        retarget_quantity_event(&mut event, moved_item);
        let remove_empty_holder = self
            .entity(from)
            .is_some_and(|entity| entity.kind == EntityKind::Item)
            && !self.containers.contains_key(&from)
            && self
                .inventory(from)
                .is_none_or(|inventory| inventory.items.is_empty());
        if remove_empty_holder {
            self.remove_entity(from);
        }
        self.tick = self.tick.saturating_add(1);
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events: vec![event],
        }
    }

    fn resolve_open_container(&self, container: EntityId) -> CommandOutcome {
        if !self.entities_are_adjacent(self.player, container) {
            return failed(ActionFailure::OutOfRange);
        }
        let Some(container_state) = self.container(container) else {
            return failed(ActionFailure::NotAContainer);
        };
        if container_state.locked {
            return failed(ActionFailure::ContainerLocked);
        }
        CommandOutcome {
            advanced_time: false,
            changed_world: false,
            events: vec![SimulationEvent::ContainerOpened { container }],
        }
    }

    fn resolve_unlock_container(&mut self, container: EntityId, key: ItemId) -> CommandOutcome {
        if !self.entities_are_adjacent(self.player, container) {
            return failed(ActionFailure::OutOfRange);
        }
        let carried = self
            .player_inventory()
            .is_some_and(|inventory| inventory.items.contains(&key));
        if !carried {
            return failed(ActionFailure::ItemNotCarried);
        }
        let Some(lock_code) = self
            .container(container)
            .and_then(|container| container.locked.then_some(container.lock_code).flatten())
        else {
            return failed(ActionFailure::NotAContainer);
        };
        if self.item(key).map(|item| item.kind) != Some(ItemKind::Key { lock_code }) {
            return failed(ActionFailure::WrongKey);
        }
        self.containers
            .get_mut(&container)
            .expect("container lock checked above")
            .locked = false;
        self.tick = self.tick.saturating_add(1);
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events: vec![SimulationEvent::ContainerUnlocked { container, key }],
        }
    }

    fn resolve_place(
        &mut self,
        item: ItemId,
        container: EntityId,
        quantity: u16,
    ) -> CommandOutcome {
        if !self.entities_are_adjacent(self.player, container) {
            return failed(ActionFailure::OutOfRange);
        }
        let Some(container_state) = self.container(container) else {
            return failed(ActionFailure::NotAContainer);
        };
        if container_state.locked {
            return failed(ActionFailure::ContainerLocked);
        }
        let mut event = match self.transfer_item_quantity(item, self.player, container, quantity) {
            Ok(event) => event,
            Err(GameplayBuildError::InvalidQuantity { .. }) => {
                return failed(ActionFailure::InvalidQuantity);
            }
            Err(GameplayBuildError::ContainerFull(_)) => {
                return failed(ActionFailure::ContainerFull);
            }
            Err(GameplayBuildError::ItemNotCarried { .. }) => {
                return failed(ActionFailure::ItemNotCarried);
            }
            Err(_) => return failed(ActionFailure::InvalidTarget),
        };
        let moved_item = quantity_transfer_item(&event);
        let moved_item = self.merge_compatible_stack(container, moved_item);
        retarget_quantity_event(&mut event, moved_item);
        self.tick = self.tick.saturating_add(1);
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events: vec![event],
        }
    }

    fn resolve_drop(&mut self, item: ItemId, quantity: u16) -> CommandOutcome {
        if !self
            .player_inventory()
            .is_some_and(|inventory| inventory.items.contains(&item))
        {
            return failed(ActionFailure::ItemNotCarried);
        }
        if quantity == 0 || quantity > self.stack_quantity(item) {
            return failed(ActionFailure::InvalidQuantity);
        }
        let holder = loop {
            let candidate = EntityId(self.next_loose_item_entity);
            self.next_loose_item_entity = self.next_loose_item_entity.saturating_add(1);
            if !self.entities.contains_key(&candidate) {
                break candidate;
            }
        };
        let position = self.player().position;
        self.entities.insert(
            holder,
            Entity {
                id: holder,
                kind: EntityKind::Item,
                position,
                facing: Direction::South,
            },
        );
        let event = match self.transfer_item_quantity(item, self.player, holder, quantity) {
            Ok(event) => event,
            Err(GameplayBuildError::InvalidQuantity { .. }) => {
                self.remove_entity(holder);
                return failed(ActionFailure::InvalidQuantity);
            }
            Err(_) => {
                self.remove_entity(holder);
                return failed(ActionFailure::InvalidTarget);
            }
        };
        let moved_item = quantity_transfer_item(&event);
        self.tick = self.tick.saturating_add(1);
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events: vec![
                event,
                SimulationEvent::ItemDropped {
                    item: moved_item,
                    holder,
                    position,
                },
            ],
        }
    }

    fn resolve_read(&mut self, item: ItemId) -> CommandOutcome {
        if !self
            .player_inventory()
            .is_some_and(|inventory| inventory.items.contains(&item))
        {
            return failed(ActionFailure::ItemNotCarried);
        }
        let Some(ItemKind::Book { subject }) = self.item(item).map(|item| item.kind) else {
            return failed(ActionFailure::ItemCannotBeUsed);
        };
        let newly_learned = self.read_items.insert(item);
        if newly_learned {
            self.progression.discoveries = self.progression.discoveries.saturating_add(1);
        }
        self.tick = self.tick.saturating_add(1);
        CommandOutcome {
            advanced_time: true,
            changed_world: newly_learned,
            events: vec![SimulationEvent::ItemRead {
                item,
                subject,
                newly_learned,
            }],
        }
    }

    fn resolve_eat(&mut self, item: ItemId) -> CommandOutcome {
        let Some(ItemKind::Food { nourishment }) = self.carried_item_kind(item) else {
            return failed(if self.item(item).is_some() {
                ActionFailure::ItemCannotBeUsed
            } else {
                ActionFailure::ItemNotCarried
            });
        };
        if self.player_needs.hunger == 0 {
            return failed(ActionFailure::AlreadySatisfied);
        }
        self.player_needs.hunger = self.player_needs.hunger.saturating_sub(nourishment);
        self.consume_carried_item(item)
    }

    fn resolve_drink(&mut self, item: ItemId) -> CommandOutcome {
        let Some(ItemKind::Drink { hydration }) = self.carried_item_kind(item) else {
            return failed(if self.item(item).is_some() {
                ActionFailure::ItemCannotBeUsed
            } else {
                ActionFailure::ItemNotCarried
            });
        };
        if self.player_needs.thirst == 0 {
            return failed(ActionFailure::AlreadySatisfied);
        }
        self.player_needs.thirst = self.player_needs.thirst.saturating_sub(hydration);
        self.consume_carried_item(item)
    }

    fn carried_item_kind(&self, item: ItemId) -> Option<ItemKind> {
        self.player_inventory()
            .is_some_and(|inventory| inventory.items.contains(&item))
            .then(|| self.item(item).map(|item| item.kind))
            .flatten()
    }

    fn stack_quantity(&self, item: ItemId) -> u16 {
        self.item(item)
            .map(|item| item.quantity)
            .unwrap_or_default()
    }

    fn consume_carried_item(&mut self, item: ItemId) -> CommandOutcome {
        let item_state = self.items.get_mut(&item).expect("carried item exists");
        if item_state.quantity == 0 {
            return failed(ActionFailure::ItemCannotBeUsed);
        }
        consume_one_unit(item_state);
        let remaining = item_state.quantity;
        self.tick = self.tick.saturating_add(1);
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events: vec![
                SimulationEvent::ItemConsumed {
                    owner: self.player,
                    item,
                    remaining,
                },
                SimulationEvent::NeedsChanged {
                    hunger: self.player_needs.hunger,
                    thirst: self.player_needs.thirst,
                },
            ],
        }
    }

    fn advance_player_needs(&mut self, events: &mut Vec<SimulationEvent>) {
        if !self.tick.is_multiple_of(12) {
            return;
        }
        self.player_needs.hunger = self.player_needs.hunger.saturating_add(1).min(100);
        self.player_needs.thirst = self.player_needs.thirst.saturating_add(2).min(100);
        events.push(SimulationEvent::NeedsChanged {
            hunger: self.player_needs.hunger,
            thirst: self.player_needs.thirst,
        });
    }

    fn entities_are_adjacent(&self, first: EntityId, second: EntityId) -> bool {
        let Some(first) = self.entity(first) else {
            return false;
        };
        let Some(second) = self.entity(second) else {
            return false;
        };
        first.position.map == second.position.map
            && first.position.grid.z == second.position.grid.z
            && grid_distance(first.position.grid, second.position.grid) <= 1
    }

    fn resolve_experiment(&mut self, first: ItemId, second: ItemId) -> CommandOutcome {
        if first == second {
            return failed(ActionFailure::ExperimentFailed);
        }
        let carried = |item| {
            self.player_inventory()
                .is_some_and(|inventory| inventory.items.contains(&item))
        };
        if !carried(first) || !carried(second) {
            return failed(ActionFailure::ItemNotCarried);
        }
        let reagent = |item| {
            self.items.get(&item).and_then(|item| {
                (item.quantity > 0)
                    .then_some(item.kind)
                    .and_then(|kind| match kind {
                        ItemKind::Reagent { material } => Some(material),
                        _ => None,
                    })
            })
        };
        let Some(first_material) = reagent(first) else {
            return failed(ActionFailure::ItemCannotBeUsed);
        };
        let Some(second_material) = reagent(second) else {
            return failed(ActionFailure::ItemCannotBeUsed);
        };
        let materials = BTreeSet::from([first_material, second_material]);
        let formula = self
            .rules
            .formulas
            .iter()
            .find(|formula| {
                formula.reagents.iter().copied().collect::<BTreeSet<_>>() == materials
                    && self.magical_condition_met(formula.condition)
            })
            .cloned();

        let mut events = Vec::new();
        for item in [first, second] {
            let reagent = self.items.get_mut(&item).expect("experiment item checked");
            consume_one_unit(reagent);
            events.push(SimulationEvent::ItemConsumed {
                owner: self.player,
                item,
                remaining: reagent.quantity,
            });
        }
        self.tick = self.tick.saturating_add(1);
        let Some(formula) = formula else {
            events.push(SimulationEvent::ActionFailed(
                ActionFailure::ExperimentFailed,
            ));
            return CommandOutcome {
                advanced_time: true,
                changed_world: true,
                events,
            };
        };
        if self.known_formulas.insert(formula.id) {
            self.progression.discoveries = self.progression.discoveries.saturating_add(1);
            self.progression.arcane_lore = self.progression.arcane_lore.saturating_add(1);
            self.progression.magical_practice = self.progression.magical_practice.saturating_add(1);
            events.push(SimulationEvent::FormulaLearned {
                formula: formula.id,
                source: first,
            });
            self.grant_experience(30, &mut events);
        }
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events,
        }
    }

    fn resolve_cast(&mut self, formula: FormulaId, target: Option<EntityId>) -> CommandOutcome {
        if !self.known_formulas.contains(&formula) {
            return failed(ActionFailure::UnknownFormula);
        }
        let Some(rule) = self.rules.formula(formula).cloned() else {
            return failed(ActionFailure::UnknownFormula);
        };
        if !self.magical_condition_met(rule.condition) {
            return failed(ActionFailure::MagicalConditionUnmet(rule.condition));
        }
        let mut reagent_items = Vec::new();
        for material in &rule.reagents {
            let Some(item) = self.player_inventory().and_then(|inventory| {
                inventory.items.iter().copied().find(|item| {
                    self.items.get(item).is_some_and(|item| {
                        item.quantity > 0
                            && item.kind
                                == ItemKind::Reagent {
                                    material: *material,
                                }
                    })
                })
            }) else {
                return failed(ActionFailure::MissingReagent(*material));
            };
            reagent_items.push(item);
        }

        let target = match rule.effect {
            MagicEffect::Heal => self.player,
            MagicEffect::Kindle => {
                let Some(target) = target else {
                    return failed(ActionFailure::InvalidTarget);
                };
                let Some((caster_position, target_position)) =
                    self.combat_positions(self.player, target)
                else {
                    return failed(ActionFailure::InvalidTarget);
                };
                if caster_position.map != target_position.map
                    || caster_position.grid.z != target_position.grid.z
                    || ranged_distance(caster_position.grid, target_position.grid) > 5
                {
                    return failed(ActionFailure::OutOfRange);
                }
                if !self.clear_line(caster_position, target_position) {
                    return failed(ActionFailure::LineBlocked);
                }
                target
            }
        };

        let mut events = Vec::new();
        for item in reagent_items {
            let reagent = self.items.get_mut(&item).expect("reagent was checked");
            consume_one_unit(reagent);
            events.push(SimulationEvent::ItemConsumed {
                owner: self.player,
                item,
                remaining: reagent.quantity,
            });
        }
        events.push(SimulationEvent::SpellCast {
            caster: self.player,
            formula,
            effect: rule.effect,
        });
        match rule.effect {
            MagicEffect::Heal => {
                let combatant = self
                    .combatants
                    .get_mut(&self.player)
                    .expect("player progression requires a combatant");
                let previous = combatant.health;
                combatant.health =
                    (combatant.health + i16::from(rule.potency)).min(combatant.max_health);
                events.push(SimulationEvent::Healed {
                    entity: self.player,
                    amount: combatant.health - previous,
                    health: combatant.health,
                });
            }
            MagicEffect::Kindle => {
                self.deal_damage(
                    self.player,
                    target,
                    i16::from(rule.potency) + self.progression.arcane_lore as i16,
                    CombatMethod::Magic,
                    &mut events,
                );
                self.resolve_defeat_rewards(self.player, target, &mut events);
                self.advance_quests_from_events(&mut events);
            }
        }
        self.progression.magical_practice = self.progression.magical_practice.saturating_add(1);
        self.tick += 1;
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events,
        }
    }

    fn magical_condition_met(&self, condition: FormulaCondition) -> bool {
        let position = self.player().position;
        match condition {
            FormulaCondition::CleanWater => [
                position.grid,
                position.grid.offset(1, 0, 0),
                position.grid.offset(-1, 0, 0),
                position.grid.offset(0, 1, 0),
                position.grid.offset(0, -1, 0),
            ]
            .into_iter()
            .any(|grid| {
                self.map(position.map)
                    .and_then(|map| map.cell(grid))
                    .is_some_and(|cell| {
                        matches!(cell.terrain, TerrainKind::Water | TerrainKind::Ocean)
                    })
            }),
            FormulaCondition::DirectSunlight => position.grid.z == 0 && self.tick % 24 < 12,
            FormulaCondition::NightSky => position.grid.z == 0 && self.tick % 24 >= 12,
            FormulaCondition::ExistingFlame => self.player_inventory().is_some_and(|inventory| {
                inventory.items.iter().any(|item| {
                    self.items.get(item).is_some_and(|item| {
                        item.quantity > 0
                            && item.kind
                                == ItemKind::Reagent {
                                    material: MaterialKind::Emberseed,
                                }
                    })
                })
            }),
        }
    }

    fn resolve_melee(&mut self, attacker: EntityId, target: EntityId) -> CommandOutcome {
        let Some((attacker_position, target_position)) = self.combat_positions(attacker, target)
        else {
            return failed(ActionFailure::InvalidTarget);
        };
        if attacker_position.map != target_position.map
            || attacker_position.grid.z != target_position.grid.z
            || grid_distance(attacker_position.grid, target_position.grid) > 1
        {
            return failed(ActionFailure::OutOfRange);
        }
        let damage = self
            .inventory(attacker)
            .and_then(|inventory| inventory.equipped_melee)
            .and_then(|item| self.item(item))
            .and_then(|item| match item.kind {
                ItemKind::MeleeWeapon { damage } => Some(damage),
                _ => None,
            })
            .unwrap_or(1)
            + if attacker == self.player {
                self.progression.attack_bonus
            } else {
                0
            };
        let mut events = Vec::new();
        self.deal_damage(attacker, target, damage, CombatMethod::Melee, &mut events);
        if attacker == self.player {
            self.progression.martial_practice = self.progression.martial_practice.saturating_add(1);
        }
        self.resolve_defeat_rewards(attacker, target, &mut events);
        self.advance_quests_from_events(&mut events);
        self.tick += 1;
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events,
        }
    }

    fn resolve_ranged(&mut self, attacker: EntityId, target: EntityId) -> CommandOutcome {
        let (mut damage, ammunition_item) = match self.ranged_attack_plan(attacker, target) {
            Ok(plan) => plan,
            Err(reason) => return failed(reason),
        };
        if attacker == self.player {
            damage += self.progression.attack_bonus;
        }
        let ammunition = self
            .items
            .get_mut(&ammunition_item)
            .expect("ammunition selected from items");
        consume_one_unit(ammunition);
        let remaining = ammunition.quantity;
        let mut events = vec![SimulationEvent::ItemConsumed {
            owner: attacker,
            item: ammunition_item,
            remaining,
        }];
        self.deal_damage(attacker, target, damage, CombatMethod::Ranged, &mut events);
        if attacker == self.player {
            self.progression.ranged_practice = self.progression.ranged_practice.saturating_add(1);
        }
        self.resolve_defeat_rewards(attacker, target, &mut events);
        self.advance_quests_from_events(&mut events);
        self.tick += 1;
        CommandOutcome {
            advanced_time: true,
            changed_world: true,
            events,
        }
    }

    fn ranged_attack_plan(
        &self,
        attacker: EntityId,
        target: EntityId,
    ) -> Result<(i16, ItemId), ActionFailure> {
        let (attacker_position, target_position) = self
            .combat_positions(attacker, target)
            .ok_or(ActionFailure::InvalidTarget)?;
        let inventory = self.inventory(attacker).ok_or(ActionFailure::NoWeapon)?;
        let weapon = inventory
            .equipped_ranged
            .and_then(|item| self.items.get(&item))
            .ok_or(ActionFailure::NoWeapon)?;
        let ItemKind::RangedWeapon {
            damage,
            range,
            ammunition,
        } = weapon.kind
        else {
            return Err(ActionFailure::NoWeapon);
        };
        let distance = ranged_distance(attacker_position.grid, target_position.grid);
        if attacker_position.map != target_position.map
            || attacker_position.grid.z != target_position.grid.z
            || distance > i32::from(range)
        {
            return Err(ActionFailure::OutOfRange);
        }
        if !self.clear_line(attacker_position, target_position) {
            return Err(ActionFailure::LineBlocked);
        }
        let ammunition_item = inventory.items.iter().copied().find(|item| {
            self.items.get(item).is_some_and(|item| {
                item.quantity > 0 && item.kind == ItemKind::Ammunition { kind: ammunition }
            })
        });
        ammunition_item
            .map(|ammunition_item| (damage, ammunition_item))
            .ok_or(ActionFailure::NoAmmunition)
    }

    fn resolve_defeat_rewards(
        &mut self,
        attacker: EntityId,
        target: EntityId,
        events: &mut Vec<SimulationEvent>,
    ) {
        let defeated = events.iter().any(|event| {
            matches!(
                event,
                SimulationEvent::Defeated {
                    entity,
                    by
                } if *entity == target && *by == attacker
            )
        });
        if !defeated || attacker != self.player {
            return;
        }

        let reward = self
            .combatants
            .get(&target)
            .map(|combatant| combatant.experience_reward)
            .unwrap_or_default();
        if reward > 0 {
            self.grant_experience(reward, events);
        }
        let loot = self
            .inventories
            .get(&target)
            .map(|inventory| inventory.items.iter().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        for item in loot {
            if let Ok(event) = self.transfer_item(item, target, attacker) {
                events.push(event);
            }
        }
    }

    fn grant_experience(&mut self, amount: u32, events: &mut Vec<SimulationEvent>) {
        if amount == 0 {
            return;
        }
        self.progression.experience = self.progression.experience.saturating_add(amount);
        events.push(SimulationEvent::ExperienceGained {
            amount,
            total: self.progression.experience,
        });
        while self.progression.experience >= self.progression.experience_for_next_level() {
            self.progression.level = self.progression.level.saturating_add(1);
            self.progression.attack_bonus = self.progression.attack_bonus.saturating_add(1);
            let combatant = self
                .combatants
                .get_mut(&self.player)
                .expect("player progression requires a combatant");
            combatant.max_health = combatant.max_health.saturating_add(2);
            combatant.health = combatant.max_health;
            events.push(SimulationEvent::LevelGained {
                level: self.progression.level,
                max_health: combatant.max_health,
                attack_bonus: self.progression.attack_bonus,
            });
        }
    }

    fn record_map_discovery(&mut self, map: MapId, events: &mut Vec<SimulationEvent>) {
        if !self.visited_maps.insert(map) {
            return;
        }
        self.progression.exploration = self.progression.exploration.saturating_add(1);
        self.progression.discoveries = self.progression.discoveries.saturating_add(1);
        self.grant_experience(10, events);
    }

    fn advance_quests_from_events(&mut self, events: &mut Vec<SimulationEvent>) {
        let source_events = events.clone();
        let player = self.player;
        let mut quest_events = Vec::new();
        for quest in self.quests.values_mut() {
            if quest.status != QuestStatus::Active {
                continue;
            }
            for (index, objective) in quest.objectives.iter_mut().enumerate() {
                if objective.completed {
                    continue;
                }
                let completed = match objective.kind {
                    QuestObjectiveKind::Defeat(target) => source_events.iter().any(|event| {
                        matches!(
                            event,
                            SimulationEvent::Defeated { entity, by }
                                if *entity == target && *by == player
                        )
                    }),
                    QuestObjectiveKind::Recover(target) => {
                        source_events.iter().any(|event| match event {
                            SimulationEvent::ItemTransferred { item, to, .. } => {
                                *item == target && *to == player
                            }
                            SimulationEvent::ItemQuantityTransferred { source, to, .. } => {
                                *source == target && *to == player
                            }
                            _ => false,
                        })
                    }
                };
                if completed {
                    objective.completed = true;
                    quest_events.push(SimulationEvent::QuestAdvanced {
                        quest: quest.id,
                        objective: index,
                    });
                }
            }
            if quest.objectives.iter().all(|objective| objective.completed) {
                quest.status = QuestStatus::ReadyToTurnIn;
                quest_events.push(SimulationEvent::QuestReadyToTurnIn { quest: quest.id });
            }
        }
        events.extend(quest_events);
    }

    fn combat_positions(
        &self,
        attacker: EntityId,
        target: EntityId,
    ) -> Option<(WorldPosition, WorldPosition)> {
        let attacker_combatant = self.combatants.get(&attacker)?;
        let target_combatant = self.combatants.get(&target)?;
        if !attacker_combatant.is_alive() || !target_combatant.is_alive() {
            return None;
        }
        Some((
            self.entities.get(&attacker)?.position,
            self.entities.get(&target)?.position,
        ))
    }

    fn advance_hostiles(&mut self, events: &mut Vec<SimulationEvent>) -> bool {
        let hostiles = self
            .living_hostiles()
            .map(|entity| entity.id)
            .collect::<Vec<_>>();
        let mut changed = false;
        // Adjacent enemies share one initiative window per world turn. All of
        // them may pursue, but only one lands a blow before the player receives
        // another action.
        let mut player_attacked = false;
        for hostile in hostiles {
            if self
                .player_combatant()
                .is_none_or(|combatant| !combatant.is_alive())
            {
                break;
            }
            let Some(hostile_position) = self.entity(hostile).map(|entity| entity.position) else {
                continue;
            };
            let player_position = self.player().position;
            if hostile_position.map != player_position.map
                || hostile_position.grid.z != player_position.grid.z
            {
                continue;
            }
            let distance = grid_distance(hostile_position.grid, player_position.grid);
            if distance <= 1 && !player_attacked {
                self.deal_damage(hostile, self.player, 2, CombatMethod::Retaliation, events);
                player_attacked = true;
                changed = true;
            } else if distance <= HOSTILE_NOTICE_RANGE
                && let Some(next) = self.hostile_step_toward(hostile, player_position.grid)
            {
                changed |= self.move_entity(
                    hostile,
                    WorldPosition {
                        map: hostile_position.map,
                        grid: next,
                    },
                );
            }
        }
        changed
    }

    fn hostile_step_toward(&self, hostile: EntityId, goal: GridPos) -> Option<GridPos> {
        let start = self.entity(hostile)?.position;
        let mut occupied = self
            .entities()
            .filter(|entity| entity.id != hostile && entity.position.map == start.map)
            .filter(|entity| entity.position.grid.z == start.grid.z)
            .map(|entity| entity.position.grid)
            .collect::<BTreeSet<_>>();
        occupied.remove(&goal);

        let mut frontier = VecDeque::from([start.grid]);
        let mut previous = BTreeMap::from([(start.grid, start.grid)]);
        while let Some(current) = frontier.pop_front() {
            for direction in Direction::ALL {
                let (dx, dy) = direction.delta();
                let neighbor = current.offset(dx, dy, 0);
                if neighbor == goal
                    || previous.contains_key(&neighbor)
                    || occupied.contains(&neighbor)
                    || self
                        .map(start.map)
                        .and_then(|map| map.cell(neighbor))
                        .is_none_or(|cell| cell.movement_blocked)
                {
                    continue;
                }
                previous.insert(neighbor, current);
                if grid_distance(neighbor, goal) <= 1 {
                    let mut step = neighbor;
                    while previous[&step] != start.grid {
                        step = previous[&step];
                    }
                    return Some(step);
                }
                if grid_distance(start.grid, neighbor) < HOSTILE_NOTICE_RANGE {
                    frontier.push_back(neighbor);
                }
            }
        }
        None
    }

    fn revive_player_at_nearest_healer(&mut self, events: &mut Vec<SimulationEvent>) -> bool {
        if self
            .player_combatant()
            .is_none_or(|combatant| combatant.is_alive())
        {
            return false;
        }
        let defeated_at = self.player().position;
        let healer = self
            .healers
            .iter()
            .filter_map(|healer| {
                self.entity(*healer)
                    .map(|entity| (*healer, entity.position))
            })
            .min_by_key(|(healer, position)| {
                (
                    position.map != defeated_at.map,
                    position.grid.z != defeated_at.grid.z,
                    if position.map == defeated_at.map && position.grid.z == defeated_at.grid.z {
                        grid_distance(position.grid, defeated_at.grid)
                    } else {
                        i32::MAX
                    },
                    *healer,
                )
            });
        let Some((healer, healer_position)) = healer else {
            return false;
        };
        let destination = self.recovery_destination(healer, healer_position);
        let combatant = self
            .combatants
            .get_mut(&self.player)
            .expect("defeated player has a combatant");
        combatant.health = combatant.max_health;
        self.entities
            .get_mut(&self.player)
            .expect("player entity must exist")
            .position = destination;
        self.paused = false;
        events.push(SimulationEvent::RevivedAtHealer {
            entity: self.player,
            healer,
            health: combatant.health,
            destination,
        });
        true
    }

    fn recovery_destination(
        &self,
        healer: EntityId,
        healer_position: WorldPosition,
    ) -> WorldPosition {
        let occupied = self
            .entities()
            .filter(|entity| entity.id != self.player && entity.id != healer)
            .filter(|entity| entity.position.map == healer_position.map)
            .map(|entity| entity.position.grid)
            .collect::<BTreeSet<_>>();
        Direction::ALL
            .into_iter()
            .map(|direction| {
                let (dx, dy) = direction.delta();
                healer_position.grid.offset(dx, dy, 0)
            })
            .find(|position| {
                !occupied.contains(position)
                    && self
                        .map(healer_position.map)
                        .and_then(|map| map.cell(*position))
                        .is_some_and(|cell| !cell.movement_blocked)
            })
            .map_or(healer_position, |grid| WorldPosition {
                map: healer_position.map,
                grid,
            })
    }

    fn deal_damage(
        &mut self,
        attacker: EntityId,
        target: EntityId,
        raw_damage: i16,
        method: CombatMethod,
        events: &mut Vec<SimulationEvent>,
    ) {
        let target_combatant = self
            .combatants
            .get_mut(&target)
            .expect("combatants validated before damage");
        let damage = (raw_damage - target_combatant.armor).max(1);
        target_combatant.health = (target_combatant.health - damage).max(0);
        events.push(SimulationEvent::Damaged {
            attacker,
            target,
            amount: damage,
            remaining_health: target_combatant.health,
            method,
        });
        if target_combatant.health == 0 {
            events.push(SimulationEvent::Defeated {
                entity: target,
                by: attacker,
            });
            if target == self.player {
                self.paused = true;
            }
        }
    }

    fn clear_line(&self, start: WorldPosition, end: WorldPosition) -> bool {
        if start.map != end.map || start.grid.z != end.grid.z {
            return false;
        }

        let mut x = start.grid.x;
        let mut y = start.grid.y;
        let dx = (end.grid.x - x).abs();
        let sx = if x < end.grid.x { 1 } else { -1 };
        let dy = -(end.grid.y - y).abs();
        let sy = if y < end.grid.y { 1 } else { -1 };
        let mut error = dx + dy;

        loop {
            if x == end.grid.x && y == end.grid.y {
                return true;
            }
            let twice_error = error * 2;
            if twice_error >= dy {
                error += dy;
                x += sx;
            }
            if twice_error <= dx {
                error += dx;
                y += sy;
            }
            if x == end.grid.x && y == end.grid.y {
                return true;
            }
            let current = GridPos::new(x, y, start.grid.z);
            if self
                .map(start.map)
                .and_then(|map| map.cell(current))
                .is_none_or(|cell| cell.sight_blocked)
            {
                return false;
            }
        }
    }
}

fn failed(reason: ActionFailure) -> CommandOutcome {
    CommandOutcome {
        advanced_time: false,
        changed_world: false,
        events: vec![SimulationEvent::ActionFailed(reason)],
    }
}

fn quantity_transfer_item(event: &SimulationEvent) -> ItemId {
    match event {
        SimulationEvent::ItemQuantityTransferred { item, .. }
        | SimulationEvent::ItemTransferred { item, .. } => *item,
        _ => unreachable!("item transfer helper received a non-transfer event"),
    }
}

fn retarget_quantity_event(event: &mut SimulationEvent, final_item: ItemId) {
    if let SimulationEvent::ItemQuantityTransferred { item, .. } = event {
        *item = final_item;
    }
}

fn grid_distance(first: GridPos, second: GridPos) -> i32 {
    (first.x - second.x).abs() + (first.y - second.y).abs()
}

fn stack_weight_for_quantity(item: &Item, quantity: u16) -> u32 {
    if quantity >= item.quantity {
        item.weight_grams
    } else {
        let numerator = u64::from(item.weight_grams) * u64::from(quantity);
        let denominator = u64::from(item.quantity.max(1));
        numerator.div_ceil(denominator).min(u64::from(u32::MAX)) as u32
    }
}

fn consume_one_unit(item: &mut Item) {
    debug_assert!(item.quantity > 0);
    let consumed_weight = stack_weight_for_quantity(item, 1);
    item.quantity -= 1;
    item.weight_grams = item.weight_grams.saturating_sub(consumed_weight);
}

const fn item_kind_is_stackable(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Ammunition { .. }
            | ItemKind::Consumable { .. }
            | ItemKind::Food { .. }
            | ItemKind::Drink { .. }
            | ItemKind::Reagent { .. }
    )
}

fn ranged_distance(first: GridPos, second: GridPos) -> i32 {
    (first.x - second.x).abs().max((first.y - second.y).abs())
}

fn paint_rect(
    map: &mut WorldMap,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    cell: TerrainCell,
) {
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            map.set_cell(GridPos::new(x, y, 0), cell);
        }
    }
}

fn paint_building(
    map: &mut WorldMap,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    entrance: GridPos,
) {
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let boundary = x == min_x || x == max_x || y == min_y || y == max_y;
            let terrain = if GridPos::new(x, y, 0) == entrance {
                TerrainKind::Road
            } else if boundary {
                TerrainKind::Wall
            } else {
                TerrainKind::StoneFloor
            };
            map.set_cell(GridPos::new(x, y, 0), TerrainCell::new(terrain));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combat_simulation() -> Simulation {
        combat_simulation_at(GridPos::new(3, 0, 0))
    }

    fn combat_simulation_at(hostile_position: GridPos) -> Simulation {
        let map_id = MapId(9);
        let mut map = WorldMap::new(map_id);
        for y in -2..=8 {
            for x in -2..=8 {
                map.set_cell(GridPos::new(x, y, 0), TerrainCell::new(TerrainKind::Road));
            }
        }
        let player = EntityId(1);
        let hostile = EntityId(2);
        let mut simulation = Simulation::from_map(
            77,
            map,
            [
                Entity {
                    id: player,
                    kind: EntityKind::Player,
                    position: WorldPosition {
                        map: map_id,
                        grid: GridPos::new(0, 0, 0),
                    },
                    facing: Direction::East,
                },
                Entity {
                    id: hostile,
                    kind: EntityKind::Creature,
                    position: WorldPosition {
                        map: map_id,
                        grid: hostile_position,
                    },
                    facing: Direction::West,
                },
            ],
            Vec::new(),
            player,
        )
        .expect("combat map");
        for combatant in [
            Combatant {
                entity: player,
                health: 12,
                max_health: 12,
                armor: 0,
                hostile_to_player: false,
                experience_reward: 0,
            },
            Combatant {
                entity: hostile,
                health: 8,
                max_health: 8,
                armor: 0,
                hostile_to_player: true,
                experience_reward: 35,
            },
        ] {
            simulation
                .add_combatant(combatant)
                .expect("valid combatant");
        }
        for item in [
            Item {
                id: ItemId(1),
                name: "Test Sword".to_string(),
                kind: ItemKind::MeleeWeapon { damage: 4 },
                quantity: 1,
                weight_grams: 1_000,
                quality: 50,
            },
            Item {
                id: ItemId(2),
                name: "Test Bow".to_string(),
                kind: ItemKind::RangedWeapon {
                    damage: 3,
                    range: 6,
                    ammunition: AmmunitionKind::Arrow,
                },
                quantity: 1,
                weight_grams: 800,
                quality: 50,
            },
            Item {
                id: ItemId(3),
                name: "Test Arrows".to_string(),
                kind: ItemKind::Ammunition {
                    kind: AmmunitionKind::Arrow,
                },
                quantity: 5,
                weight_grams: 200,
                quality: 40,
            },
            Item {
                id: ItemId(4),
                name: "Dressing".to_string(),
                kind: ItemKind::Consumable { healing: 5 },
                quantity: 1,
                weight_grams: 100,
                quality: 40,
            },
        ] {
            simulation.give_item(player, item).expect("valid item");
        }
        simulation.apply_command(GameCommand::Equip(ItemId(1)));
        simulation.apply_command(GameCommand::Equip(ItemId(2)));
        simulation
    }

    #[test]
    fn negative_positions_round_trip_through_chunks() {
        for position in [
            GridPos::new(-1, -1, -1),
            GridPos::new(-32, -32, -8),
            GridPos::new(-33, 65, 9),
            GridPos::new(0, 0, 0),
        ] {
            let (chunk, local) = ChunkCoord::containing(position);
            assert_eq!(chunk.resolve(local), position);
        }
    }

    #[test]
    fn sparse_map_finds_cells_across_chunk_boundaries() {
        let mut map = WorldMap::new(MapId(7));
        let position = GridPos::new(-33, 64, -9);
        map.set_cell(position, TerrainCell::new(TerrainKind::StoneFloor));

        assert_eq!(
            map.cell(position).map(|cell| cell.terrain),
            Some(TerrainKind::StoneFloor)
        );
        assert_eq!(map.chunk_count(), 1);
    }

    #[test]
    fn campaign_seed_and_replay_are_deterministic() {
        let commands = [
            GameCommand::Move(Direction::East),
            GameCommand::Move(Direction::South),
            GameCommand::Wait,
            GameCommand::Move(Direction::North),
        ];
        let mut first = Simulation::demo(42);
        let mut second = Simulation::demo(42);

        for command in commands {
            first.apply_command(command);
            second.apply_command(command);
        }

        assert_eq!(first, second);
    }

    #[test]
    fn pause_stops_time_and_commands() {
        let mut simulation = Simulation::demo(42);
        let initial_position = simulation.player().position;

        simulation.apply_command(GameCommand::Pause);
        simulation.apply_command(GameCommand::Move(Direction::East));
        simulation.apply_command(GameCommand::Wait);

        assert_eq!(simulation.tick, 0);
        assert_eq!(simulation.player().position, initial_position);
    }

    #[test]
    fn demo_town_has_semantic_places_people_and_obstacles() {
        let simulation = Simulation::demo(42);
        let map = simulation.map(MapId(1)).expect("demo map");

        assert!(
            simulation
                .landmarks()
                .any(|landmark| landmark.name == "Trading House")
        );
        assert!(
            simulation
                .entities()
                .any(|entity| entity.kind == EntityKind::Character)
        );
        assert_eq!(
            map.cell(GridPos::new(-16, 3, 0))
                .map(|cell| (cell.terrain, cell.movement_blocked)),
            Some((TerrainKind::Wall, true))
        );
        assert_eq!(
            map.cell(GridPos::new(14, 0, 0))
                .map(|cell| (cell.terrain, cell.movement_blocked)),
            Some((TerrainKind::Bridge, false))
        );
    }

    #[test]
    fn ranged_combat_consumes_ammunition_and_defeats_target_deterministically() {
        let mut first = combat_simulation();
        let mut second = combat_simulation();
        for simulation in [&mut first, &mut second] {
            for _ in 0..3 {
                simulation.apply_command(GameCommand::FireAt(EntityId(2)));
            }
        }

        assert_eq!(first, second);
        assert_eq!(
            first
                .combatant(EntityId(2))
                .map(|combatant| combatant.health),
            Some(0)
        );
        assert_eq!(first.item(ItemId(3)).map(|item| item.quantity), Some(2));
        assert!(!first.entities().any(|entity| entity.id == EntityId(2)));
        assert_eq!(first.progression().ranged_practice, 3);
    }

    #[test]
    fn ranged_preview_and_execution_support_diagonal_targets() {
        let mut simulation = combat_simulation_at(GridPos::new(3, 3, 0));

        assert_eq!(
            simulation.check_ranged_attack(EntityId(1), EntityId(2)),
            Ok(())
        );
        let outcome = simulation.apply_command(GameCommand::FireAt(EntityId(2)));

        assert!(outcome.changed_world);
        assert!(outcome.events.iter().any(|event| matches!(
            event,
            SimulationEvent::Damaged {
                target: EntityId(2),
                method: CombatMethod::Ranged,
                ..
            }
        )));
    }

    #[test]
    fn melee_retaliation_and_field_dressing_use_authoritative_health() {
        let mut simulation = combat_simulation_at(GridPos::new(1, 0, 0));
        let first_attack = simulation.apply_command(GameCommand::Attack(EntityId(2)));

        assert!(first_attack.events.iter().any(|event| matches!(
            event,
            SimulationEvent::Damaged {
                target: EntityId(1),
                method: CombatMethod::Retaliation,
                ..
            }
        )));
        assert_eq!(
            simulation
                .player_combatant()
                .map(|combatant| combatant.health),
            Some(10)
        );

        simulation.apply_command(GameCommand::UseItem(ItemId(4)));
        assert_eq!(
            simulation
                .player_combatant()
                .map(|combatant| combatant.health),
            Some(12)
        );
        assert_eq!(
            simulation.item(ItemId(4)).map(|item| item.quantity),
            Some(0)
        );
        assert_eq!(simulation.progression().martial_practice, 1);
    }

    #[test]
    fn hostile_turns_pursue_and_attack_a_waiting_player() {
        let mut simulation = combat_simulation_at(GridPos::new(4, 0, 0));

        simulation.apply_command(GameCommand::Wait);
        assert_eq!(
            simulation
                .entity(EntityId(2))
                .map(|entity| entity.position.grid),
            Some(GridPos::new(3, 0, 0))
        );
        simulation.apply_command(GameCommand::Wait);
        assert_eq!(
            simulation
                .entity(EntityId(2))
                .map(|entity| entity.position.grid),
            Some(GridPos::new(2, 0, 0))
        );
        simulation.apply_command(GameCommand::Wait);
        assert_eq!(
            simulation
                .entity(EntityId(2))
                .map(|entity| entity.position.grid),
            Some(GridPos::new(1, 0, 0))
        );
        let attack = simulation.apply_command(GameCommand::Wait);

        assert!(attack.events.iter().any(|event| matches!(
            event,
            SimulationEvent::Damaged {
                attacker: EntityId(2),
                target: EntityId(1),
                method: CombatMethod::Retaliation,
                ..
            }
        )));
        assert_eq!(
            simulation
                .player_combatant()
                .map(|combatant| combatant.health),
            Some(10)
        );
    }

    #[test]
    fn defeat_revives_the_player_beside_the_nearest_healer() {
        let mut simulation = combat_simulation_at(GridPos::new(1, 0, 0));
        let healer = EntityId(3);
        let healer_position = WorldPosition {
            map: MapId(9),
            grid: GridPos::new(5, 5, 0),
        };
        simulation
            .add_entity(Entity {
                id: healer,
                kind: EntityKind::Character,
                position: healer_position,
                facing: Direction::South,
            })
            .expect("healer entity");
        simulation
            .register_healer(healer)
            .expect("registered healer");
        simulation
            .combatants
            .get_mut(&simulation.player)
            .expect("player combatant")
            .health = 2;

        let outcome = simulation.apply_command(GameCommand::Wait);

        assert!(outcome.events.iter().any(|event| matches!(
            event,
            SimulationEvent::Defeated {
                entity: EntityId(1),
                by: EntityId(2)
            }
        )));
        assert!(outcome.events.iter().any(|event| matches!(
            event,
            SimulationEvent::RevivedAtHealer {
                entity: EntityId(1),
                healer: EntityId(3),
                health: 12,
                ..
            }
        )));
        assert_eq!(
            grid_distance(simulation.player().position.grid, healer_position.grid),
            1
        );
        assert_eq!(
            simulation
                .player_combatant()
                .map(|combatant| combatant.health),
            Some(12)
        );
        assert!(!simulation.paused);
    }

    #[test]
    fn transitions_change_real_depth_and_combat_stays_on_one_floor() {
        let map_id = MapId(12);
        let mut map = WorldMap::new(map_id);
        for z in [-1, 0] {
            for x in 0..=2 {
                map.set_cell(
                    GridPos::new(x, 0, z),
                    TerrainCell::new(TerrainKind::StoneFloor),
                );
            }
        }
        let player = EntityId(1);
        let hostile = EntityId(2);
        let mut simulation = Simulation::from_map(
            91,
            map,
            [
                Entity {
                    id: player,
                    kind: EntityKind::Player,
                    position: WorldPosition {
                        map: map_id,
                        grid: GridPos::new(0, 0, 0),
                    },
                    facing: Direction::East,
                },
                Entity {
                    id: hostile,
                    kind: EntityKind::Creature,
                    position: WorldPosition {
                        map: map_id,
                        grid: GridPos::new(1, 0, -1),
                    },
                    facing: Direction::West,
                },
            ],
            Vec::new(),
            player,
        )
        .expect("two-level map");
        for combatant in [
            Combatant {
                entity: player,
                health: 12,
                max_health: 12,
                armor: 0,
                hostile_to_player: false,
                experience_reward: 0,
            },
            Combatant {
                entity: hostile,
                health: 3,
                max_health: 3,
                armor: 0,
                hostile_to_player: true,
                experience_reward: 10,
            },
        ] {
            simulation
                .add_combatant(combatant)
                .expect("valid combatant");
        }
        for item in [
            Item {
                id: ItemId(1),
                name: "Test Bow".to_string(),
                kind: ItemKind::RangedWeapon {
                    damage: 3,
                    range: 6,
                    ammunition: AmmunitionKind::Arrow,
                },
                quantity: 1,
                weight_grams: 800,
                quality: 50,
            },
            Item {
                id: ItemId(2),
                name: "Test Arrow".to_string(),
                kind: ItemKind::Ammunition {
                    kind: AmmunitionKind::Arrow,
                },
                quantity: 1,
                weight_grams: 20,
                quality: 40,
            },
        ] {
            simulation.give_item(player, item).expect("valid item");
        }
        simulation.apply_command(GameCommand::Equip(ItemId(1)));
        simulation
            .add_transition(Transition {
                from: WorldPosition {
                    map: map_id,
                    grid: GridPos::new(0, 0, 0),
                },
                to: WorldPosition {
                    map: map_id,
                    grid: GridPos::new(0, 0, -1),
                },
                kind: TransitionKind::Descend,
                name: "Descend".to_string(),
            })
            .expect("valid transition");

        assert_eq!(
            simulation.check_ranged_attack(player, hostile),
            Err(ActionFailure::OutOfRange)
        );
        let traversal = simulation.apply_command(GameCommand::Traverse);
        assert_eq!(simulation.player().position.grid.z, -1);
        assert!(traversal.events.iter().any(|event| matches!(
            event,
            SimulationEvent::Traversed {
                kind: TransitionKind::Descend,
                ..
            }
        )));
        assert_eq!(simulation.check_ranged_attack(player, hostile), Ok(()));
    }

    #[test]
    fn boss_loot_advances_quest_and_turn_in_levels_the_player() {
        let map_id = MapId(13);
        let mut map = WorldMap::new(map_id);
        for x in 0..=3 {
            map.set_cell(
                GridPos::new(x, 0, 0),
                TerrainCell::new(TerrainKind::StoneFloor),
            );
        }
        let player = EntityId(1);
        let boss = EntityId(2);
        let giver = EntityId(3);
        let relic = ItemId(3);
        let quest = QuestId(1);
        let mut simulation = Simulation::from_map(
            92,
            map,
            [
                Entity {
                    id: player,
                    kind: EntityKind::Player,
                    position: WorldPosition {
                        map: map_id,
                        grid: GridPos::new(0, 0, 0),
                    },
                    facing: Direction::East,
                },
                Entity {
                    id: boss,
                    kind: EntityKind::Creature,
                    position: WorldPosition {
                        map: map_id,
                        grid: GridPos::new(3, 0, 0),
                    },
                    facing: Direction::West,
                },
                Entity {
                    id: giver,
                    kind: EntityKind::Character,
                    position: WorldPosition {
                        map: map_id,
                        grid: GridPos::new(1, 0, 0),
                    },
                    facing: Direction::West,
                },
            ],
            Vec::new(),
            player,
        )
        .expect("quest map");
        for combatant in [
            Combatant {
                entity: player,
                health: 12,
                max_health: 12,
                armor: 0,
                hostile_to_player: false,
                experience_reward: 0,
            },
            Combatant {
                entity: boss,
                health: 4,
                max_health: 4,
                armor: 0,
                hostile_to_player: true,
                experience_reward: 35,
            },
        ] {
            simulation
                .add_combatant(combatant)
                .expect("valid combatant");
        }
        for item in [
            Item {
                id: ItemId(1),
                name: "Test Bow".to_string(),
                kind: ItemKind::RangedWeapon {
                    damage: 4,
                    range: 6,
                    ammunition: AmmunitionKind::Arrow,
                },
                quantity: 1,
                weight_grams: 800,
                quality: 50,
            },
            Item {
                id: ItemId(2),
                name: "Test Arrow".to_string(),
                kind: ItemKind::Ammunition {
                    kind: AmmunitionKind::Arrow,
                },
                quantity: 1,
                weight_grams: 20,
                quality: 40,
            },
        ] {
            simulation.give_item(player, item).expect("valid item");
        }
        simulation
            .give_item(
                boss,
                Item {
                    id: relic,
                    name: "Founders' Seal".to_string(),
                    kind: ItemKind::Artifact,
                    quantity: 1,
                    weight_grams: 400,
                    quality: 75,
                },
            )
            .expect("boss relic");
        simulation
            .add_quest(Quest {
                id: quest,
                title: "The Founders' Seal".to_string(),
                description: "Recover the historical seal.".to_string(),
                giver,
                status: QuestStatus::Active,
                objectives: vec![
                    QuestObjective {
                        description: "Defeat the keeper".to_string(),
                        kind: QuestObjectiveKind::Defeat(boss),
                        completed: false,
                    },
                    QuestObjective {
                        description: "Recover the seal".to_string(),
                        kind: QuestObjectiveKind::Recover(relic),
                        completed: false,
                    },
                ],
                reward_experience: 120,
            })
            .expect("valid quest");
        simulation.apply_command(GameCommand::Equip(ItemId(1)));

        let defeat = simulation.apply_command(GameCommand::FireAt(boss));
        assert_eq!(
            simulation.quest(quest).map(|quest| quest.status),
            Some(QuestStatus::ReadyToTurnIn)
        );
        assert!(
            simulation
                .player_inventory()
                .is_some_and(|inventory| inventory.items.contains(&relic))
        );
        assert!(defeat.events.iter().any(|event| matches!(
            event,
            SimulationEvent::QuestReadyToTurnIn { quest: ready } if *ready == quest
        )));

        let completion = simulation.apply_command(GameCommand::TurnInQuest(quest));
        assert_eq!(
            simulation.quest(quest).map(|quest| quest.status),
            Some(QuestStatus::Completed)
        );
        assert_eq!(simulation.progression().experience, 155);
        assert_eq!(simulation.progression().level, 2);
        assert_eq!(simulation.progression().attack_bonus, 1);
        assert!(
            completion
                .events
                .iter()
                .any(|event| matches!(event, SimulationEvent::LevelGained { level: 2, .. }))
        );
    }

    #[test]
    fn simulation_can_travel_between_detailed_and_regional_maps() {
        let local_map = MapId(1);
        let regional_map = MapId(2);
        let position = GridPos::new(0, 0, 0);
        let mut local = WorldMap::new(local_map);
        local.set_cell(position, TerrainCell::new(TerrainKind::StairsDown));
        let player = EntityId(1);
        let mut simulation = Simulation::from_map(
            7,
            local,
            [Entity {
                id: player,
                kind: EntityKind::Player,
                position: WorldPosition {
                    map: local_map,
                    grid: position,
                },
                facing: Direction::South,
            }],
            Vec::new(),
            player,
        )
        .expect("local simulation");
        let mut region = WorldMap::new(regional_map);
        region.set_cell(position, TerrainCell::new(TerrainKind::StairsUp));
        simulation
            .add_map(region, Vec::new())
            .expect("regional map should attach");
        simulation
            .add_transition(Transition {
                from: WorldPosition {
                    map: local_map,
                    grid: position,
                },
                to: WorldPosition {
                    map: regional_map,
                    grid: position,
                },
                kind: TransitionKind::Descend,
                name: "Regional road".to_string(),
            })
            .expect("cross-map transition");

        let outcome = simulation.apply_command(GameCommand::Traverse);
        assert!(outcome.changed_world);
        assert_eq!(simulation.player().position.map, regional_map);
        assert_eq!(simulation.progression().exploration, 1);
        assert_eq!(simulation.progression().experience, 10);
    }

    #[test]
    fn inscribed_history_object_teaches_and_performs_seeded_formula() {
        let mut simulation = Simulation::demo(2);
        let player = simulation.player_id();
        simulation
            .add_combatant(Combatant {
                entity: player,
                health: 5,
                max_health: 12,
                armor: 0,
                hostile_to_player: false,
                experience_reward: 0,
            })
            .expect("player combatant");
        let formula = simulation.rules().formula(FormulaId(1)).unwrap().clone();
        let artifact = ItemId(0x9000);
        simulation
            .give_item(
                player,
                Item {
                    id: artifact,
                    name: "Recovered Hearth Ledger".to_string(),
                    kind: ItemKind::InscribedArtifact {
                        object: ObjectId(77),
                        formula: formula.id,
                    },
                    quantity: 1,
                    weight_grams: 400,
                    quality: 60,
                },
            )
            .expect("artifact");
        for (index, material) in formula.reagents.iter().copied().enumerate() {
            simulation
                .give_item(
                    player,
                    Item {
                        id: ItemId(0x9010 + index as u64),
                        name: material.name().to_string(),
                        kind: ItemKind::Reagent { material },
                        quantity: 2,
                        weight_grams: 50,
                        quality: 50,
                    },
                )
                .expect("reagent");
        }

        let study = simulation.apply_command(GameCommand::Study(artifact));
        assert!(study.events.iter().any(|event| matches!(
            event,
            SimulationEvent::FormulaLearned {
                formula: FormulaId(1),
                ..
            }
        )));
        assert_eq!(simulation.progression().discoveries, 1);
        assert_eq!(simulation.progression().arcane_lore, 1);
        assert_eq!(simulation.progression().experience, 30);

        let casting = simulation.apply_command(GameCommand::Cast {
            formula: FormulaId(1),
            target: None,
        });
        assert!(casting.events.iter().any(|event| matches!(
            event,
            SimulationEvent::SpellCast {
                effect: MagicEffect::Heal,
                ..
            }
        )));
        assert_eq!(simulation.progression().magical_practice, 1);
        assert_eq!(simulation.player_combatant().unwrap().health, 10);
        for item in &simulation.player_inventory().unwrap().items {
            if matches!(
                simulation.item(*item).unwrap().kind,
                ItemKind::Reagent { .. }
            ) {
                assert_eq!(simulation.item(*item).unwrap().quantity, 1);
            }
        }
    }

    #[test]
    fn per_world_formula_can_be_discovered_by_reagent_experiment() {
        let mut simulation = Simulation::demo(2);
        let player = simulation.player_id();
        let formula = simulation.rules().formula(FormulaId(1)).unwrap().clone();
        match formula.condition {
            FormulaCondition::CleanWater => {
                let position = simulation.player().position;
                assert!(simulation.move_entity(
                    player,
                    WorldPosition {
                        map: position.map,
                        grid: GridPos::new(13, 2, 0),
                    },
                ));
            }
            FormulaCondition::NightSky => {
                for _ in 0..12 {
                    simulation.apply_command(GameCommand::Wait);
                }
            }
            FormulaCondition::ExistingFlame
                if !formula.reagents.contains(&MaterialKind::Emberseed) =>
            {
                simulation
                    .give_item(
                        player,
                        Item {
                            id: ItemId(0xa100),
                            name: "emberseed".to_string(),
                            kind: ItemKind::Reagent {
                                material: MaterialKind::Emberseed,
                            },
                            quantity: 1,
                            weight_grams: 20,
                            quality: 40,
                        },
                    )
                    .expect("flame source");
            }
            FormulaCondition::DirectSunlight | FormulaCondition::ExistingFlame => {}
        }
        let first = ItemId(0xa101);
        let second = ItemId(0xa102);
        for (item, material) in [first, second]
            .into_iter()
            .zip(formula.reagents.iter().copied())
        {
            simulation
                .give_item(
                    player,
                    Item {
                        id: item,
                        name: material.name().to_string(),
                        kind: ItemKind::Reagent { material },
                        quantity: 1,
                        weight_grams: 20,
                        quality: 40,
                    },
                )
                .expect("reagent");
        }

        let outcome = simulation.apply_command(GameCommand::Experiment { first, second });
        assert!(outcome.events.iter().any(|event| matches!(
            event,
            SimulationEvent::FormulaLearned { formula: learned, .. } if *learned == formula.id
        )));
        assert!(simulation.known_formulas().contains(&formula.id));
        assert_eq!(simulation.progression().arcane_lore, 1);
        assert_eq!(simulation.progression().magical_practice, 1);
    }

    #[test]
    fn walking_onto_a_cross_map_gate_travels_without_a_second_button() {
        let local_map = MapId(1);
        let regional_map = MapId(2);
        let approach = GridPos::new(-1, 0, 0);
        let gate = GridPos::new(0, 0, 0);
        let mut local = WorldMap::new(local_map);
        local.set_cell(approach, TerrainCell::new(TerrainKind::StoneFloor));
        local.set_cell(gate, TerrainCell::new(TerrainKind::StairsDown));
        let player = EntityId(1);
        let mut simulation = Simulation::from_map(
            7,
            local,
            [Entity {
                id: player,
                kind: EntityKind::Player,
                position: WorldPosition {
                    map: local_map,
                    grid: approach,
                },
                facing: Direction::East,
            }],
            Vec::new(),
            player,
        )
        .expect("local simulation");
        let mut region = WorldMap::new(regional_map);
        region.set_cell(gate, TerrainCell::new(TerrainKind::StairsUp));
        simulation
            .add_map(region, Vec::new())
            .expect("regional map should attach");
        simulation
            .add_transition(Transition {
                from: WorldPosition {
                    map: local_map,
                    grid: gate,
                },
                to: WorldPosition {
                    map: regional_map,
                    grid: gate,
                },
                kind: TransitionKind::Descend,
                name: "Regional road".to_string(),
            })
            .expect("cross-map transition");

        let outcome = simulation.apply_command(GameCommand::Move(Direction::East));

        assert_eq!(simulation.tick, 1);
        assert_eq!(simulation.player().position.map, regional_map);
        assert!(outcome.events.iter().any(|event| matches!(
            event,
            SimulationEvent::Traversed {
                destination,
                ..
            } if destination.map == regional_map
        )));
    }

    #[test]
    fn containers_enforce_locks_capacity_custody_and_ordinary_object_actions() {
        let mut simulation = Simulation::demo(0xabc);
        let player = simulation.player_id();
        let owner = EntityId(2);
        let container = EntityId(0xcafe);
        let position = WorldPosition {
            map: simulation.player().position.map,
            grid: simulation.player().position.grid.offset(1, 0, 0),
        };
        simulation
            .add_entity(Entity {
                id: container,
                kind: EntityKind::Item,
                position,
                facing: Direction::South,
            })
            .expect("container entity");
        simulation
            .add_container(Container {
                entity: container,
                name: "Locked Pantry".to_string(),
                owner,
                capacity_grams: 1_000,
                lock_code: Some(77),
                locked: true,
            })
            .expect("container");
        let food = ItemId(0x501);
        simulation
            .give_item_with_owner(
                container,
                owner,
                Item {
                    id: food,
                    name: "Brown Bread".to_string(),
                    kind: ItemKind::Food { nourishment: 12 },
                    quantity: 2,
                    weight_grams: 600,
                    quality: 40,
                },
            )
            .expect("food");
        let key = ItemId(0x502);
        simulation
            .give_item(
                player,
                Item {
                    id: key,
                    name: "Pantry Key".to_string(),
                    kind: ItemKind::Key { lock_code: 77 },
                    quantity: 1,
                    weight_grams: 40,
                    quality: 50,
                },
            )
            .expect("key");

        assert_eq!(
            simulation
                .apply_command(GameCommand::OpenContainer(container))
                .events,
            vec![SimulationEvent::ActionFailed(
                ActionFailure::ContainerLocked
            )]
        );
        simulation.apply_command(GameCommand::UnlockContainer { container, key });
        assert!(!simulation.container(container).expect("container").locked);
        simulation.apply_command(GameCommand::Take {
            item: food,
            from: container,
        });
        assert_eq!(simulation.legal_owner(food), Some(owner));
        assert!(simulation.is_stolen(food));
        let hunger = simulation.player_needs().hunger;
        simulation.apply_command(GameCommand::Eat(food));
        assert!(simulation.player_needs().hunger < hunger);

        let book = ItemId(0x503);
        simulation
            .give_item(
                player,
                Item {
                    id: book,
                    name: "Town Record".to_string(),
                    kind: ItemKind::Book {
                        subject: BookSubject::LocalHistory,
                    },
                    quantity: 1,
                    weight_grams: 1_100,
                    quality: 60,
                },
            )
            .expect("book");
        let reading = simulation.apply_command(GameCommand::Read(book));
        assert!(reading.events.iter().any(|event| matches!(
            event,
            SimulationEvent::ItemRead {
                item,
                newly_learned: true,
                ..
            } if *item == book
        )));
        let full = simulation.apply_command(GameCommand::Place {
            item: book,
            container,
        });
        assert_eq!(
            full.events,
            vec![SimulationEvent::ActionFailed(ActionFailure::ContainerFull)]
        );

        let drop = simulation.apply_command(GameCommand::Drop(book));
        let holder = drop.events.iter().find_map(|event| match event {
            SimulationEvent::ItemDropped { holder, .. } => Some(*holder),
            _ => None,
        });
        assert!(holder.is_some());
        assert_eq!(simulation.legal_owner(book), Some(player));
    }

    #[test]
    fn custody_and_legal_ownership_distinguish_gifts_from_theft() {
        let mut simulation = Simulation::demo(44);
        let player = simulation.player_id();
        let neighbor = EntityId(2);
        let player_position = simulation.player().position;
        assert!(simulation.move_entity(
            neighbor,
            WorldPosition {
                map: player_position.map,
                grid: player_position.grid.offset(1, 0, 0),
            },
        ));
        let item = ItemId(0xfeed);
        simulation
            .give_item(
                player,
                Item {
                    id: item,
                    name: "Plain wool cloak".to_string(),
                    kind: ItemKind::Artifact,
                    quantity: 1,
                    weight_grams: 900,
                    quality: 30,
                },
            )
            .expect("item");

        simulation.apply_command(GameCommand::Give { item, to: neighbor });
        assert_eq!(simulation.legal_owner(item), Some(neighbor));
        assert!(!simulation.is_stolen(item));
        assert!(
            simulation
                .inventory(neighbor)
                .unwrap()
                .items
                .contains(&item)
        );

        simulation.apply_command(GameCommand::Take {
            item,
            from: neighbor,
        });
        assert_eq!(simulation.legal_owner(item), Some(neighbor));
        assert!(simulation.is_stolen(item));
        assert!(simulation.player_inventory().unwrap().items.contains(&item));

        simulation.apply_command(GameCommand::Give { item, to: neighbor });
        assert_eq!(simulation.legal_owner(item), Some(neighbor));
        assert!(!simulation.is_stolen(item));
    }

    #[test]
    fn stolen_property_cannot_be_laundered_by_gifting_it_to_a_third_party() {
        let mut simulation = Simulation::demo(45);
        let player = simulation.player_id();
        let owner = EntityId(2);
        let recipient = EntityId(3);
        let player_position = simulation.player().position;
        assert!(simulation.move_entity(
            owner,
            WorldPosition {
                map: player_position.map,
                grid: player_position.grid.offset(1, 0, 0),
            },
        ));
        let item = ItemId(0xbeef);
        simulation
            .give_item(
                owner,
                Item {
                    id: item,
                    name: "Household medicine".to_string(),
                    kind: ItemKind::Consumable { healing: 5 },
                    quantity: 1,
                    weight_grams: 100,
                    quality: 40,
                },
            )
            .expect("item");

        simulation.apply_command(GameCommand::Take { item, from: owner });
        assert!(simulation.move_entity(
            recipient,
            WorldPosition {
                map: player_position.map,
                grid: player_position.grid.offset(0, 1, 0),
            },
        ));
        simulation.apply_command(GameCommand::Give {
            item,
            to: recipient,
        });

        assert_eq!(simulation.legal_owner(item), Some(owner));
        assert!(simulation.is_stolen(item));
        assert!(
            simulation
                .inventory(recipient)
                .is_some_and(|inventory| inventory.items.contains(&item))
        );
        assert!(!simulation.player_inventory().unwrap().items.contains(&item));
        assert_eq!(simulation.player_id(), player);
    }

    #[test]
    fn quantity_transfers_preserve_mass_title_and_merge_compatible_stacks() {
        let mut simulation = Simulation::demo(46);
        let player = simulation.player_id();
        let neighbor = EntityId(2);
        let player_position = simulation.player().position;
        assert!(simulation.move_entity(
            neighbor,
            WorldPosition {
                map: player_position.map,
                grid: player_position.grid.offset(1, 0, 0),
            },
        ));
        let provisions = ItemId(0xcafe);
        simulation
            .give_item(
                player,
                Item {
                    id: provisions,
                    name: "Travel bread".to_string(),
                    kind: ItemKind::Food { nourishment: 12 },
                    quantity: 5,
                    weight_grams: 1_750,
                    quality: 40,
                },
            )
            .expect("stack");

        let first = simulation.apply_command(GameCommand::GiveQuantity {
            item: provisions,
            to: neighbor,
            quantity: 2,
        });
        let first_moved = first
            .events
            .iter()
            .find_map(|event| match event {
                SimulationEvent::ItemQuantityTransferred { item, quantity, .. }
                    if *quantity == 2 =>
                {
                    Some(*item)
                }
                _ => None,
            })
            .expect("partial transfer");
        assert_eq!(simulation.item(provisions).unwrap().quantity, 3);
        assert_eq!(simulation.item(provisions).unwrap().weight_grams, 1_050);
        assert_eq!(simulation.item(first_moved).unwrap().quantity, 2);
        assert_eq!(simulation.item(first_moved).unwrap().weight_grams, 700);
        assert_eq!(simulation.legal_owner(first_moved), Some(neighbor));

        simulation.apply_command(GameCommand::GiveQuantity {
            item: provisions,
            to: neighbor,
            quantity: 1,
        });
        assert_eq!(simulation.item(provisions).unwrap().quantity, 2);
        assert_eq!(simulation.item(first_moved).unwrap().quantity, 3);
        assert_eq!(
            simulation
                .items()
                .map(|item| item.weight_grams)
                .sum::<u32>(),
            1_750
        );

        let theft = simulation.apply_command(GameCommand::TakeQuantity {
            item: first_moved,
            from: neighbor,
            quantity: 2,
        });
        let stolen = theft
            .events
            .iter()
            .find_map(|event| match event {
                SimulationEvent::ItemQuantityTransferred { item, .. } => Some(*item),
                _ => None,
            })
            .expect("theft transfer");
        assert_eq!(simulation.item(first_moved).unwrap().quantity, 1);
        assert_eq!(simulation.item(stolen).unwrap().quantity, 2);
        assert_eq!(simulation.legal_owner(stolen), Some(neighbor));
        assert!(simulation.is_stolen(stolen));

        let before = simulation.clone();
        let invalid = simulation.apply_command(GameCommand::DropQuantity {
            item: stolen,
            quantity: 0,
        });
        assert!(invalid.events.contains(&SimulationEvent::ActionFailed(
            ActionFailure::InvalidQuantity
        )));
        assert_eq!(simulation, before);
    }
}
