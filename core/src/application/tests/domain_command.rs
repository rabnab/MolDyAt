use crate::application::DomainCommand;
use crate::application::operator_command::AddLineSegment;
use crate::application::physics_thread::PhysicsCommand;
use crate::objects2d::{Rect, Vec2D};

#[test]
fn test_domain_command_maps_toggle_pause_to_physics_command() {
    let command: PhysicsCommand = DomainCommand::TogglePause.into();
    assert!(matches!(command, PhysicsCommand::TogglePause));
}

#[test]
fn test_domain_command_maps_resize_to_physics_command() {
    let world_rect = Rect::new(1.0, 2.0, 3.0, 4.0);
    let command: PhysicsCommand = DomainCommand::Resize { world_rect }.into();

    assert!(matches!(
        command,
        PhysicsCommand::Resize {
            world_rect: mapped_rect
        } if mapped_rect == world_rect
    ));
}

#[test]
fn test_domain_command_maps_operator_to_physics_command() {
    let command: PhysicsCommand = DomainCommand::Operator(Box::new(AddLineSegment {
        start: Vec2D::new(1.0, 2.0),
        end: Vec2D::new(3.0, 4.0),
    }))
    .into();

    assert!(matches!(command, PhysicsCommand::Operator(_)));
}

#[test]
fn test_domain_command_maps_resume_after_resize_to_physics_command() {
    let command: PhysicsCommand = DomainCommand::ResumeAfterResize.into();
    assert!(matches!(command, PhysicsCommand::ResumeAfterResize));
}
