//! Deterministic physical geography shared by history, simulation, and rendering.
//!
//! The atlas stores semantic facts rather than pixels. History decides where
//! settlements and routes can exist from these facts; clients may project the
//! same cells as overhead, text, or isometric maps.

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
};

pub const ATLAS_WIDTH: i16 = 256;
pub const ATLAS_HEIGHT: i16 = 256;
pub const ATLAS_MIN_X: i16 = -(ATLAS_WIDTH / 2);
pub const ATLAS_MIN_Y: i16 = -(ATLAS_HEIGHT / 2);
pub const ATLAS_MAX_X: i16 = ATLAS_MIN_X + ATLAS_WIDTH - 1;
pub const ATLAS_MAX_Y: i16 = ATLAS_MIN_Y + ATLAS_HEIGHT - 1;

const CELL_COUNT: usize = ATLAS_WIDTH as usize * ATLAS_HEIGHT as usize;
const SEA_LEVEL: i16 = -70;
const GEOGRAPHY_STREAM: u64 = 0x574f_524c_445f_4745;
const MOISTURE_STREAM: u64 = 0x4d4f_4953_5455_5245;
const TEMPERATURE_STREAM: u64 = 0x5445_4d50_4552_4154;
const RIVER_STREAM: u64 = 0x5249_5645_5253_2020;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AtlasPosition {
    pub x: i16,
    pub y: i16,
}

