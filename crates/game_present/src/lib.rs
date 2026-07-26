//! Semantic, player-visible presentation state.
//!
//! Presentation snapshots contain no atlas coordinates, filenames, GPU handles, or
//! projection-specific coordinates.

use ultimate_fate_core::{
    Direction, EntityId, EntityKind, GridPos, LandmarkKind, MapId, Simulation, TerrainKind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewportRequest {
    pub map: MapId,
    pub center: GridPos,
    pub half_width: i32,
    pub half_height: i32,
    pub z: i16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisibleTerrain {
    pub position: GridPos,
    pub kind: TerrainKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharacterAppearance {
    Player,
    Villager,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Appearance {
    Character(CharacterAppearance),
    Creature,
    Item,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VisualState {
    #[default]
    Idle,
    Moving,
    Acting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisibleEntity {
    pub id: EntityId,
    pub position: GridPos,
    pub facing: Direction,
    pub appearance: Appearance,
    pub state: VisualState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleLandmark {
    pub name: String,
    pub kind: LandmarkKind,
    pub position: GridPos,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationSnapshot {
    pub tick: u64,
    pub paused: bool,
    pub map: MapId,
    pub camera_center: GridPos,
    pub terrain: Vec<VisibleTerrain>,
    pub entities: Vec<VisibleEntity>,
    pub landmarks: Vec<VisibleLandmark>,
}

impl PresentationSnapshot {
    /// Produces the ordinary player-facing snapshot. An omniscient debugger should
    /// use a separate API so hidden facts never leak into gameplay presentation.
    pub fn from_simulation(simulation: &Simulation, request: ViewportRequest) -> Self {
        let min_x = request.center.x - request.half_width;
        let max_x = request.center.x + request.half_width;
        let min_y = request.center.y - request.half_height;
        let max_y = request.center.y + request.half_height;

        let terrain = simulation
            .map(request.map)
            .into_iter()
            .flat_map(|map| map.cells())
            .filter(|(position, _)| {
                position.z == request.z
                    && (min_x..=max_x).contains(&position.x)
                    && (min_y..=max_y).contains(&position.y)
            })
            .map(|(position, cell)| VisibleTerrain {
                position,
                kind: cell.terrain,
            })
            .collect();

        let entities = simulation
            .entities()
            .filter(|entity| {
                let position = entity.position;
                position.map == request.map
                    && position.grid.z == request.z
                    && (min_x..=max_x).contains(&position.grid.x)
                    && (min_y..=max_y).contains(&position.grid.y)
            })
            .map(|entity| VisibleEntity {
                id: entity.id,
                position: entity.position.grid,
                facing: entity.facing,
                appearance: match entity.kind {
                    EntityKind::Player => Appearance::Character(CharacterAppearance::Player),
                    EntityKind::Character => Appearance::Character(CharacterAppearance::Villager),
                    EntityKind::Creature => Appearance::Creature,
                    EntityKind::Item => Appearance::Item,
                },
                state: VisualState::Idle,
            })
            .collect();
        let landmarks = simulation
            .landmarks()
            .filter(|landmark| {
                let position = landmark.position;
                position.map == request.map
                    && position.grid.z == request.z
                    && (min_x..=max_x).contains(&position.grid.x)
                    && (min_y..=max_y).contains(&position.grid.y)
            })
            .map(|landmark| VisibleLandmark {
                name: landmark.name.clone(),
                kind: landmark.kind,
                position: landmark.position.grid,
            })
            .collect();

        Self {
            tick: simulation.tick,
            paused: simulation.paused,
            map: request.map,
            camera_center: request.center,
            terrain,
            entities,
            landmarks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_semantic_and_deterministic() {
        let simulation = Simulation::demo(123);
        let request = ViewportRequest {
            map: simulation.player().position.map,
            center: simulation.player().position.grid,
            half_width: 8,
            half_height: 6,
            z: 0,
        };

        let first = PresentationSnapshot::from_simulation(&simulation, request);
        let second = PresentationSnapshot::from_simulation(&simulation, request);

        assert_eq!(first, second);
        assert!(!first.terrain.is_empty());
        assert!(first.entities.iter().any(|entity| {
            entity.appearance == Appearance::Character(CharacterAppearance::Player)
        }));
        assert!(first.entities.iter().any(|entity| {
            entity.appearance == Appearance::Character(CharacterAppearance::Villager)
        }));
        assert!(
            first
                .landmarks
                .iter()
                .any(|landmark| landmark.name == "Trading House")
        );
    }
}
