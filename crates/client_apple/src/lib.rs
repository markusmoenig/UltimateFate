//! C ABI consumed by the thin Swift/Xcode application host.
//!
//! The native host owns the CAMetalLayer and application lifecycle. This library
//! owns the game simulation and WGPU rendering state.

use std::{ffi::c_void, time::Duration};

use ultimate_fate_core::{Direction, GameCommand, MapId};
use ultimate_fate_input::{DigitalInput, GameplayButton, InputAction, InputController};
use ultimate_fate_present::{PresentationSnapshot, ViewportRequest};
use ultimate_fate_render::{
    ClassicArtPack, OverheadProjection, Projection, UiDrawList, ViewportSize, WgpuRenderer,
};
use ultimate_fate_session::{CampaignCommand, CampaignSession};

const REGIONAL_VIEW_SCALE: f32 = 0.55;
const WORLD_HEARTBEAT: Duration = Duration::from_millis(600);

pub struct UltimateFateAppleClient {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: WgpuRenderer,
    campaign: CampaignSession,
    projection: OverheadProjection,
    regional_map: MapId,
    art_pack: ClassicArtPack,
    input: InputController,
    idle_elapsed: Duration,
}

impl UltimateFateAppleClient {
    #[cfg(target_vendor = "apple")]
    fn new(
        layer: *mut c_void,
        width: u32,
        height: u32,
        scale: f32,
        campaign_seed: u64,
    ) -> Result<Self, String> {
        if layer.is_null() {
            return Err("CAMetalLayer pointer is null".into());
        }

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        // SAFETY: The Xcode host contract requires `layer` to point to a live
        // CAMetalLayer which outlives this client and its WGPU surface.
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer))
        }
        .map_err(|error| format!("failed to create CAMetalLayer surface: {error}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|error| format!("failed to request GPU adapter: {error}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ultimate-fate-apple-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .map_err(|error| format!("failed to request GPU device: {error}"))?;
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| "surface exposes no texture formats".to_string())?;
        let present_mode = if capabilities
            .present_modes
            .contains(&wgpu::PresentMode::Fifo)
        {
            wgpu::PresentMode::Fifo
        } else {
            capabilities
                .present_modes
                .first()
                .copied()
                .ok_or_else(|| "surface exposes no present modes".to_string())?
        };
        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Opaque);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        let renderer = WgpuRenderer::new(&device, format);
        let campaign = CampaignSession::new(campaign_seed)?;
        let regional_map = campaign.site_plan().regional_map;

        Ok(Self {
            surface,
            device,
            queue,
            config,
            renderer,
            campaign,
            projection: OverheadProjection {
                cell_size: 24.0 * scale.max(1.0),
            },
            regional_map,
            art_pack: ClassicArtPack,
            input: InputController::default(),
            idle_elapsed: Duration::ZERO,
        })
    }

    fn resize(&mut self, width: u32, height: u32, scale: f32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.projection.cell_size = 24.0 * scale.max(1.0);
        self.surface.configure(&self.device, &self.config);
    }

    fn update(&mut self, elapsed_seconds: f64) {
        if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 {
            return;
        }

        let elapsed_seconds = elapsed_seconds.min(1.0);
        let elapsed = Duration::from_secs_f64(elapsed_seconds);
        let tick_before_input = self.campaign.simulation().tick;
        self.input.update(elapsed);
        let input_events = self.input.drain_events().collect::<Vec<_>>();
        for event in input_events {
            self.apply_input_action(event.action);
        }
        if self.campaign.simulation().tick == tick_before_input
            && !self.campaign.simulation().paused
        {
            self.idle_elapsed += elapsed;
            if self.idle_elapsed >= WORLD_HEARTBEAT {
                self.apply_game_command(GameCommand::Wait);
            }
        }
    }

    fn set_input(&mut self, input: u32, pressed: bool) {
        if let Some(input) = decode_input(input) {
            self.input.set_digital(input, pressed);
        }
    }

    fn set_movement(&mut self, x: f32, y: f32) {
        self.input.set_movement_axis(x, y, 0.3);
    }

    fn apply_input_action(&mut self, action: InputAction) {
        match action {
            InputAction::Move(direction) => {
                self.apply_game_command(GameCommand::Move(direction));
            }
            InputAction::Button(GameplayButton::Back) | InputAction::Menu => {
                self.apply_game_command(GameCommand::Pause);
            }
            InputAction::Button(GameplayButton::Primary) => {
                self.apply_campaign_command(CampaignCommand::Interact);
            }
            InputAction::Button(GameplayButton::Inspect) => {
                self.apply_campaign_command(CampaignCommand::InspectHere);
            }
            InputAction::Button(GameplayButton::Journal) => {
                // The native host will present the journal view. Keeping this
                // button semantic avoids platform key knowledge in the campaign.
            }
        }
    }

    fn command(&mut self, command: u32) {
        let command = match command {
            0 => GameCommand::Move(Direction::North),
            1 => GameCommand::Move(Direction::East),
            2 => GameCommand::Move(Direction::South),
            3 => GameCommand::Move(Direction::West),
            4 => GameCommand::Wait,
            5 => GameCommand::Pause,
            _ => return,
        };
        self.apply_game_command(command);
    }

    fn apply_game_command(&mut self, command: GameCommand) {
        self.apply_campaign_command(CampaignCommand::Game(command));
    }

    fn apply_campaign_command(&mut self, command: CampaignCommand) {
        let outcome = self.campaign.apply_command(command);
        if outcome.advanced_time() || outcome.changed_world() {
            self.idle_elapsed = Duration::ZERO;
        }
    }

    fn render(&self) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let viewport = ViewportSize {
            width: self.config.width,
            height: self.config.height,
        };
        let simulation = self.campaign.simulation();
        let player = simulation.player();
        let map_scale = if player.position.map == self.regional_map {
            REGIONAL_VIEW_SCALE
        } else {
            1.0
        };
        let projection = OverheadProjection {
            cell_size: self.projection.cell_size * map_scale,
        };
        let half_width = (self.config.width as f32 / projection.cell_size * 0.5).ceil() as i32 + 1;
        let half_height =
            (self.config.height as f32 / projection.cell_size * 0.5).ceil() as i32 + 1;
        let snapshot = PresentationSnapshot::from_simulation(
            simulation,
            ViewportRequest {
                map: player.position.map,
                center: player.position.grid,
                half_width,
                half_height,
                z: player.position.grid.z,
            },
        );
        let draw_list = projection.project(&snapshot, viewport, &self.art_pack);
        self.renderer.render(
            &self.device,
            &self.queue,
            &view,
            viewport,
            &draw_list,
            &UiDrawList::default(),
        );
        frame.present();
        Ok(())
    }
}