impl AtlasPosition {
    pub const fn new(x: i16, y: i16) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Self) -> u16 {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaterBody {
    None,
    Ocean,
    River,
    Lake,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Biome {
    Ocean,
    Coast,
    Grassland,
    Forest,
    Desert,
    Swamp,
    Tundra,
    Hills,
    Mountains,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtlasCell {
    /// Height relative to sea level in abstract metres.
    pub elevation: i16,
    /// 0 is arid and 1,000 is saturated.
    pub moisture: u16,
    /// 0 is polar and 1,000 is tropical.
    pub temperature: u16,
    pub water: WaterBody,
    pub biome: Biome,
    /// Connected passable landmass. Zero is water or impassable terrain.
    pub landmass: u16,
}

impl AtlasCell {
    pub fn is_water(self) -> bool {
        self.water != WaterBody::None
    }

    pub fn is_passable_land(self) -> bool {
        self.water == WaterBody::None && self.biome != Biome::Mountains
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SitePreference {
    Capital,
    Agrarian,
    Forest,
    Mining,
    Crossroads,
    River,
    Monastic,
    Fortress,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldAtlas {
    campaign_seed: u64,
    cells: Vec<AtlasCell>,
}

impl WorldAtlas {
    pub fn generate(campaign_seed: u64) -> Self {
        let mut cells = Vec::with_capacity(CELL_COUNT);
        for y in ATLAS_MIN_Y..=ATLAS_MAX_Y {
            for x in ATLAS_MIN_X..=ATLAS_MAX_X {
                let continental = fractal_noise(campaign_seed ^ GEOGRAPHY_STREAM, x, y);
                let edge = edge_pressure(x, y);
                let elevation = (i32::from(continental) - edge - 40).clamp(-1_200, 1_200) as i16;
                let moisture_noise =
                    i32::from(fractal_noise(campaign_seed ^ MOISTURE_STREAM, x, y));
                let temperature_noise =
                    i32::from(fractal_noise(campaign_seed ^ TEMPERATURE_STREAM, x, y));
                let latitude = i32::from(y.unsigned_abs()) * 560 / i32::from(ATLAS_HEIGHT / 2);
                let altitude_cooling = i32::from(elevation.max(0)) / 3;
                cells.push(AtlasCell {
                    elevation,
                    moisture: (520 + moisture_noise / 3).clamp(0, 1_000) as u16,
                    temperature: (820 - latitude - altitude_cooling + temperature_noise / 8)
                        .clamp(0, 1_000) as u16,
                    water: WaterBody::None,
                    biome: Biome::Grassland,
                    landmass: 0,
                });
            }
        }

        let mut atlas = Self {
            campaign_seed,
            cells,
        };
        atlas.mark_oceans_and_lakes();
        atlas.carve_rivers();
        atlas.reclassify_biomes();
        atlas.assign_landmasses();
        atlas
    }

    pub const fn width(&self) -> i16 {
        ATLAS_WIDTH
    }

    pub const fn height(&self) -> i16 {
        ATLAS_HEIGHT
    }

    pub fn cell(&self, position: AtlasPosition) -> Option<&AtlasCell> {
        index(position).map(|index| &self.cells[index])
    }

    pub fn cells(&self) -> impl Iterator<Item = (AtlasPosition, &AtlasCell)> {
        self.cells.iter().enumerate().map(|(index, cell)| {
            let x = index % ATLAS_WIDTH as usize;
            let y = index / ATLAS_WIDTH as usize;
            (
                AtlasPosition::new(ATLAS_MIN_X + x as i16, ATLAS_MIN_Y + y as i16),
                cell,
            )
        })
    }

    pub fn largest_landmass(&self) -> u16 {
        let mut sizes = vec![0_u32; usize::from(u16::MAX) + 1];
        for cell in &self.cells {
            sizes[usize::from(cell.landmass)] += 1;
        }
        sizes
            .iter()
            .enumerate()
            .skip(1)
            .max_by_key(|(_, size)| **size)
            .map_or(0, |(landmass, _)| landmass as u16)
    }

    pub fn choose_site(
        &self,
        preference: SitePreference,
        occupied: &[AtlasPosition],
        required_landmass: Option<u16>,
    ) -> Option<AtlasPosition> {
        for minimum_distance in [20, 14, 8, 1] {
            let candidate = self
                .cells()
                .filter(|(_, cell)| {
                    cell.is_passable_land()
                        && required_landmass.is_none_or(|landmass| cell.landmass == landmass)
                })
                .filter(|(position, _)| {
                    occupied
                        .iter()
                        .all(|other| position.distance(*other) >= minimum_distance)
                })
                .max_by_key(|(position, cell)| {
                    settlement_score(
                        self,
                        preference,
                        *position,
                        **cell,
                        occupied,
                        self.campaign_seed,
                    )
                })
                .map(|(position, _)| position);
            if candidate.is_some() {
                return candidate;
            }
        }
        None
    }

    pub fn route(&self, start: AtlasPosition, goal: AtlasPosition) -> Option<Vec<AtlasPosition>> {
        let start_index = index(start)?;
        let goal_index = index(goal)?;
        let mut costs = vec![u32::MAX; CELL_COUNT];
        let mut previous = vec![None; CELL_COUNT];
        let mut frontier = BinaryHeap::new();
        costs[start_index] = 0;
        frontier.push(Reverse((start.distance(goal) as u32, 0_u32, start_index)));

        while let Some(Reverse((_, cost, current_index))) = frontier.pop() {
            if current_index == goal_index {
                break;
            }
            if cost != costs[current_index] {
                continue;
            }
            let current = position(current_index);
            for neighbor in neighbors(current) {
                let Some(neighbor_index) = index(neighbor) else {
                    continue;
                };
                let cell = self.cells[neighbor_index];
                let Some(step_cost) = travel_cost(cell) else {
                    continue;
                };
                let next_cost = cost.saturating_add(step_cost);
                if next_cost >= costs[neighbor_index] {
                    continue;
                }
                costs[neighbor_index] = next_cost;
                previous[neighbor_index] = Some(current_index);
                let estimate = next_cost + u32::from(neighbor.distance(goal)) * 2;
                frontier.push(Reverse((estimate, next_cost, neighbor_index)));
            }
        }

        if costs[goal_index] == u32::MAX {
            return None;
        }
        let mut path = vec![goal];
        let mut current = goal_index;
        while current != start_index {
            current = previous[current]?;
            path.push(position(current));
        }
        path.reverse();
        Some(path)
    }

    fn mark_oceans_and_lakes(&mut self) {
        let mut ocean = vec![false; CELL_COUNT];
        let mut queue = VecDeque::new();
        for x in ATLAS_MIN_X..=ATLAS_MAX_X {
            for y in [ATLAS_MIN_Y, ATLAS_MAX_Y] {
                let position = AtlasPosition::new(x, y);
                let cell_index = index(position).expect("atlas boundary");
                if self.cells[cell_index].elevation <= SEA_LEVEL && !ocean[cell_index] {
                    ocean[cell_index] = true;
                    queue.push_back(position);
                }
            }
        }
        for y in ATLAS_MIN_Y..=ATLAS_MAX_Y {
            for x in [ATLAS_MIN_X, ATLAS_MAX_X] {
                let position = AtlasPosition::new(x, y);
                let cell_index = index(position).expect("atlas boundary");
                if self.cells[cell_index].elevation <= SEA_LEVEL && !ocean[cell_index] {
                    ocean[cell_index] = true;
                    queue.push_back(position);
                }
            }
        }
        while let Some(current) = queue.pop_front() {
            for neighbor in neighbors(current) {
                let Some(neighbor_index) = index(neighbor) else {
                    continue;
                };
                if !ocean[neighbor_index] && self.cells[neighbor_index].elevation <= SEA_LEVEL {
                    ocean[neighbor_index] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        for (cell_index, cell) in self.cells.iter_mut().enumerate() {
            if ocean[cell_index] {
                cell.water = WaterBody::Ocean;
            } else if cell.elevation <= SEA_LEVEL {
                cell.water = WaterBody::Lake;
            }
        }
    }

    fn carve_rivers(&mut self) {
        let distance_to_ocean = self.distance_to_ocean();
        let mut candidates = self
            .cells()
            .filter(|(_, cell)| {
                cell.water == WaterBody::None && cell.elevation > 280 && cell.moisture > 480
            })
            .map(|(position, cell)| {
                let variation =
                    (coordinate_hash(self.campaign_seed ^ RIVER_STREAM, position.x, position.y)
                        % 101) as i32;
                (
                    i32::from(cell.elevation) + i32::from(cell.moisture) / 2 + variation,
                    position,
                )
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| right.cmp(left));

        let mut sources = Vec::new();
        for (_, candidate) in candidates {
            if sources
                .iter()
                .all(|source: &AtlasPosition| source.distance(candidate) >= 24)
            {
                sources.push(candidate);
                if sources.len() == 14 {
                    break;
                }
            }
        }

        for source in sources {
            let mut current = source;
            let mut visited = vec![false; CELL_COUNT];
            for _ in 0..512 {
                let current_index = index(current).expect("river source inside atlas");
                if visited[current_index] {
                    break;
                }
                visited[current_index] = true;
                if self.cells[current_index].water == WaterBody::Ocean {
                    break;
                }
                if self.cells[current_index].water == WaterBody::Lake {
                    break;
                }
                self.cells[current_index].water = WaterBody::River;
                self.cells[current_index].moisture = self.cells[current_index]
                    .moisture
                    .saturating_add(160)
                    .min(1_000);
                let current_distance = distance_to_ocean[current_index];
                let current_elevation = self.cells[current_index].elevation;
                let next = neighbors(current)
                    .into_iter()
                    .filter_map(|neighbor| {
                        let neighbor_index = index(neighbor)?;
                        let distance = distance_to_ocean[neighbor_index];
                        (distance < current_distance).then(|| {
                            let uphill = i32::from(
                                (self.cells[neighbor_index].elevation - current_elevation).max(0),
                            );
                            let meander = (coordinate_hash(
                                self.campaign_seed ^ RIVER_STREAM.rotate_left(7),
                                neighbor.x,
                                neighbor.y,
                            ) % 17) as i32;
                            (i32::from(distance) * 24 + uphill * 3 + meander, neighbor)
                        })
                    })
                    .min_by_key(|candidate| *candidate);
                let Some((_, next)) = next else {
                    break;
                };
                current = next;
            }
        }

        let river_positions = self
            .cells()
            .filter(|(_, cell)| cell.water == WaterBody::River)
            .map(|(position, _)| position)
            .collect::<Vec<_>>();
        for river in river_positions {
            for dy in -3..=3_i16 {
                for dx in -3..=3_i16 {
                    if dx.abs() + dy.abs() > 3 {
                        continue;
                    }
                    let nearby = AtlasPosition::new(river.x + dx, river.y + dy);
                    if let Some(nearby_index) = index(nearby) {
                        let bonus = u16::try_from(4 - (dx.abs() + dy.abs())).unwrap_or(0) * 35;
                        self.cells[nearby_index].moisture = self.cells[nearby_index]
                            .moisture
                            .saturating_add(bonus)
                            .min(1_000);
                    }
                }
            }
        }
    }

    fn distance_to_ocean(&self) -> Vec<u16> {
        let mut distance = vec![u16::MAX; CELL_COUNT];
        let mut queue = VecDeque::new();
        for (cell_index, cell) in self.cells.iter().enumerate() {
            if cell.water == WaterBody::Ocean {
                distance[cell_index] = 0;
                queue.push_back(position(cell_index));
            }
        }
        while let Some(current) = queue.pop_front() {
            let current_index = index(current).expect("queued atlas position");
            let next_distance = distance[current_index].saturating_add(1);
            for neighbor in neighbors(current) {
                let Some(neighbor_index) = index(neighbor) else {
                    continue;
                };
                if next_distance < distance[neighbor_index] {
                    distance[neighbor_index] = next_distance;
                    queue.push_back(neighbor);
                }
            }
        }
        distance
    }

    fn reclassify_biomes(&mut self) {
        let ocean_adjacency = self
            .cells()
            .map(|(position, _)| {
                neighbors(position).into_iter().any(|neighbor| {
                    self.cell(neighbor)
                        .is_some_and(|cell| cell.water == WaterBody::Ocean)
                })
            })
            .collect::<Vec<_>>();
        for (cell_index, cell) in self.cells.iter_mut().enumerate() {
            cell.biome = if cell.water == WaterBody::Ocean {
                Biome::Ocean
            } else if cell.elevation > 400 {
                Biome::Mountains
            } else if cell.elevation > 220 {
                Biome::Hills
            } else if cell.temperature < 190 {
                Biome::Tundra
            } else if ocean_adjacency[cell_index] {
                Biome::Coast
            } else if cell.moisture < 250 {
                Biome::Desert
            } else if cell.moisture > 790 && cell.elevation < 100 {
                Biome::Swamp
            } else if cell.moisture > 570 {
                Biome::Forest
            } else {
                Biome::Grassland
            };
        }
    }

    fn assign_landmasses(&mut self) {
        let mut next_landmass = 1_u16;
        for start_index in 0..CELL_COUNT {
            if self.cells[start_index].landmass != 0 || !self.cells[start_index].is_passable_land()
            {
                continue;
            }
            let mut queue = VecDeque::from([position(start_index)]);
            self.cells[start_index].landmass = next_landmass;
            while let Some(current) = queue.pop_front() {
                for neighbor in neighbors(current) {
                    let Some(neighbor_index) = index(neighbor) else {
                        continue;
                    };
                    if self.cells[neighbor_index].landmass == 0
                        && self.cells[neighbor_index].is_passable_land()
                    {
                        self.cells[neighbor_index].landmass = next_landmass;
                        queue.push_back(neighbor);
                    }
                }
            }
            next_landmass = next_landmass.saturating_add(1);
        }
    }
}

fn settlement_score(
    atlas: &WorldAtlas,
    preference: SitePreference,
    position: AtlasPosition,
    cell: AtlasCell,
    occupied: &[AtlasPosition],
    seed: u64,
) -> i64 {
    let adjacent = neighbors(position)
        .into_iter()
        .filter_map(|neighbor| atlas.cell(neighbor).copied())
        .collect::<Vec<_>>();
    let river = adjacent
        .iter()
        .any(|neighbor| neighbor.water == WaterBody::River);
    let coast = adjacent
        .iter()
        .any(|neighbor| neighbor.water == WaterBody::Ocean);
    let mountain = adjacent
        .iter()
        .any(|neighbor| neighbor.biome == Biome::Mountains);
    let center_penalty = i64::from(position.x.abs()) + i64::from(position.y.abs());
    let remoteness = occupied
        .iter()
        .map(|other| position.distance(*other))
        .min()
        .unwrap_or(0);
    let biome_score = |wanted| i64::from(cell.biome == wanted) * 1_000;
    let role_score = match preference {
        SitePreference::Capital => {
            biome_score(Biome::Grassland) + i64::from(river || coast) * 850 - center_penalty * 3
        }
        SitePreference::Agrarian => {
            biome_score(Biome::Grassland) + i64::from(cell.moisture.clamp(350, 700))
                - i64::from(cell.elevation.abs())
        }
        SitePreference::Forest => biome_score(Biome::Forest) + i64::from(cell.moisture),
        SitePreference::Mining => {
            biome_score(Biome::Hills) + i64::from(mountain) * 900 + i64::from(cell.elevation.max(0))
        }
        SitePreference::Crossroads => {
            biome_score(Biome::Grassland) + i64::from(river) * 450
                - center_penalty
                - i64::from(remoteness) * 2
        }
        SitePreference::River => {
            i64::from(river) * 1_500 + i64::from(coast) * 600 + biome_score(Biome::Grassland)
        }
        SitePreference::Monastic => {
            (biome_score(Biome::Hills) + biome_score(Biome::Forest)) + i64::from(remoteness) * 8
        }
        SitePreference::Fortress => {
            biome_score(Biome::Hills) + i64::from(mountain) * 750 + i64::from(remoteness) * 3
        }
    };
    role_score
        + i64::from(cell.temperature) / 4
        + (coordinate_hash(seed, position.x, position.y) % 97) as i64
}

fn travel_cost(cell: AtlasCell) -> Option<u32> {
    if cell.water == WaterBody::Ocean || cell.water == WaterBody::Lake {
        return None;
    }
    if cell.water == WaterBody::River {
        return Some(28);
    }
    match cell.biome {
        Biome::Ocean | Biome::Mountains => None,
        Biome::Grassland | Biome::Coast => Some(2),
        Biome::Desert => Some(5),
        Biome::Forest => Some(6),
        Biome::Hills | Biome::Tundra => Some(9),
        Biome::Swamp => Some(16),
    }
}

fn edge_pressure(x: i16, y: i16) -> i32 {
    let x_pressure = i32::from(x.abs()) * 1_000 / i32::from(ATLAS_WIDTH / 2);
    let y_pressure = i32::from(y.abs()) * 1_000 / i32::from(ATLAS_HEIGHT / 2);
    let edge = x_pressure.max(y_pressure);
    if edge > 650 { (edge - 650) * 3 } else { 0 }
}

fn fractal_noise(seed: u64, x: i16, y: i16) -> i16 {
    let octaves = [(96_i16, 16_i32), (48, 8), (24, 4), (12, 2), (6, 1)];
    let weighted = octaves
        .into_iter()
        .map(|(spacing, weight)| smooth_noise(seed ^ spacing as u64, x, y, spacing) * weight)
        .sum::<i32>();
    (weighted / 31 / 32).clamp(-1_024, 1_024) as i16
}

fn smooth_noise(seed: u64, x: i16, y: i16, spacing: i16) -> i32 {
    let grid_x = x.div_euclid(spacing);
    let grid_y = y.div_euclid(spacing);
    let remainder_x = i32::from(x.rem_euclid(spacing));
    let remainder_y = i32::from(y.rem_euclid(spacing));
    let spacing = i32::from(spacing);
    let smooth_x = smooth_step(remainder_x, spacing);
    let smooth_y = smooth_step(remainder_y, spacing);
    let n00 = lattice(seed, grid_x, grid_y);
    let n10 = lattice(seed, grid_x + 1, grid_y);
    let n01 = lattice(seed, grid_x, grid_y + 1);
    let n11 = lattice(seed, grid_x + 1, grid_y + 1);
    let top = lerp(n00, n10, smooth_x, spacing);
    let bottom = lerp(n01, n11, smooth_x, spacing);
    lerp(top, bottom, smooth_y, spacing)
}

fn smooth_step(value: i32, scale: i32) -> i32 {
    value * value * (3 * scale - 2 * value) / (scale * scale)
}

fn lerp(first: i32, second: i32, amount: i32, scale: i32) -> i32 {
    first + (second - first) * amount / scale
}

fn lattice(seed: u64, x: i16, y: i16) -> i32 {
    let value = coordinate_hash(seed, x, y);
    (value & 0xffff) as i32 - 32_768
}

fn coordinate_hash(seed: u64, x: i16, y: i16) -> u64 {
    mix64(
        seed ^ (x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ (y as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9),
    )
}

const fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn index(position: AtlasPosition) -> Option<usize> {
    if !(ATLAS_MIN_X..=ATLAS_MAX_X).contains(&position.x)
        || !(ATLAS_MIN_Y..=ATLAS_MAX_Y).contains(&position.y)
    {
        return None;
    }
    let x = usize::try_from(position.x - ATLAS_MIN_X).ok()?;
    let y = usize::try_from(position.y - ATLAS_MIN_Y).ok()?;
    Some(y * ATLAS_WIDTH as usize + x)
}

fn position(index: usize) -> AtlasPosition {
    AtlasPosition::new(
        ATLAS_MIN_X + (index % ATLAS_WIDTH as usize) as i16,
        ATLAS_MIN_Y + (index / ATLAS_WIDTH as usize) as i16,
    )
}

fn neighbors(position: AtlasPosition) -> [AtlasPosition; 4] {
    [
        AtlasPosition::new(position.x, position.y - 1),
        AtlasPosition::new(position.x + 1, position.y),
        AtlasPosition::new(position.x, position.y + 1),
        AtlasPosition::new(position.x - 1, position.y),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_is_deterministic_and_contains_large_scale_geography() {
        let first = WorldAtlas::generate(0x55aa_2026);
        let replay = WorldAtlas::generate(0x55aa_2026);
        assert_eq!(first, replay);
        assert_eq!(first.cells.len(), 65_536);

        let count = |predicate: fn(&AtlasCell) -> bool| {
            first.cells.iter().filter(|cell| predicate(cell)).count()
        };
        let oceans = count(|cell| cell.water == WaterBody::Ocean);
        let rivers = count(|cell| cell.water == WaterBody::River);
        let mountains = count(|cell| cell.biome == Biome::Mountains);
        let forests = count(|cell| cell.biome == Biome::Forest);
        assert!(oceans > 8_000, "only {oceans} ocean cells");
        assert!(rivers > 100, "only {rivers} river cells");
        assert!(mountains > 100, "only {mountains} mountain cells");
        assert!(forests > 100, "only {forests} forest cells");
        assert_ne!(first.largest_landmass(), 0);
    }

    #[test]
    fn settlement_preferences_and_routes_use_real_terrain() {
        let atlas = WorldAtlas::generate(77);
        let landmass = atlas.largest_landmass();
        let capital = atlas
            .choose_site(SitePreference::Capital, &[], Some(landmass))
            .expect("capital");
        let mine = atlas
            .choose_site(SitePreference::Mining, &[capital], Some(landmass))
            .expect("mine");
        let route = atlas.route(capital, mine).expect("connected road");

        assert!(capital.distance(mine) >= 20);
        assert!(route.len() > 20);
        assert!(route.iter().all(|position| {
            atlas
                .cell(*position)
                .is_some_and(|cell| cell.water != WaterBody::Ocean)
        }));
    }
}
