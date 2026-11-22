//! Exact 1:1 Rust translation of Ping Wheel
//! (LukenSkyne/Minecraft-Ping-Wheel, MIT).
//!
//! Upstream Java package nx.pingwheel maps to the modules below; every
//! Minecraft client/server type sits behind the host seams in `seams`. Log
//! lines, field names, branch order and quirks are kept as-is.
//!
//! - `global`        <- common/Global.java
//! - `compat`        <- common/compat/
//! - `config`        <- common/config/
//! - `network`       <- common/network/
//! - `math`          <- common/math/
//! - `util`          <- common/util/
//! - `core`          <- common/core/ (GameContext, PingData, PingView,
//!     PingController, PingManager, ServerCore)
//! - `render`        <- common/render/ (DrawContext, OverlayRenderer,
//!     DirectionIndicatorRenderer, PingLocationRenderer)
//! - `common_client` <- common/CommonClient.java
//! - `screen`, `integration`, `resource`, `seams`, `tests`
//!
//! The mod API carries the registrations: the original's config fields are
//! registered as settings (same defaults and ranges as the mod's own settings
//! screen) and the ping key binding is registered on Options > Controls with
//! the original's default. The client tick hook bootstraps
//! CommonClient::on_init on first tick and then runs the tick-start callback
//! it registered through the event-service seam; the input hook feeds the
//! ping-key consume path through the key-mapping seam. What the mod API does
//! not expose yet stays behind the seams and is documented in the manifest:
//! the HUD overlay draw (the game's GUI pose stack and sprite draw), the
//! world-raycast read that places a ping, the client-to-server custom channel
//! the packets ride, and the live-binding read for the settings key.

pub mod global;
pub mod common_client;
pub mod integration;
pub mod screen;
pub mod render;
pub mod compat;
pub mod config;
pub mod network;
pub mod math;
pub mod util;
pub mod resource;
pub mod core;
pub mod seams;

#[cfg(test)]
mod tests;

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use mod_api::{self, ClientHost, InputKeyEvent, KeyAction, RegistrationContext, Registrar, SettingSpec};

use crate::config::{ClientConfig, PlayerInfoMode, TeamColorMode};

/// The registered ping key's press, tracked from the input hook. The live
/// binding query stays host-side (the key-mapping seam has no read), so the
/// default binding the registration carries is what this watches: the
/// original's default, mouse button 5.
static PING_KEY_PRESSED: AtomicBool = AtomicBool::new(false);