fn decode_input(input: u32) -> Option<DigitalInput> {
    match input {
        0 => Some(DigitalInput::Move(Direction::North)),
        1 => Some(DigitalInput::Move(Direction::East)),
        2 => Some(DigitalInput::Move(Direction::South)),
        3 => Some(DigitalInput::Move(Direction::West)),
        4 => Some(DigitalInput::Button(GameplayButton::Primary)),
        5 => Some(DigitalInput::Button(GameplayButton::Back)),
        6 => Some(DigitalInput::Button(GameplayButton::Inspect)),
        7 => Some(DigitalInput::Button(GameplayButton::Journal)),
        8 => Some(DigitalInput::Menu),
        _ => None,
    }
}

#[cfg(target_vendor = "apple")]
#[unsafe(no_mangle)]
pub extern "C" fn ultimate_fate_client_create(
    layer: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
    campaign_seed: u64,
) -> *mut UltimateFateAppleClient {
    UltimateFateAppleClient::new(layer, width, height, scale, campaign_seed)
        .map(Box::new)
        .map(Box::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(not(target_vendor = "apple"))]
#[unsafe(no_mangle)]
pub extern "C" fn ultimate_fate_client_create(
    _layer: *mut c_void,
    _width: u32,
    _height: u32,
    _scale: f32,
    _campaign_seed: u64,
) -> *mut UltimateFateAppleClient {
    std::ptr::null_mut()
}

#[unsafe(no_mangle)]
/// # Safety
///
/// `client` must be null or a live pointer returned by
/// [`ultimate_fate_client_create`], and it must not have been destroyed before.
pub unsafe extern "C" fn ultimate_fate_client_destroy(client: *mut UltimateFateAppleClient) {
    if !client.is_null() {
        // SAFETY: The pointer was returned by `Box::into_raw` in create and the
        // native host promises to destroy it exactly once.
        drop(unsafe { Box::from_raw(client) });
    }
}

#[unsafe(no_mangle)]
/// # Safety
///
/// `client` must be null or a live pointer returned by
/// [`ultimate_fate_client_create`]. Calls for a client must be serialized.
pub unsafe extern "C" fn ultimate_fate_client_resize(
    client: *mut UltimateFateAppleClient,
    width: u32,
    height: u32,
    scale: f32,
) {
    // SAFETY: The native host passes either null or its live client pointer.
    if let Some(client) = unsafe { client.as_mut() } {
        client.resize(width, height, scale);
    }
}

#[unsafe(no_mangle)]
/// # Safety
///
/// `client` must be null or a live pointer returned by
/// [`ultimate_fate_client_create`]. Calls for a client must be serialized.
pub unsafe extern "C" fn ultimate_fate_client_update(
    client: *mut UltimateFateAppleClient,
    elapsed_seconds: f64,
) {
    // SAFETY: The native host passes either null or its live client pointer.
    if let Some(client) = unsafe { client.as_mut() } {
        client.update(elapsed_seconds);
    }
}

#[unsafe(no_mangle)]
/// # Safety
///
/// `client` must be null or a live pointer returned by
/// [`ultimate_fate_client_create`]. Calls for a client must be serialized.
pub unsafe extern "C" fn ultimate_fate_client_command(
    client: *mut UltimateFateAppleClient,
    command: u32,
) {
    // SAFETY: The native host passes either null or its live client pointer.
    if let Some(client) = unsafe { client.as_mut() } {
        client.command(command);
    }
}

#[unsafe(no_mangle)]
/// Sets one device-neutral input control. `pressed` is zero for released and
/// non-zero for pressed. Calls for a client must be serialized.
///
/// # Safety
///
/// `client` must be null or a live pointer returned by
/// [`ultimate_fate_client_create`].
pub unsafe extern "C" fn ultimate_fate_client_set_input(
    client: *mut UltimateFateAppleClient,
    input: u32,
    pressed: u32,
) {
    // SAFETY: The native host passes either null or its live client pointer.
    if let Some(client) = unsafe { client.as_mut() } {
        client.set_input(input, pressed != 0);
    }
}

#[unsafe(no_mangle)]
/// Sets a virtual joystick or controller movement vector. Values are expected
/// in `-1...1`; the shared input layer applies a dead zone and cardinalizes it.
///
/// # Safety
///
/// `client` must be null or a live pointer returned by
/// [`ultimate_fate_client_create`].
pub unsafe extern "C" fn ultimate_fate_client_set_movement(
    client: *mut UltimateFateAppleClient,
    x: f32,
    y: f32,
) {
    // SAFETY: The native host passes either null or its live client pointer.
    if let Some(client) = unsafe { client.as_mut() } {
        client.set_movement(x, y);
    }
}

#[unsafe(no_mangle)]
/// # Safety
///
/// `client` must be null or a live pointer returned by
/// [`ultimate_fate_client_create`]. Calls for a client must be serialized, and
/// the CAMetalLayer passed at creation must still be alive.
pub unsafe extern "C" fn ultimate_fate_client_render(client: *mut UltimateFateAppleClient) -> u32 {
    // SAFETY: The native host passes either null or its live client pointer.
    let Some(client) = (unsafe { client.as_mut() }) else {
        return 4;
    };

    match client.render() {
        Ok(()) => 0,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            client.surface.configure(&client.device, &client.config);
            1
        }
        Err(wgpu::SurfaceError::Timeout) => 2,
        Err(wgpu::SurfaceError::OutOfMemory) => 3,
        Err(wgpu::SurfaceError::Other) => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_input_codes_cover_movement_four_buttons_and_menu() {
        assert_eq!(decode_input(0), Some(DigitalInput::Move(Direction::North)));
        assert_eq!(
            decode_input(4),
            Some(DigitalInput::Button(GameplayButton::Primary))
        );
        assert_eq!(
            decode_input(7),
            Some(DigitalInput::Button(GameplayButton::Journal))
        );
        assert_eq!(decode_input(8), Some(DigitalInput::Menu));
        assert_eq!(decode_input(9), None);
    }
}
