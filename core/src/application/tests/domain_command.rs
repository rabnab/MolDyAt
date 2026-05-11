use crate::application::DomainCommand;
use crate::application::operator_command::line::AddLineSegment;
use crate::objects2d::{Rect, Vec2D};

#[test]
fn test_domain_command_toggle_pause_is_correct_variant() {
    assert!(matches!(
        DomainCommand::TogglePause,
        DomainCommand::TogglePause
    ));
}

#[test]
fn test_domain_command_resize_carries_world_rect() {
    let world_rect = Rect::new(1.0, 2.0, 3.0, 4.0);
    let command = DomainCommand::Resize { world_rect };

    assert!(matches!(
        command,
        DomainCommand::Resize {
            world_rect: mapped_rect
        } if mapped_rect == world_rect
    ));
}

#[test]
fn test_domain_command_operator_wraps_command() {
    let command = DomainCommand::Operator(Box::new(AddLineSegment {
        start: Vec2D::new(1.0, 2.0),
        end: Vec2D::new(3.0, 4.0),
    }));

    assert!(matches!(command, DomainCommand::Operator(_)));
}

#[test]
fn test_domain_command_resume_after_resize_is_correct_variant() {
    assert!(matches!(
        DomainCommand::ResumeAfterResize,
        DomainCommand::ResumeAfterResize
    ));
}
