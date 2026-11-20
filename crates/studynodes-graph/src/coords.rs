use serde::{Deserialize, Serialize};

/// Position in the infinite 2D world canvas (model space).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldPos {
    pub x: f32,
    pub y: f32,
}

impl WorldPos {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance_to(&self, other: &WorldPos) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Position in viewport / screen coordinates (pixels/points).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScreenPos {
    pub x: f32,
    pub y: f32,
}

impl ScreenPos {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance_to(&self, other: &ScreenPos) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// 2D Camera for the infinite canvas with pan offset and zoom factor.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    /// World position corresponding to the screen origin (0, 0)
    pub offset: WorldPos,
    /// Zoom scale factor (1.0 = 100%, 0.5 = 50%, 2.0 = 200%)
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            offset: WorldPos::ZERO,
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub const MIN_ZOOM: f32 = 0.15;
    pub const MAX_ZOOM: f32 = 4.0;

    pub fn new(offset: WorldPos, zoom: f32) -> Self {
        Self {
            offset,
            zoom: zoom.clamp(Self::MIN_ZOOM, Self::MAX_ZOOM),
        }
    }

    /// Convert a screen coordinate to world coordinate.
    pub fn screen_to_world(&self, screen: ScreenPos) -> WorldPos {
        WorldPos::new(
            self.offset.x + (screen.x / self.zoom),
            self.offset.y + (screen.y / self.zoom),
        )
    }

    /// Convert a world coordinate to screen coordinate.
    pub fn world_to_screen(&self, world: WorldPos) -> ScreenPos {
        ScreenPos::new(
            (world.x - self.offset.x) * self.zoom,
            (world.y - self.offset.y) * self.zoom,
        )
    }

    /// Pan the camera by screen delta (dx, dy).
    pub fn pan_by_screen_delta(&mut self, dx: f32, dy: f32) {
        self.offset.x -= dx / self.zoom;
        self.offset.y -= dy / self.zoom;
    }

    /// Zoom centered on a specific screen anchor point (e.g. cursor position).
    pub fn zoom_at(&mut self, anchor: ScreenPos, factor: f32) {
        let world_before = self.screen_to_world(anchor);
        let new_zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        self.zoom = new_zoom;
        let world_after = self.screen_to_world(anchor);
        self.offset.x += world_before.x - world_after.x;
        self.offset.y += world_before.y - world_after.y;
    }

    /// Center camera on a given world position for a screen of size (screen_w, screen_h).
    pub fn center_on(&mut self, target: WorldPos, screen_w: f32, screen_h: f32) {
        self.offset.x = target.x - (screen_w * 0.5 / self.zoom);
        self.offset.y = target.y - (screen_h * 0.5 / self.zoom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinate_roundtrip() {
        let cam = Camera::new(WorldPos::new(100.0, -50.0), 1.5);
        let original_world = WorldPos::new(250.0, 180.0);
        let screen = cam.world_to_screen(original_world);
        let converted_world = cam.screen_to_world(screen);

        assert!((original_world.x - converted_world.x).abs() < 1e-4);
        assert!((original_world.y - converted_world.y).abs() < 1e-4);
    }

    #[test]
    fn test_zoom_anchor_preservation() {
        let mut cam = Camera::new(WorldPos::new(50.0, 50.0), 1.0);
        let cursor = ScreenPos::new(400.0, 300.0);
        let world_at_cursor_before = cam.screen_to_world(cursor);

        cam.zoom_at(cursor, 1.5);
        let world_at_cursor_after = cam.screen_to_world(cursor);

        assert!((world_at_cursor_before.x - world_at_cursor_after.x).abs() < 1e-4);
        assert!((world_at_cursor_before.y - world_at_cursor_after.y).abs() < 1e-4);
    }
}
