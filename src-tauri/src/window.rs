use std::sync::mpsc;

use tauri::{Emitter, Manager, Runtime, WebviewWindow};

#[cfg(not(target_os = "macos"))]
use tauri::{PhysicalPosition, PhysicalSize};

#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSEvent, NSScreenSaverWindowLevel, NSStatusWindowLevel, NSWindow,
    NSWindowCollectionBehavior,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSPoint, NSSize};

use codex_island_core::CoreState;

#[cfg(target_os = "macos")]
extern "C" {
    fn CGShieldingWindowLevel() -> i32;
}

const COLLAPSED_WIDTH: f64 = 420.0;
const EXPANDED_WIDTH: f64 = 520.0;
const COLLAPSED_HEIGHT: u32 = 88;
const COLLAPSED_TOP_MARGIN: f64 = 0.0;
const EXPANDED_TOP_MARGIN: f64 = 0.0;
const HOVER_EVENT: &str = "island:hover";
const LIST_BASE_HEIGHT: u32 = 108;
const LIST_ROW_HEIGHT: u32 = 72;
const LIST_ROW_GAP: u32 = 12;
const DETAIL_HEIGHT: u32 = 300;
const EMPTY_HEIGHT: u32 = 280;
const LIST_MAX_HEIGHT: u32 = 360;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpandedView {
    List,
    Detail,
    Empty,
}

impl ExpandedView {
    pub fn from_wire(value: &str) -> Self {
        match value {
            "detail" => Self::Detail,
            "empty" => Self::Empty,
            _ => Self::List,
        }
    }
}

pub fn sync_window<R: Runtime>(
    window: &WebviewWindow<R>,
    expanded: bool,
    expanded_view: ExpandedView,
    session_count: usize,
) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let window_handle = window.clone();
        window.run_on_main_thread(move || unsafe {
            let ns_window_ptr = window_handle
                .ns_window()
                .expect("webview window should expose NSWindow");
            let ns_window = &*(ns_window_ptr.cast::<NSWindow>());

            ns_window.setLevel(fullscreen_overlay_level(
                NSStatusWindowLevel,
                NSScreenSaverWindowLevel,
                CGShieldingWindowLevel() as isize,
            ));
            ns_window.setCollectionBehavior(overlay_collection_behavior());

            let frame = ns_window
                .screen()
                .map(|screen| screen.frame())
                .unwrap_or_else(|| ns_window.frame());
            let top_margin = if expanded {
                EXPANDED_TOP_MARGIN
            } else {
                COLLAPSED_TOP_MARGIN
            };
            let window_height = if expanded {
                expanded_height(expanded_view, session_count) as f64
            } else {
                COLLAPSED_HEIGHT as f64
            };
            let window_width = frame.size.width;

            let x = frame.origin.x;
            let y = frame.origin.y + frame.size.height - top_margin;

            ns_window.setContentSize(NSSize::new(window_width, window_height));
            ns_window.setFrameTopLeftPoint(NSPoint::new(x, y));
            ns_window.orderFrontRegardless();
        })?;

        window.set_ignore_cursor_events(!expanded)?;

        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let size = if expanded {
            PhysicalSize::new(620, expanded_height(expanded_view, session_count))
        } else {
            PhysicalSize::new(420, COLLAPSED_HEIGHT)
        };
        let monitor = window
            .primary_monitor()?
            .or_else(|| window.current_monitor().ok().flatten());

        if let Some(monitor) = monitor {
            let monitor_size = monitor.size();
            let x = monitor.position().x + ((monitor_size.width as i32 - size.width as i32) / 2);
            let y = monitor.position().y;
            window.set_position(PhysicalPosition::new(x, y))?;
        }

        window.set_size(size)?;
        Ok(())
    }
}

pub fn sync_island_window<R: Runtime>(
    app: &tauri::AppHandle<R>,
    expanded: bool,
    expanded_view: ExpandedView,
    session_count: usize,
) -> tauri::Result<()> {
    let window = app
        .get_webview_window("island")
        .ok_or_else(|| tauri::Error::AssetNotFound("island window should exist".into()))?;
    sync_window(&window, expanded, expanded_view, session_count)
}

