//! Which parts of the scene are built and drawn.
//!
//! One set travels the whole way: the renderer derives it from its config
//! (F2 switches the heavy layers off), the physics thread builds snapshots
//! with only the requested layers, every snapshot records the layers it
//! holds, and each render pipeline belongs to one layer. A frame draws the
//! layers that are both requested and present.

/// A set of scene layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneLayers(u8);

impl SceneLayers {
    /// Operators, tool previews, labels, sidebar and text. Always drawn.
    pub const BASE: Self = Self(1 << 0);
    /// Balls: per-ball render data (colours, tails, neighbour lines) and
    /// their sprites.
    pub const BALLS: Self = Self(1 << 1);
    /// Thermo field grids and their heatmap and value overlays.
    pub const THERMO: Self = Self(1 << 2);

    /// The full scene.
    pub const ALL: Self = Self(Self::BASE.0 | Self::BALLS.0 | Self::THERMO.0);
    /// What F2 (rendering off) switches off: work that only exists for
    /// drawing the full scene.
    pub const HEAVY: Self = Self(Self::BALLS.0 | Self::THERMO.0);

    /// Whether every layer of `other` is in `self`.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Layers in both sets.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Layers of `self` that are not in `other`.
    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl Default for SceneLayers {
    fn default() -> Self {
        Self::ALL
    }
}

#[cfg(test)]
mod tests {
    use super::SceneLayers;

    #[test]
    fn light_set_keeps_only_the_base_layer() {
        let light = SceneLayers::ALL.difference(SceneLayers::HEAVY);
        assert_eq!(light, SceneLayers::BASE);
        assert!(light.contains(SceneLayers::BASE));
        assert!(!light.contains(SceneLayers::BALLS));
        assert!(!light.contains(SceneLayers::THERMO));
    }

    #[test]
    fn intersection_is_what_both_sets_hold() {
        let requested = SceneLayers::ALL;
        let present = SceneLayers::BASE;
        assert_eq!(requested.intersection(present), SceneLayers::BASE);
        assert!(SceneLayers::ALL.contains(SceneLayers::HEAVY));
    }
}
