use super::super::event_router::{EventRouter, WindowEventRoute};
use winit::event::{DeviceEvent, ElementState, MouseScrollDelta, WindowEvent};

#[test]
fn redraw_events_take_precedence_over_energy_window_routing() {
    let route = EventRouter::classify_window_event(&WindowEvent::RedrawRequested, true, true);

    assert_eq!(route, WindowEventRoute::Redraw);
}

#[test]
fn redraw_events_without_render_resources_are_ignored() {
    let route = EventRouter::classify_window_event(&WindowEvent::RedrawRequested, false, false);

    assert_eq!(route, WindowEventRoute::Ignore);
}

#[test]
fn energy_window_events_route_before_main_window_handling() {
    let route = EventRouter::classify_window_event(&WindowEvent::Focused(true), true, true);

    assert_eq!(route, WindowEventRoute::EnergyWindow);
}

#[test]
fn main_window_events_require_render_resources() {
    let route = EventRouter::classify_window_event(&WindowEvent::Focused(true), false, true);

    assert_eq!(route, WindowEventRoute::MainWindow);
}

#[test]
fn non_energy_events_for_other_windows_are_ignored() {
    let route = EventRouter::classify_window_event(&WindowEvent::CloseRequested, false, false);

    assert_eq!(route, WindowEventRoute::Ignore);
}

#[test]
fn non_redraw_events_without_resources_are_ignored() {
    let route = EventRouter::classify_window_event(
        &WindowEvent::MouseWheel {
            delta: MouseScrollDelta::LineDelta(0.0, 1.0),
            phase: winit::event::TouchPhase::Moved,
            device_id: winit::event::DeviceId::dummy(),
        },
        false,
        false,
    );

    assert_eq!(route, WindowEventRoute::Ignore);
}

#[test]
fn released_button_device_events_resume_after_resize() {
    let should_resume = EventRouter::should_resume_after_device_event(&DeviceEvent::Button {
        button: 1,
        state: ElementState::Released,
    });

    assert!(should_resume);
}

#[test]
fn pressed_button_device_events_do_not_resume_after_resize() {
    let should_resume = EventRouter::should_resume_after_device_event(&DeviceEvent::Button {
        button: 1,
        state: ElementState::Pressed,
    });

    assert!(!should_resume);
}

#[test]
fn non_button_device_events_do_not_resume_after_resize() {
    let should_resume = EventRouter::should_resume_after_device_event(&DeviceEvent::MouseMotion {
        delta: (1.0, -1.0),
    });

    assert!(!should_resume);
}
