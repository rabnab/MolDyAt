use crate::application::MainContent;
use crate::graphics::renderer::Margins;

#[test]
fn test_bootstrap_world_rect_matches_current_wgpu_startup_geometry() {
    let world_rect =
        MainContent::bootstrap_world_rect(1280, 720, Margins::new(110.0, 50.0, 50.0, 50.0));

    assert_eq!(world_rect.x, -56.0);
    assert_eq!(world_rect.y, 31.0);
    assert_eq!(world_rect.w, 112.0);
    assert_eq!(world_rect.h, -62.0);
}
