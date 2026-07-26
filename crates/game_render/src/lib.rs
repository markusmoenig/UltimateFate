//! Projection, art resolution, and the surface-agnostic WGPU drawing layer.

use bytemuck::{Pod, Zeroable};
use ultimate_fate_core::{Direction, GridPos, TerrainKind};
use ultimate_fate_present::{Appearance, CharacterAppearance, PresentationSnapshot, VisibleEntity};
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScreenPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewportSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl UiRect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn inset(self, amount: f32) -> Self {
        Self::new(
            self.x + amount,
            self.y + amount,
            (self.width - amount * 2.0).max(0.0),
            (self.height - amount * 2.0).max(0.0),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTextStyle {
    pub color: [f32; 4],
    pub pixel_scale: f32,
    pub line_spacing: f32,
}

impl Default for UiTextStyle {
    fn default() -> Self {
        Self {
            color: [0.88, 0.90, 0.82, 1.0],
            pixel_scale: 2.0,
            line_spacing: 4.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UiCommand {
    Rect {
        bounds: UiRect,
        color: [f32; 4],
        clip: Option<UiRect>,
    },
    Text {
        bounds: UiRect,
        text: String,
        style: UiTextStyle,
        scroll_y: f32,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiDrawList {
    pub commands: Vec<UiCommand>,
}

impl UiDrawList {
    pub fn rect(&mut self, bounds: UiRect, color: [f32; 4]) {
        self.commands.push(UiCommand::Rect {
            bounds,
            color,
            clip: None,
        });
    }

    pub fn bordered_panel(
        &mut self,
        bounds: UiRect,
        fill: [f32; 4],
        border: [f32; 4],
        border_width: f32,
    ) {
        self.rect(bounds, border);
        self.rect(bounds.inset(border_width), fill);
    }

    pub fn text(&mut self, bounds: UiRect, text: impl Into<String>, style: UiTextStyle) {
        self.commands.push(UiCommand::Text {
            bounds,
            text: text.into(),
            style,
            scroll_y: 0.0,
        });
    }

    pub fn scrolled_text(
        &mut self,
        bounds: UiRect,
        text: impl Into<String>,
        style: UiTextStyle,
        scroll_y: f32,
    ) {
        self.commands.push(UiCommand::Text {
            bounds,
            text: text.into(),
            style,
            scroll_y,
        });
    }

    pub fn tail_text(&mut self, bounds: UiRect, text: impl Into<String>, style: UiTextStyle) {
        let text = text.into();
        let content_height = text_layout_height(bounds.width, &text, style);
        self.scrolled_text(
            bounds,
            text,
            style,
            (content_height - bounds.height).max(0.0),
        );
    }
}

pub trait ArtPack {
    fn terrain_parts(&self, terrain: TerrainKind, position: GridPos) -> Vec<SpritePart>;
    fn entity_parts(&self, entity: &VisibleEntity) -> Vec<SpritePart>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpritePart {
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub layer: i64,
}

impl SpritePart {
    const fn new(offset: [f32; 2], size: [f32; 2], color: [f32; 4], layer: i64) -> Self {
        Self {
            offset,
            size,
            color,
            layer,
        }
    }
}

/// A compact, code-native pixel-art pack inspired by late-1980s and early-1990s
/// overhead role-playing games. It deliberately uses the semantic art boundary:
/// replacing these parts with an atlas later requires no simulation changes.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClassicArtPack;

impl ArtPack for ClassicArtPack {
    fn terrain_parts(&self, terrain: TerrainKind, position: GridPos) -> Vec<SpritePart> {
        let mut parts = terrain_sprite(terrain, position);
        for part in &mut parts {
            part.color[0] *= 0.72;
            part.color[1] *= 0.72;
            part.color[2] *= 0.72;
        }
        parts
    }

    fn entity_parts(&self, entity: &VisibleEntity) -> Vec<SpritePart> {
        match entity.appearance {
            Appearance::Character(CharacterAppearance::Player) => player_sprite(entity),
            Appearance::Character(CharacterAppearance::Villager) => villager_sprite(entity),
            Appearance::Creature => creature_sprite(entity),
            Appearance::Item => item_sprite(),
        }
    }
}

fn terrain_sprite(terrain: TerrainKind, position: GridPos) -> Vec<SpritePart> {
    // A deliberately constrained, earthy palette: strong enough to read at
    // 24 pixels per tile, but closer to DOS-era Western CRPGs than modern
    // candy-colored console RPGs.
    const GRASS: [f32; 4] = [0.19, 0.32, 0.13, 1.0];
    const GRASS_DARK: [f32; 4] = [0.07, 0.20, 0.07, 1.0];
    const GRASS_LIGHT: [f32; 4] = [0.34, 0.43, 0.13, 1.0];
    const DIRT: [f32; 4] = [0.31, 0.20, 0.09, 1.0];
    const DIRT_LIGHT: [f32; 4] = [0.49, 0.34, 0.15, 1.0];
    const STONE: [f32; 4] = [0.36, 0.38, 0.36, 1.0];
    const STONE_DARK: [f32; 4] = [0.17, 0.19, 0.19, 1.0];
    const STONE_LIGHT: [f32; 4] = [0.55, 0.57, 0.52, 1.0];
    const WATER: [f32; 4] = [0.04, 0.22, 0.43, 1.0];
    const WATER_DARK: [f32; 4] = [0.018, 0.10, 0.25, 1.0];
    const WATER_LIGHT: [f32; 4] = [0.19, 0.43, 0.57, 1.0];

    let hash = tile_hash(position, terrain as u64);
    let x = hash_axis(hash, 0);
    let y = hash_axis(hash, 8);
    let mut parts = Vec::with_capacity(10);
    let mut base = |color| {
        // A slight overlap prevents filtering or high-DPI rounding from
        // reintroducing dark seams between cells.
        parts.push(SpritePart::new([0.0, 0.0], [1.02, 1.02], color, 0));
    };
    match terrain {
        TerrainKind::Grass => {
            base(GRASS);
            parts.extend([
                SpritePart::new([x, y], [0.08, 0.18], GRASS_DARK, 1),
                SpritePart::new([-x * 0.7, -y * 0.8], [0.06, 0.12], GRASS_LIGHT, 1),
            ]);
        }
        TerrainKind::Forest => {
            base(GRASS_DARK);
            parts.extend([
                SpritePart::new([0.0, 0.25], [0.18, 0.38], [0.28, 0.16, 0.07, 1.0], 2),
                SpritePart::new([-0.20, -0.06], [0.48, 0.42], [0.04, 0.20, 0.07, 1.0], 3),
                SpritePart::new([0.18, -0.08], [0.50, 0.46], [0.06, 0.27, 0.09, 1.0], 4),
                SpritePart::new([0.0, -0.28], [0.54, 0.42], [0.09, 0.34, 0.10, 1.0], 5),
                SpritePart::new([-0.12, -0.34], [0.16, 0.12], [0.29, 0.49, 0.13, 1.0], 6),
            ]);
        }
        TerrainKind::Hills => {
            base([0.25, 0.38, 0.14, 1.0]);
            parts.extend([
                SpritePart::new([0.0, 0.28], [0.82, 0.20], [0.25, 0.25, 0.11, 1.0], 1),
                SpritePart::new([0.0, 0.08], [0.62, 0.22], [0.38, 0.43, 0.17, 1.0], 2),
                SpritePart::new([0.0, -0.12], [0.38, 0.20], [0.48, 0.53, 0.22, 1.0], 3),
                SpritePart::new([-0.08, -0.24], [0.16, 0.10], GRASS_LIGHT, 4),
            ]);
        }
        TerrainKind::Mountain => {
            base([0.20, 0.24, 0.25, 1.0]);
            parts.extend([
                SpritePart::new([0.0, 0.28], [0.88, 0.28], STONE_DARK, 1),
                SpritePart::new([0.0, 0.05], [0.66, 0.28], [0.39, 0.41, 0.41, 1.0], 2),
                SpritePart::new([0.0, -0.18], [0.42, 0.28], [0.54, 0.55, 0.53, 1.0], 3),
                SpritePart::new([0.0, -0.37], [0.20, 0.14], [0.82, 0.86, 0.82, 1.0], 4),
                SpritePart::new([-0.19, 0.02], [0.10, 0.40], [0.61, 0.62, 0.58, 1.0], 4),
            ]);
        }
        TerrainKind::Sand => {
            base([0.72, 0.58, 0.30, 1.0]);
            parts.extend([
                SpritePart::new([x, y], [0.12, 0.07], [0.50, 0.36, 0.16, 1.0], 1),
                SpritePart::new([-0.12, 0.23], [0.54, 0.06], [0.86, 0.72, 0.39, 1.0], 1),
            ]);
        }
        TerrainKind::Snow => {
            base([0.79, 0.84, 0.82, 1.0]);
            parts.extend([
                SpritePart::new([x, y], [0.15, 0.08], [0.58, 0.70, 0.73, 1.0], 1),
                SpritePart::new([-0.20, -0.28], [0.28, 0.08], [0.93, 0.94, 0.88, 1.0], 1),
            ]);
        }
        TerrainKind::Swamp => {
            base([0.13, 0.27, 0.17, 1.0]);
            parts.extend([
                SpritePart::new([x * 0.5, 0.20], [0.52, 0.16], [0.08, 0.32, 0.31, 1.0], 1),
                SpritePart::new([-0.28, -0.08], [0.07, 0.42], [0.34, 0.43, 0.14, 1.0], 2),
                SpritePart::new([-0.19, -0.18], [0.06, 0.28], [0.48, 0.51, 0.20, 1.0], 2),
            ]);
        }
        TerrainKind::Dirt => {
            base(DIRT);
            parts.extend([
                SpritePart::new([x, y], [0.13, 0.09], [0.22, 0.13, 0.07, 1.0], 1),
                SpritePart::new([-x, -y], [0.08, 0.06], DIRT_LIGHT, 1),
            ]);
        }
        TerrainKind::Road => {
            base([0.48, 0.36, 0.20, 1.0]);
            parts.extend([
                SpritePart::new([x, y], [0.20, 0.12], DIRT_LIGHT, 1),
                SpritePart::new([-x * 0.8, -y], [0.14, 0.08], [0.28, 0.20, 0.12, 1.0], 1),
            ]);
        }
        TerrainKind::Ocean => {
            base(WATER_DARK);
            parts.extend([
                SpritePart::new([-0.20 + x * 0.25, -0.22], [0.48, 0.07], WATER_LIGHT, 1),
                SpritePart::new([0.24 - x * 0.20, 0.18], [0.38, 0.06], WATER, 1),
            ]);
        }
        TerrainKind::Water => {
            base(WATER);
            parts.extend([
                SpritePart::new([-0.22 + x * 0.2, -0.20], [0.44, 0.07], WATER_LIGHT, 1),
                SpritePart::new([0.20 - x * 0.2, 0.20], [0.34, 0.06], WATER_DARK, 1),
            ]);
        }
        TerrainKind::Bridge => {
            base([0.49, 0.28, 0.10, 1.0]);
            for offset in [-0.375, -0.125, 0.125, 0.375] {
                parts.push(SpritePart::new(
                    [0.0, offset],
                    [1.02, 0.06],
                    [0.22, 0.12, 0.06, 1.0],
                    1,
                ));
            }
            parts.push(SpritePart::new(
                [-0.34, 0.0],
                [0.08, 1.02],
                [0.68, 0.44, 0.17, 1.0],
                2,
            ));
        }
        TerrainKind::StoneFloor => {
            base(STONE);
            parts.extend([
                SpritePart::new([x, y], [0.20, 0.06], STONE_DARK, 1),
                SpritePart::new([-x * 0.7, -y], [0.08, 0.16], STONE_LIGHT, 1),
            ]);
        }
        TerrainKind::Wall => {
            base([0.18, 0.20, 0.21, 1.0]);
            parts.extend([
                SpritePart::new([0.0, -0.35], [1.02, 0.20], STONE_LIGHT, 1),
                SpritePart::new([0.0, -0.20], [1.02, 0.08], STONE_DARK, 2),
                SpritePart::new([-0.27, 0.10], [0.42, 0.10], [0.34, 0.36, 0.36, 1.0], 2),
                SpritePart::new([0.28, 0.31], [0.38, 0.10], [0.29, 0.31, 0.32, 1.0], 2),
            ]);
        }
        TerrainKind::Farmland => {
            base([0.34, 0.25, 0.09, 1.0]);
            for offset in [-0.34, -0.11, 0.12, 0.35] {
                parts.push(SpritePart::new(
                    [offset, 0.0],
                    [0.08, 1.02],
                    [0.53, 0.49, 0.12, 1.0],
                    1,
                ));
            }
            parts.push(SpritePart::new(
                [x * 0.35, y],
                [0.13, 0.18],
                [0.20, 0.39, 0.10, 1.0],
                2,
            ));
        }
        TerrainKind::Rubble => {
            base([0.29, 0.26, 0.23, 1.0]);
            parts.extend([
                SpritePart::new([-0.26, 0.18], [0.34, 0.24], STONE_DARK, 1),
                SpritePart::new([0.22, 0.24], [0.30, 0.20], STONE_LIGHT, 2),
                SpritePart::new([0.13, -0.18], [0.38, 0.28], [0.36, 0.34, 0.31, 1.0], 3),
                SpritePart::new([-0.28, -0.27], [0.20, 0.17], [0.47, 0.43, 0.37, 1.0], 3),
            ]);
        }
        TerrainKind::StairsUp | TerrainKind::StairsDown => {
            let up = terrain == TerrainKind::StairsUp;
            base(if up { STONE } else { STONE_DARK });
            let colors = if up {
                [STONE_DARK, STONE_LIGHT]
            } else {
                [[0.08, 0.09, 0.10, 1.0], STONE]
            };
            for (index, width) in [0.82, 0.66, 0.50, 0.34].into_iter().enumerate() {
                let y = if up {
                    0.30 - index as f32 * 0.20
                } else {
                    -0.30 + index as f32 * 0.20
                };
                parts.push(SpritePart::new(
                    [0.0, y],
                    [width, 0.12],
                    colors[index % 2],
                    1 + index as i64,
                ));
            }
        }
    }
    parts
}

fn player_sprite(_entity: &VisibleEntity) -> Vec<SpritePart> {
    let mut parts = adventurer_sprite(
        Direction::North,
        [0.08, 0.18, 0.42, 1.0],
        [0.78, 0.58, 0.12, 1.0],
        [0.30, 0.16, 0.07, 1.0],
    );

    parts.push(oriented_part(
        Direction::North,
        0.02,
        0.34,
        0.08,
        0.58,
        [0.70, 0.72, 0.67, 1.0],
        10,
    ));
    parts.push(oriented_part(
        Direction::North,
        -0.04,
        -0.30,
        0.24,
        0.42,
        [0.13, 0.15, 0.16, 1.0],
        8,
    ));
    parts.push(oriented_part(
        Direction::North,
        -0.04,
        -0.30,
        0.14,
        0.30,
        [0.58, 0.43, 0.10, 1.0],
        9,
    ));
    parts
}

fn villager_sprite(entity: &VisibleEntity) -> Vec<SpritePart> {
    const CLOTHES: [[f32; 4]; 6] = [
        [0.38, 0.12, 0.09, 1.0],
        [0.08, 0.29, 0.28, 1.0],
        [0.42, 0.28, 0.07, 1.0],
        [0.24, 0.15, 0.34, 1.0],
        [0.16, 0.29, 0.13, 1.0],
        [0.10, 0.20, 0.34, 1.0],
    ];
    let index = (entity.id.0 as usize) % CLOTHES.len();
    adventurer_sprite(
        Direction::North,
        CLOTHES[index],
        [0.53, 0.37, 0.18, 1.0],
        [0.15, 0.09, 0.04, 1.0],
    )
}

fn adventurer_sprite(
    facing: Direction,
    clothing: [f32; 4],
    trim: [f32; 4],
    hair: [f32; 4],
) -> Vec<SpritePart> {
    vec![
        oriented_part(
            facing,
            -0.28,
            0.0,
            0.62,
            0.18,
            [0.015, 0.018, 0.016, 0.62],
            1,
        ),
        oriented_part(
            facing,
            -0.20,
            -0.16,
            0.15,
            0.30,
            [0.08, 0.055, 0.035, 1.0],
            2,
        ),
        oriented_part(
            facing,
            -0.20,
            0.16,
            0.15,
            0.30,
            [0.08, 0.055, 0.035, 1.0],
            2,
        ),
        oriented_part(facing, -0.02, 0.0, 0.58, 0.54, [0.025, 0.03, 0.025, 1.0], 3),
        oriented_part(facing, -0.02, 0.0, 0.46, 0.46, clothing, 4),
        oriented_part(facing, -0.02, -0.13, 0.08, 0.34, trim, 5),
        oriented_part(facing, 0.28, 0.0, 0.34, 0.30, [0.025, 0.03, 0.025, 1.0], 5),
        oriented_part(facing, 0.28, 0.0, 0.24, 0.22, [0.55, 0.36, 0.18, 1.0], 6),
        oriented_part(facing, 0.36, 0.0, 0.25, 0.10, hair, 7),
        oriented_part(facing, 0.40, 0.0, 0.11, 0.06, [0.68, 0.49, 0.26, 1.0], 7),
    ]
}

fn creature_sprite(_entity: &VisibleEntity) -> Vec<SpritePart> {
    vec![
        oriented_part(
            Direction::East,
            -0.24,
            0.0,
            0.74,
            0.17,
            [0.025, 0.015, 0.012, 0.62],
            1,
        ),
        oriented_part(
            Direction::East,
            -0.14,
            -0.27,
            0.12,
            0.34,
            [0.20, 0.055, 0.035, 1.0],
            2,
        ),
        oriented_part(
            Direction::East,
            -0.14,
            0.27,
            0.12,
            0.34,
            [0.20, 0.055, 0.035, 1.0],
            2,
        ),
        oriented_part(
            Direction::East,
            0.12,
            0.0,
            0.55,
            0.66,
            [0.37, 0.075, 0.045, 1.0],
            3,
        ),
        oriented_part(
            Direction::East,
            0.38,
            0.0,
            0.36,
            0.31,
            [0.52, 0.12, 0.065, 1.0],
            4,
        ),
        oriented_part(
            Direction::East,
            0.54,
            0.0,
            0.18,
            0.20,
            [0.23, 0.045, 0.03, 1.0],
            5,
        ),
        oriented_part(
            Direction::East,
            0.42,
            -0.12,
            0.05,
            0.05,
            [0.90, 0.59, 0.10, 1.0],
            5,
        ),
        oriented_part(
            Direction::East,
            0.42,
            0.12,
            0.05,
            0.05,
            [0.90, 0.59, 0.10, 1.0],
            5,
        ),
        oriented_part(
            Direction::East,
            -0.35,
            0.0,
            0.10,
            0.35,
            [0.28, 0.055, 0.035, 1.0],
            3,
        ),
    ]
}

fn item_sprite() -> Vec<SpritePart> {
    vec![
        SpritePart::new([0.0, 0.23], [0.48, 0.13], [0.02, 0.02, 0.015, 0.55], 1),
        SpritePart::new([0.0, 0.03], [0.38, 0.42], [0.12, 0.07, 0.025, 1.0], 2),
        SpritePart::new([0.0, 0.03], [0.28, 0.32], [0.62, 0.43, 0.12, 1.0], 3),
        SpritePart::new([0.0, -0.17], [0.21, 0.08], [0.76, 0.60, 0.22, 1.0], 4),
    ]
}

fn oriented_part(
    facing: Direction,
    forward: f32,
    side: f32,
    side_size: f32,
    forward_size: f32,
    color: [f32; 4],
    layer: i64,
) -> SpritePart {
    let (offset, size) = match facing {
        Direction::North => ([side, -forward], [side_size, forward_size]),
        Direction::East => ([forward, side], [forward_size, side_size]),
        Direction::South => ([-side, forward], [side_size, forward_size]),
        Direction::West => ([-forward, -side], [forward_size, side_size]),
    };
    SpritePart::new(offset, size, color, layer)
}

fn tile_hash(position: GridPos, salt: u64) -> u64 {
    let mut value = (position.x as i64 as u64).wrapping_mul(0x9e37_79b9);
    value ^= (position.y as i64 as u64).rotate_left(21);
    value ^= (position.z as i64 as u64).rotate_left(43);
    value ^= salt.wrapping_mul(0xbf58_476d);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^ (value >> 27)
}

fn hash_axis(hash: u64, shift: u32) -> f32 {
    let unit = ((hash >> shift) & 0x0f) as f32 / 15.0;
    (unit - 0.5) * 0.70
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedQuad {
    pub world_position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub depth: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawList {
    pub clear_color: [f64; 4],
    pub camera_center: [f32; 2],
    pub cell_size: f32,
    pub quads: Vec<ResolvedQuad>,
}

pub trait Projection {
    fn project(
        &self,
        snapshot: &PresentationSnapshot,
        viewport: ViewportSize,
        art_pack: &dyn ArtPack,
    ) -> DrawList;

    fn screen_to_world(
        &self,
        screen: ScreenPoint,
        viewport: ViewportSize,
        center: GridPos,
    ) -> GridPos;
}

#[derive(Clone, Copy, Debug)]
pub struct OverheadProjection {
    pub cell_size: f32,
}

impl Default for OverheadProjection {
    fn default() -> Self {
        Self { cell_size: 24.0 }
    }
}

impl Projection for OverheadProjection {
    fn project(
        &self,
        snapshot: &PresentationSnapshot,
        _viewport: ViewportSize,
        art_pack: &dyn ArtPack,
    ) -> DrawList {
        let mut quads =
            Vec::with_capacity(snapshot.terrain.len() * 3 + snapshot.entities.len() * 9);

        for cell in &snapshot.terrain {
            let base_depth = i64::from(cell.position.y) * 128;
            quads.extend(
                art_pack
                    .terrain_parts(cell.kind, cell.position)
                    .into_iter()
                    .map(|part| ResolvedQuad {
                        world_position: [
                            cell.position.x as f32 + part.offset[0],
                            cell.position.y as f32 + part.offset[1],
                        ],
                        size: part.size,
                        color: part.color,
                        depth: base_depth + part.layer,
                    }),
            );
        }

        for entity in &snapshot.entities {
            let base_depth = entity_depth(entity.position, entity.appearance);
            quads.extend(
                art_pack
                    .entity_parts(entity)
                    .into_iter()
                    .map(|part| ResolvedQuad {
                        world_position: [
                            entity.position.x as f32 + part.offset[0],
                            entity.position.y as f32 + part.offset[1],
                        ],
                        size: part.size,
                        color: part.color,
                        depth: base_depth + part.layer,
                    }),
            );
        }

        quads.sort_by_key(|quad| quad.depth);

        DrawList {
            clear_color: [0.025, 0.030, 0.035, 1.0],
            camera_center: [
                snapshot.camera_center.x as f32,
                snapshot.camera_center.y as f32,
            ],
            cell_size: self.cell_size,
            quads,
        }
    }

    fn screen_to_world(
        &self,
        screen: ScreenPoint,
        viewport: ViewportSize,
        center: GridPos,
    ) -> GridPos {
        let x = center.x as f32 + (screen.x - viewport.width as f32 * 0.5) / self.cell_size;
        let y = center.y as f32 + (screen.y - viewport.height as f32 * 0.5) / self.cell_size;
        GridPos::new(x.round() as i32, y.round() as i32, center.z)
    }
}

fn entity_depth(position: GridPos, appearance: Appearance) -> i64 {
    let layer = if matches!(
        appearance,
        Appearance::Character(CharacterAppearance::Player)
    ) {
        96
    } else {
        64
    };
    i64::from(position.y) * 128 + layer
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    viewport: [f32; 2],
    center: [f32; 2],
    cell_size: f32,
    padding: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UiViewportUniform {
    size: [f32; 2],
    padding: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuQuad {
    world_position: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
}

const GPU_QUAD_ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 8,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 16,
        shader_location: 2,
    },
];

fn gpu_quad_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GpuQuad>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &GPU_QUAD_ATTRIBUTES,
    }
}

fn resolve_ui_quads(draw_list: &UiDrawList) -> Vec<GpuQuad> {
    let mut quads = Vec::new();
    for command in &draw_list.commands {
        match command {
            UiCommand::Rect {
                bounds,
                color,
                clip,
            } => {
                if let Some(bounds) = clipped_rect(*bounds, *clip) {
                    push_ui_quad(&mut quads, bounds, *color);
                }
            }
            UiCommand::Text {
                bounds,
                text,
                style,
                scroll_y,
            } => {
                push_text_quads(&mut quads, *bounds, text, *style, *scroll_y);
            }
        }
    }
    quads
}

fn clipped_rect(bounds: UiRect, clip: Option<UiRect>) -> Option<UiRect> {
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return None;
    }
    let Some(clip) = clip else {
        return Some(bounds);
    };
    let left = bounds.x.max(clip.x);
    let top = bounds.y.max(clip.y);
    let right = (bounds.x + bounds.width).min(clip.x + clip.width);
    let bottom = (bounds.y + bounds.height).min(clip.y + clip.height);
    (right > left && bottom > top).then(|| UiRect::new(left, top, right - left, bottom - top))
}

fn push_ui_quad(quads: &mut Vec<GpuQuad>, bounds: UiRect, color: [f32; 4]) {
    quads.push(GpuQuad {
        world_position: [
            bounds.x + bounds.width * 0.5,
            bounds.y + bounds.height * 0.5,
        ],
        size: [bounds.width, bounds.height],
        color,
    });
}

fn push_text_quads(
    quads: &mut Vec<GpuQuad>,
    bounds: UiRect,
    text: &str,
    style: UiTextStyle,
    scroll_y: f32,
) {
    let scale = style.pixel_scale.max(1.0);
    let advance = 6.0 * scale;
    let line_height = 7.0 * scale + style.line_spacing.max(0.0);
    let right = bounds.x + bounds.width;
    let bottom = bounds.y + bounds.height;
    let mut y = bounds.y - scroll_y.max(0.0);

    for source_line in text.lines() {
        let mut x = bounds.x;
        if source_line.is_empty() {
            y += line_height;
            continue;
        }

        for word in source_line.split_whitespace() {
            let word_width = word.chars().count() as f32 * advance;
            if x > bounds.x && x + word_width > right {
                x = bounds.x;
                y += line_height;
            }
            for character in word.chars() {
                if x + advance > right && x > bounds.x {
                    x = bounds.x;
                    y += line_height;
                }
                if y >= bottom {
                    return;
                }
                push_glyph_quads(quads, bounds, x, y, scale, character, style.color);
                x += advance;
            }
            x += advance;
        }
        y += line_height;
        if y >= bottom {
            return;
        }
    }
}

fn text_layout_height(width: f32, text: &str, style: UiTextStyle) -> f32 {
    if text.is_empty() || width <= 0.0 {
        return 0.0;
    }

    let scale = style.pixel_scale.max(1.0);
    let advance = 6.0 * scale;
    let line_height = 7.0 * scale + style.line_spacing.max(0.0);
    let mut lines = 0usize;

    for source_line in text.lines() {
        let mut x = 0.0;
        lines += 1;
        if source_line.is_empty() {
            continue;
        }

        for word in source_line.split_whitespace() {
            let word_width = word.chars().count() as f32 * advance;
            if x > 0.0 && x + word_width > width {
                x = 0.0;
                lines += 1;
            }
            for _ in word.chars() {
                if x + advance > width && x > 0.0 {
                    x = 0.0;
                    lines += 1;
                }
                x += advance;
            }
            x += advance;
        }
    }

    (lines.saturating_sub(1) as f32 * line_height) + 7.0 * scale
}

fn push_glyph_quads(
    quads: &mut Vec<GpuQuad>,
    clip: UiRect,
    x: f32,
    y: f32,
    scale: f32,
    character: char,
    color: [f32; 4],
) {
    let rows = glyph_rows(character);
    for (row, bits) in rows.into_iter().enumerate() {
        for column in 0..5 {
            if bits & (1 << (4 - column)) == 0 {
                continue;
            }
            let pixel = UiRect::new(
                x + column as f32 * scale,
                y + row as f32 * scale,
                scale,
                scale,
            );
            if let Some(pixel) = clipped_rect(pixel, Some(clip)) {
                push_ui_quad(quads, pixel, color);
            }
        }
    }
}

#[rustfmt::skip]
fn glyph_rows(character: char) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        'A' => [0b01110,0b10001,0b10001,0b11111,0b10001,0b10001,0b10001],
        'B' => [0b11110,0b10001,0b10001,0b11110,0b10001,0b10001,0b11110],
        'C' => [0b01111,0b10000,0b10000,0b10000,0b10000,0b10000,0b01111],
        'D' => [0b11110,0b10001,0b10001,0b10001,0b10001,0b10001,0b11110],
        'E' => [0b11111,0b10000,0b10000,0b11110,0b10000,0b10000,0b11111],
        'F' => [0b11111,0b10000,0b10000,0b11110,0b10000,0b10000,0b10000],
        'G' => [0b01111,0b10000,0b10000,0b10111,0b10001,0b10001,0b01111],
        'H' => [0b10001,0b10001,0b10001,0b11111,0b10001,0b10001,0b10001],
        'I' => [0b11111,0b00100,0b00100,0b00100,0b00100,0b00100,0b11111],
        'J' => [0b00111,0b00010,0b00010,0b00010,0b10010,0b10010,0b01100],
        'K' => [0b10001,0b10010,0b10100,0b11000,0b10100,0b10010,0b10001],
        'L' => [0b10000,0b10000,0b10000,0b10000,0b10000,0b10000,0b11111],
        'M' => [0b10001,0b11011,0b10101,0b10101,0b10001,0b10001,0b10001],
        'N' => [0b10001,0b11001,0b10101,0b10011,0b10001,0b10001,0b10001],
        'O' => [0b01110,0b10001,0b10001,0b10001,0b10001,0b10001,0b01110],
        'P' => [0b11110,0b10001,0b10001,0b11110,0b10000,0b10000,0b10000],
        'Q' => [0b01110,0b10001,0b10001,0b10001,0b10101,0b10010,0b01101],
        'R' => [0b11110,0b10001,0b10001,0b11110,0b10100,0b10010,0b10001],
        'S' => [0b01111,0b10000,0b10000,0b01110,0b00001,0b00001,0b11110],
        'T' => [0b11111,0b00100,0b00100,0b00100,0b00100,0b00100,0b00100],
        'U' => [0b10001,0b10001,0b10001,0b10001,0b10001,0b10001,0b01110],
        'V' => [0b10001,0b10001,0b10001,0b10001,0b10001,0b01010,0b00100],
        'W' => [0b10001,0b10001,0b10001,0b10101,0b10101,0b10101,0b01010],
        'X' => [0b10001,0b10001,0b01010,0b00100,0b01010,0b10001,0b10001],
        'Y' => [0b10001,0b10001,0b01010,0b00100,0b00100,0b00100,0b00100],
        'Z' => [0b11111,0b00001,0b00010,0b00100,0b01000,0b10000,0b11111],
        '0' => [0b01110,0b10001,0b10011,0b10101,0b11001,0b10001,0b01110],
        '1' => [0b00100,0b01100,0b00100,0b00100,0b00100,0b00100,0b01110],
        '2' => [0b01110,0b10001,0b00001,0b00010,0b00100,0b01000,0b11111],
        '3' => [0b11110,0b00001,0b00001,0b01110,0b00001,0b00001,0b11110],
        '4' => [0b00010,0b00110,0b01010,0b10010,0b11111,0b00010,0b00010],
        '5' => [0b11111,0b10000,0b10000,0b11110,0b00001,0b00001,0b11110],
        '6' => [0b01110,0b10000,0b10000,0b11110,0b10001,0b10001,0b01110],
        '7' => [0b11111,0b00001,0b00010,0b00100,0b01000,0b01000,0b01000],
        '8' => [0b01110,0b10001,0b10001,0b01110,0b10001,0b10001,0b01110],
        '9' => [0b01110,0b10001,0b10001,0b01111,0b00001,0b00001,0b01110],
        '.' => [0,0,0,0,0,0b00110,0b00110],
        ',' => [0,0,0,0,0b00110,0b00110,0b00100],
        ':' => [0,0b00110,0b00110,0,0b00110,0b00110,0],
        ';' => [0,0b00110,0b00110,0,0b00110,0b00110,0b00100],
        '!' => [0b00100,0b00100,0b00100,0b00100,0b00100,0,0b00100],
        '?' => [0b01110,0b10001,0b00001,0b00010,0b00100,0,0b00100],
        '\'' => [0b00100,0b00100,0b00010,0,0,0,0],
        '"' => [0b01010,0b01010,0b00100,0,0,0,0],
        '-' | '—' | '–' => [0,0,0,0b11111,0,0,0],
        '/' => [0b00001,0b00010,0b00010,0b00100,0b01000,0b01000,0b10000],
        '(' => [0b00010,0b00100,0b01000,0b01000,0b01000,0b00100,0b00010],
        ')' => [0b01000,0b00100,0b00010,0b00010,0b00010,0b00100,0b01000],
        '[' => [0b01110,0b01000,0b01000,0b01000,0b01000,0b01000,0b01110],
        ']' => [0b01110,0b00010,0b00010,0b00010,0b00010,0b00010,0b01110],
        '+' => [0,0b00100,0b00100,0b11111,0b00100,0b00100,0],
        '=' => [0,0,0b11111,0,0b11111,0,0],
        '%' => [0b11001,0b11010,0b00100,0b01000,0b10110,0b00110,0],
        ' ' => [0; 7],
        _ => [0b01110,0b10001,0b00001,0b00010,0b00100,0,0b00100],
    }
}

/// Draws already-resolved primitives into a caller-owned frame target.
///
/// This type knows nothing about winit, CAMetalLayer, entities, or terrain.
pub struct WgpuRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    ui_pipeline: wgpu::RenderPipeline,
    ui_viewport_buffer: wgpu::Buffer,
    ui_viewport_bind_group: wgpu::BindGroup,
}

impl WgpuRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ultimate-fate-pixel-map"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/colored_grid.wgsl").into()),
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ultimate-fate-camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ultimate-fate-camera-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ultimate-fate-camera-bind-group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ultimate-fate-pipeline-layout"),
            bind_group_layouts: &[&camera_layout],
            push_constant_ranges: &[],
        });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuQuad>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 8,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 2,
                },
            ],
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ultimate-fate-pixel-map-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[instance_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let ui_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ultimate-fate-ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/ui.wgsl").into()),
        });
        let ui_viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ultimate-fate-ui-viewport"),
            size: std::mem::size_of::<UiViewportUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ui_viewport_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ultimate-fate-ui-viewport-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let ui_viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ultimate-fate-ui-viewport-bind-group"),
            layout: &ui_viewport_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ui_viewport_buffer.as_entire_binding(),
            }],
        });
        let ui_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ultimate-fate-ui-pipeline-layout"),
            bind_group_layouts: &[&ui_viewport_layout],
            push_constant_ranges: &[],
        });
        let ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ultimate-fate-ui-pipeline"),
            layout: Some(&ui_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &ui_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[gpu_quad_layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &ui_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            camera_buffer,
            camera_bind_group,
            ui_pipeline,
            ui_viewport_buffer,
            ui_viewport_bind_group,
        }
    }

    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        viewport: ViewportSize,
        draw_list: &DrawList,
        ui_draw_list: &UiDrawList,
    ) {
        let camera = CameraUniform {
            viewport: [viewport.width.max(1) as f32, viewport.height.max(1) as f32],
            center: draw_list.camera_center,
            cell_size: draw_list.cell_size,
            padding: [0.0; 3],
        };
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
        let ui_viewport = UiViewportUniform {
            size: [viewport.width.max(1) as f32, viewport.height.max(1) as f32],
            padding: [0.0; 2],
        };
        queue.write_buffer(
            &self.ui_viewport_buffer,
            0,
            bytemuck::bytes_of(&ui_viewport),
        );

        let gpu_quads: Vec<_> = draw_list
            .quads
            .iter()
            .map(|quad| GpuQuad {
                world_position: quad.world_position,
                size: quad.size,
                color: quad.color,
            })
            .collect();
        let instance_buffer = (!gpu_quads.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ultimate-fate-pixel-map-instances"),
                contents: bytemuck::cast_slice(&gpu_quads),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let ui_quads = resolve_ui_quads(ui_draw_list);
        let ui_instance_buffer = (!ui_quads.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ultimate-fate-ui-instances"),
                contents: bytemuck::cast_slice(&ui_quads),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ultimate-fate-render-encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ultimate-fate-main-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: draw_list.clear_color[0],
                            g: draw_list.clear_color[1],
                            b: draw_list.clear_color[2],
                            a: draw_list.clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if let Some(instance_buffer) = &instance_buffer {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, instance_buffer.slice(..));
                pass.draw(0..6, 0..gpu_quads.len() as u32);
            }
            if let Some(ui_instance_buffer) = &ui_instance_buffer {
                pass.set_pipeline(&self.ui_pipeline);
                pass.set_bind_group(0, &self.ui_viewport_bind_group, &[]);
                pass.set_vertex_buffer(0, ui_instance_buffer.slice(..));
                pass.draw(0..6, 0..ui_quads.len() as u32);
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ultimate_fate_core::Simulation;
    use ultimate_fate_present::ViewportRequest;

    #[test]
    fn overhead_projection_resolves_semantics_without_mutating_snapshot() {
        let simulation = Simulation::demo(7);
        let player = simulation.player();
        let snapshot = PresentationSnapshot::from_simulation(
            &simulation,
            ViewportRequest {
                map: player.position.map,
                center: player.position.grid,
                half_width: 8,
                half_height: 6,
                z: player.position.grid.z,
            },
        );
        let original = snapshot.clone();
        let draw_list = OverheadProjection::default().project(
            &snapshot,
            ViewportSize {
                width: 800,
                height: 600,
            },
            &ClassicArtPack,
        );

        assert_eq!(snapshot, original);
        assert!(draw_list.quads.len() > snapshot.terrain.len() + snapshot.entities.len());
        assert_eq!(
            draw_list
                .quads
                .iter()
                .filter(|quad| quad.size == [1.02, 1.02])
                .count(),
            snapshot.terrain.len()
        );
    }

    #[test]
    fn classic_player_is_a_layered_sprite_instead_of_a_marker() {
        let simulation = Simulation::demo(7);
        let player = simulation.player();
        let snapshot = PresentationSnapshot::from_simulation(
            &simulation,
            ViewportRequest {
                map: player.position.map,
                center: player.position.grid,
                half_width: 1,
                half_height: 1,
                z: player.position.grid.z,
            },
        );
        let visible_player = snapshot
            .entities
            .iter()
            .find(|entity| entity.appearance == Appearance::Character(CharacterAppearance::Player))
            .expect("the player should be visible");

        assert!(ClassicArtPack.entity_parts(visible_player).len() >= 9);
    }

    #[test]
    fn classic_people_remain_upright_regardless_of_movement_direction() {
        let simulation = Simulation::demo(7);
        let player = simulation.player();
        let snapshot = PresentationSnapshot::from_simulation(
            &simulation,
            ViewportRequest {
                map: player.position.map,
                center: player.position.grid,
                half_width: 1,
                half_height: 1,
                z: player.position.grid.z,
            },
        );
        let mut visible_player = *snapshot
            .entities
            .iter()
            .find(|entity| entity.appearance == Appearance::Character(CharacterAppearance::Player))
            .expect("the player should be visible");
        visible_player.facing = Direction::North;
        let north = ClassicArtPack.entity_parts(&visible_player);
        visible_player.facing = Direction::South;

        assert_eq!(north, ClassicArtPack.entity_parts(&visible_player));
    }

    #[test]
    fn overhead_center_picks_camera_cell() {
        let projection = OverheadProjection::default();
        let center = GridPos::new(-30, 17, 2);
        assert_eq!(
            projection.screen_to_world(
                ScreenPoint { x: 400.0, y: 300.0 },
                ViewportSize {
                    width: 800,
                    height: 600
                },
                center
            ),
            center
        );
    }

    #[test]
    fn player_marker_sorts_above_other_entities_on_the_same_tile() {
        let position = GridPos::new(4, 7, 0);
        let player = entity_depth(position, Appearance::Character(CharacterAppearance::Player));
        let villager = entity_depth(
            position,
            Appearance::Character(CharacterAppearance::Villager),
        );

        assert!(player > villager);
    }

    #[test]
    fn bitmap_text_wraps_and_clips_to_its_bounds() {
        let bounds = UiRect::new(10.0, 20.0, 42.0, 24.0);
        let mut ui = UiDrawList::default();
        ui.text(
            bounds,
            "A LONG LINE OF TEXT",
            UiTextStyle {
                pixel_scale: 1.0,
                line_spacing: 1.0,
                ..Default::default()
            },
        );
        let quads = resolve_ui_quads(&ui);

        assert!(!quads.is_empty());
        for quad in quads {
            let left = quad.world_position[0] - quad.size[0] * 0.5;
            let top = quad.world_position[1] - quad.size[1] * 0.5;
            let right = quad.world_position[0] + quad.size[0] * 0.5;
            let bottom = quad.world_position[1] + quad.size[1] * 0.5;
            assert!(left >= bounds.x);
            assert!(top >= bounds.y);
            assert!(right <= bounds.x + bounds.width);
            assert!(bottom <= bounds.y + bounds.height);
        }
    }

    #[test]
    fn tail_text_scrolls_to_the_newest_wrapped_lines() {
        let bounds = UiRect::new(0.0, 0.0, 60.0, 8.0);
        let style = UiTextStyle {
            pixel_scale: 1.0,
            line_spacing: 1.0,
            ..Default::default()
        };
        let mut ui = UiDrawList::default();
        ui.tail_text(bounds, "FIRST\nSECOND\nLATEST", style);

        let UiCommand::Text { scroll_y, .. } = &ui.commands[0] else {
            panic!("tail text should create a text command");
        };
        assert!(*scroll_y > 0.0);

        for quad in resolve_ui_quads(&ui) {
            let top = quad.world_position[1] - quad.size[1] * 0.5;
            let bottom = quad.world_position[1] + quad.size[1] * 0.5;
            assert!(top >= bounds.y);
            assert!(bottom <= bounds.y + bounds.height);
        }
    }

    #[test]
    fn wgpu_pipeline_is_valid_when_an_adapter_is_available() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
        else {
            // Headless CI is allowed to lack a GPU. Desktop and Apple hosts still
            // report adapter creation failures through their platform boundary.
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ultimate-fate-render-test-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .expect("adapter should create a basic WGPU device");

        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let renderer = WgpuRenderer::new(&device, format);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ultimate-fate-render-test-target"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let draw_list = DrawList {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            camera_center: [0.0, 0.0],
            cell_size: 16.0,
            quads: vec![ResolvedQuad {
                world_position: [0.0, 0.0],
                size: [1.0, 1.0],
                color: [1.0, 0.0, 0.0, 1.0],
                depth: 0,
            }],
        };

        renderer.render(
            &device,
            &queue,
            &target_view,
            ViewportSize {
                width: 64,
                height: 64,
            },
            &draw_list,
            &{
                let mut ui = UiDrawList::default();
                ui.bordered_panel(
                    UiRect::new(4.0, 4.0, 56.0, 56.0),
                    [0.05, 0.06, 0.08, 0.95],
                    [0.7, 0.6, 0.3, 1.0],
                    2.0,
                );
                ui.text(
                    UiRect::new(8.0, 8.0, 48.0, 48.0),
                    "TEST UI",
                    UiTextStyle {
                        pixel_scale: 1.0,
                        ..Default::default()
                    },
                );
                ui
            },
        );
    }
}