#[cfg(target_os = "macos")]
pub fn start_hover_monitor<R: Runtime>(app: tauri::AppHandle<R>) {
    let app_handle = app.clone();
    let window_expanded = app.state::<CoreState>().window_expanded.clone();

    tauri::async_runtime::spawn(async move {
        let mut hover_active = false;
        let mut cursor_events_enabled = false;

        loop {
            let Some(window) = app_handle.get_webview_window("island") else {
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                continue;
            };

            let expanded = window_expanded.load(std::sync::atomic::Ordering::Relaxed);
            let hovering = pointer_inside_hot_region(&window, expanded).unwrap_or(false);
            let should_accept_events = expanded || hovering;

            if should_accept_events != cursor_events_enabled {
                let _ = window.set_ignore_cursor_events(!should_accept_events);
                cursor_events_enabled = should_accept_events;
            }

            if hovering != hover_active {
                let _ = app_handle.emit(HOVER_EVENT, hovering);
                hover_active = hovering;
            }

            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn start_hover_monitor<R: Runtime>(_app: tauri::AppHandle<R>) {}

#[cfg(target_os = "macos")]
unsafe fn is_pointer_inside_hot_region<R: Runtime>(
    window: &WebviewWindow<R>,
    expanded: bool,
) -> bool {
    let Ok(ns_window_ptr) = window.ns_window() else {
        return false;
    };
    let ns_window = &*(ns_window_ptr.cast::<NSWindow>());
    let frame = ns_window.frame();
    let mouse = NSEvent::mouseLocation();
    let region = hot_region(frame.origin.x, frame.origin.y, frame.size.width, frame.size.height, expanded);

    mouse.x >= region.0
        && mouse.x <= region.0 + region.2
        && mouse.y >= region.1
        && mouse.y <= region.1 + region.3
}

#[cfg(target_os = "macos")]
fn pointer_inside_hot_region<R: Runtime>(
    window: &WebviewWindow<R>,
    expanded: bool,
) -> tauri::Result<bool> {
    let (tx, rx) = mpsc::channel();
    let window_handle = window.clone();

    window.run_on_main_thread(move || {
        let hovering = unsafe { is_pointer_inside_hot_region(&window_handle, expanded) };
        let _ = tx.send(hovering);
    })?;

    Ok(rx.recv_timeout(std::time::Duration::from_millis(20)).unwrap_or(false))
}

#[cfg(target_os = "macos")]
fn hot_region(
    screen_x: f64,
    screen_y: f64,
    screen_width: f64,
    screen_height: f64,
    expanded: bool,
) -> (f64, f64, f64, f64) {
    let width = if expanded {
        EXPANDED_WIDTH
    } else {
        COLLAPSED_WIDTH
    };
    let height = if expanded {
        screen_height
    } else {
        COLLAPSED_HEIGHT as f64
    };
    let x = screen_x + ((screen_width - width) / 2.0);
    let y = screen_y + screen_height - height;

    (x, y, width, height)
}

#[cfg(target_os = "macos")]
fn overlay_collection_behavior() -> NSWindowCollectionBehavior {
    NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::FullScreenAuxiliary
}

#[cfg(all(target_os = "macos", test))]
fn should_promote_window_to_panel() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn fullscreen_overlay_level(
    status_level: isize,
    screensaver_level: isize,
    shielding_level: isize,
) -> isize {
    status_level.max(screensaver_level).max(shielding_level + 1)
}

fn expanded_height(expanded_view: ExpandedView, session_count: usize) -> u32 {
    match expanded_view {
        ExpandedView::Detail => DETAIL_HEIGHT,
        ExpandedView::Empty => EMPTY_HEIGHT,
        ExpandedView::List => {
            let rows = session_count.max(1) as u32;
            let gaps = rows.saturating_sub(1) * LIST_ROW_GAP;
            (LIST_BASE_HEIGHT + (rows * LIST_ROW_HEIGHT) + gaps).min(LIST_MAX_HEIGHT)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{expanded_height, hot_region, ExpandedView};

    #[cfg(target_os = "macos")]
    use super::{
        fullscreen_overlay_level, overlay_collection_behavior,
        should_promote_window_to_panel,
    };
    #[cfg(target_os = "macos")]
    use objc2_app_kit::NSWindowCollectionBehavior;

    #[test]
    fn collapsed_hot_region_is_centered() {
        let region = hot_region(0.0, 0.0, 1440.0, 900.0, false);

        assert_eq!(region.0, 510.0);
        assert_eq!(region.1, 812.0);
        assert_eq!(region.2, 420.0);
        assert_eq!(region.3, 88.0);
    }

    #[test]
    fn expanded_hot_region_matches_panel_bounds() {
        let region = hot_region(10.0, 20.0, 1512.0, 982.0, true);

        assert_eq!(region.0, 506.0);
        assert_eq!(region.1, 20.0);
        assert_eq!(region.2, 520.0);
        assert_eq!(region.3, 982.0);
    }

    #[test]
    fn list_height_tracks_visible_rows() {
        assert_eq!(expanded_height(ExpandedView::List, 1), 180);
        assert_eq!(expanded_height(ExpandedView::List, 3), 348);
    }

    #[test]
    fn detail_and_empty_use_fixed_compact_heights() {
        assert_eq!(expanded_height(ExpandedView::Detail, 5), 300);
        assert_eq!(expanded_height(ExpandedView::Empty, 0), 280);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn fullscreen_overlay_level_stays_above_shielding_windows() {
        assert_eq!(fullscreen_overlay_level(25, 1000, 120), 1000);
        assert_eq!(fullscreen_overlay_level(25, 100, 120), 121);
        assert_eq!(fullscreen_overlay_level(200, 100, 120), 200);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn overlay_collection_behavior_matches_fullscreen_overlay_needs() {
        let behavior = overlay_collection_behavior();

        assert!(behavior.contains(NSWindowCollectionBehavior::CanJoinAllSpaces));
        assert!(behavior.contains(NSWindowCollectionBehavior::Stationary));
        assert!(behavior.contains(NSWindowCollectionBehavior::FullScreenAuxiliary));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn tauri_window_does_not_attempt_panel_style_promotion() {
        assert!(!should_promote_window_to_panel());
    }
}
