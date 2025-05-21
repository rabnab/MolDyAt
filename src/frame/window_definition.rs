use crate::objects2d::Vec2D;
use ggez::graphics::{Text, TextFragment};

const LEN_TO_PX: f32 = 1. / 10.; // 50 px are one LEN_SCALE (SIG is about 0.5 to 1 LEN_SCALE)

pub struct WindowDefinition {
    pub curs_pos: Vec2D,
    pub curs_pos_of_txt: Vec2D,
    pub region_px: (f32, f32),
    pub margin_px: (f32, f32),
    pub ball_status_text_pos: Option<[f32; 2]>,
    pub txt_cursor: Text,
    pub last_status_text: String,
    pub status_text_mesh: Text,
}

impl WindowDefinition {
    pub(crate) fn update_cursor_and_text(&mut self, x: f32, y: f32) {
        self.curs_pos = Vec2D::new(x, y);
        let curs_pos_coord = self.px_to_coord(x, y);

        let curs_moved_sq = self.curs_pos.dist(&self.curs_pos_of_txt).norm_sq();
        if curs_moved_sq > (f32::min(self.region_px.0, self.region_px.1) * 0.1) {
            self.curs_pos_of_txt = self.curs_pos.clone();
            self.txt_cursor = Text::new(
                TextFragment::new(format!(
                    "x: {}, y: {} pos ({:.3},{:.3})",
                    x, y, curs_pos_coord.x, curs_pos_coord.y,
                ))
                .scale(20.),
            );
        }
    }

    pub fn px_to_coord(&self, px_x: f32, px_y: f32) -> Vec2D {
        //window top left (0,0) px
        //margin margin_px_[xy]
        //region region_px_[xy]
        //window bottom [xy] = region_px_[xy]+2*margin_px[xy]
        //pos = px_to_len(px_pos - (region_px_[xy]+2*margin_px_[xy])*0.5)
        Vec2D::new(
            Self::px_to_len(px_x - self.region_px.0 * 0.5 - self.margin_px.0),
            -Self::px_to_len(px_y - self.region_px.1 * 0.5 - self.margin_px.1),
        )
    }

    pub fn len_to_px(coord_len: f32) -> f32 {
        coord_len / LEN_TO_PX
    }

    pub fn px_to_len(coord_px: f32) -> f32 {
        coord_px * LEN_TO_PX
    }

    #[cfg(test)]
    pub fn get_len_to_px() -> f32 {
        LEN_TO_PX
    }
}

impl Default for WindowDefinition {
    fn default() -> WindowDefinition {
        WindowDefinition {
            curs_pos: Vec2D::new(0., 0.),
            curs_pos_of_txt: Vec2D::new(0., 0.),
            region_px: (100., 100.),
            margin_px: (10., 10.),
            ball_status_text_pos: None,
            txt_cursor: Text::new(
                TextFragment::new(format!("x: {}, y: {} pos ({:.3},{:.3})", 0., 0., 0., 0.,))
                    .scale(20.0),
            ),
            status_text_mesh: Text::new(TextFragment::new("")),
            last_status_text: "".to_string(),
        }
    }
}