/// Takes the tracked ping-key press, consuming it (the next read sees false).
pub fn take_ping_key_pressed() -> bool {
    PING_KEY_PRESSED.swap(false, Ordering::Relaxed)
}

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl mod_api::Hooks for Mod {
    /// The original's config fields registered as settings, same defaults and
    /// ranges as the mod's own settings screen, and the ping key binding on
    /// Options > Controls (the original's mouse-button-5 default).
    fn register(&self, registrar: &mut dyn Registrar, _context: &RegistrationContext) {
        let cfg = ClientConfig::default();

        let _ = registrar.register_setting(SettingSpec::range(
            "ping_volume",
            "settings.pingwheel.ping_volume",
            0.0,
            100.0,
            1.0,
            cfg.ping_volume as f64,
        ));
        let _ = registrar.register_setting(SettingSpec::range(
            "ping_duration",
            "settings.pingwheel.ping_duration",
            1.0,
            crate::config::MAX_PING_DURATION as f64,
            1.0,
            cfg.ping_duration as f64,
        ));
        let _ = registrar.register_setting(SettingSpec::range(
            "ping_distance",
            "settings.pingwheel.ping_distance",
            0.0,
            crate::config::MAX_PING_DISTANCE as f64,
            16.0,
            cfg.ping_distance as f64,
        ));
        let _ = registrar.register_setting(SettingSpec::range(
            "correction_period",
            "settings.pingwheel.correction_period",
            0.1,
            crate::config::MAX_CORRECTION_PERIOD as f64,
            0.1,
            cfg.correction_period as f64,
        ));
        let _ = registrar.register_setting(SettingSpec::toggle(
            "item_icon_visible",
            "settings.pingwheel.item_icon_visible",
            cfg.item_icon_visible,
        ));
        let _ = registrar.register_setting(SettingSpec::toggle(
            "direction_indicator_visible",
            "settings.pingwheel.direction_indicator_visible",
            cfg.direction_indicator_visible,
        ));
        let _ = registrar.register_setting(SettingSpec::choice(
            "player_info_mode",
            "settings.pingwheel.player_info_mode",
            ["value.pingwheel.hold", "value.pingwheel.disabled", "value.pingwheel.always", "value.pingwheel.compact"],
            PlayerInfoMode::VALUES
                .iter()
                .position(|mode| *mode == cfg.player_info_mode)
                .unwrap_or(0),
        ));
        let _ = registrar.register_setting(SettingSpec::choice(
            "team_color_mode",
            "settings.pingwheel.team_color_mode",
            ["value.pingwheel.full", "value.pingwheel.disabled", "value.pingwheel.ping_only", "value.pingwheel.labels_only"],
            TeamColorMode::VALUES
                .iter()
                .position(|mode| *mode == cfg.team_color_mode)
                .unwrap_or(0),
        ));
        let _ = registrar.register_setting(SettingSpec::range(
            "ping_size",
            "settings.pingwheel.ping_size",
            40.0,
            300.0,
            10.0,
            cfg.ping_size as f64,
        ));

        registrar.register_key_mapping(
            mod_api::KeyRegistration::new("ping", "key.mouse.5")
                .label("key.pingwheel.ping_location")
                .category("key.category.pingwheel.name"),
        );
    }

    fn input_key(&self, _host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        // The original consumes the ping binding per tick through the host's
        // key-mapping service; a live-binding read stays host-side, so the
        // default binding this registers is what feeds the same consume path.
        if event.key == "key.mouse.5" && event.action == KeyAction::Press && event.modifiers == 0 {
            PING_KEY_PRESSED.store(true, Ordering::Relaxed);
        }
    }

    fn client_tick_pre(&self, _host: &mut dyn ClientHost, _event: &mut mod_api::ClientTickPreEvent) {
        // CommonClient::on_init registers the tick-start callback through the
        // event-service seam; the real service installed here drives it from
        // this hook (first tick bootstraps it).
        install_real_seams();
        drain_tick_start();
    }
}

// ---------------------------------------------------------------------------
// The real seam services the hooks drive.
// ---------------------------------------------------------------------------

thread_local! {
    static TICK_START_CALLBACKS: std::cell::RefCell<Vec<Rc<dyn Fn()>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The client event service the tick hook drives: the tick-start callback
/// CommonClient::on_init registers runs per tick; the join/leave and render
/// callbacks stay parked (their surfaces are mod API gaps, see the manifest).
struct RealClientEventService;

impl crate::seams::IPlatformClientEventService for RealClientEventService {
    fn register_tick_start_event(&self, cb: Rc<dyn Fn()>) {
        TICK_START_CALLBACKS.with(|callbacks| callbacks.borrow_mut().push(cb));
    }
    fn register_join_server_event(&self, _cb: Rc<dyn Fn()>) {}
    fn register_leave_server_event(&self, _cb: Rc<dyn Fn()>) {}
    fn register_render_world_event(&self, _cb: Rc<dyn Fn(&crate::core::WorldRenderContext)>) {}
    fn register_render_gui_event(&self, _cb: Rc<dyn Fn(crate::seams::PoseStack, f32)>) {}
}

/// Installs the real seam services once and runs CommonClient::on_init, which
/// loads the config and registers the callbacks through the seams.
fn install_real_seams() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        crate::seams::set_client_event_service(Rc::new(RealClientEventService));
        crate::common_client::CommonClient::on_init();
    });
}

/// Runs the registered tick-start callbacks (CommonClient::on_tick_start).
fn drain_tick_start() {
    let callbacks = TICK_START_CALLBACKS.with(|callbacks| callbacks.borrow().clone());
    for callback in callbacks {
        callback();
    }
}
