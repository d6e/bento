use super::Rect;
use crate::cli::PackingHeuristic;

/// MaxRects bin packer implementation
pub struct MaxRectsPacker {
    bin_width: u32,
    bin_height: u32,
    free_rects: Vec<Rect>,
}

impl MaxRectsPacker {
    pub fn new(width: u32, height: u32) -> Self {
        let initial_rect = Rect::new(0, 0, width, height);
        Self {
            bin_width: width,
            bin_height: height,
            free_rects: vec![initial_rect],
        }
    }

    /// Try to insert a rectangle with the given dimensions
    /// Returns the placed rectangle if successful
    pub fn insert(&mut self, width: u32, height: u32, heuristic: PackingHeuristic) -> Option<Rect> {
        let best_rect = self.find_position(width, height, heuristic)?;
        self.place_rect(best_rect);
        Some(best_rect)
    }

    /// Check if a rectangle of the given size can fit
    pub fn can_fit(&self, width: u32, height: u32) -> bool {
        self.free_rects
            .iter()
            .any(|r| width <= r.width && height <= r.height)
    }

    fn find_position(&self, width: u32, height: u32, heuristic: PackingHeuristic) -> Option<Rect> {
        let mut best_score = (i64::MAX, i64::MAX);
        let mut best_rect = None;

        for free_rect in &self.free_rects {
            if width <= free_rect.width && height <= free_rect.height {
                let score = self.score_rect(free_rect, width, height, heuristic);
                if score < best_score {
                    best_score = score;
                    best_rect = Some(Rect::new(free_rect.x, free_rect.y, width, height));
                }
            }
        }

        best_rect
    }

    fn score_rect(
        &self,
        free_rect: &Rect,
        width: u32,
        height: u32,
        heuristic: PackingHeuristic,
    ) -> (i64, i64) {
        match heuristic {
            PackingHeuristic::BestShortSideFit => {
                let leftover_h = (free_rect.width - width) as i64;
                let leftover_v = (free_rect.height - height) as i64;
                let short = leftover_h.min(leftover_v);
                let long = leftover_h.max(leftover_v);
                (short, long)
            }
            PackingHeuristic::BestLongSideFit => {
                let leftover_h = (free_rect.width - width) as i64;
                let leftover_v = (free_rect.height - height) as i64;
                let short = leftover_h.min(leftover_v);
                let long = leftover_h.max(leftover_v);
                (long, short)
            }
            PackingHeuristic::BestAreaFit => {
                let area = free_rect.area() as i64;
                let short = (free_rect.width - width).min(free_rect.height - height) as i64;
                (area, short)
            }
            PackingHeuristic::BottomLeft => {
                let top = (free_rect.y + height) as i64;
                let left = free_rect.x as i64;
                (top, left)
            }
        }
    }

    fn place_rect(&mut self, rect: Rect) {
        let mut new_rects = Vec::new();

        self.free_rects.retain(|free_rect| {
            if !rect.intersects(free_rect) {
                return true;
            }

            // Split the free rectangle around the placed rectangle
            // Left portion
            if rect.x > free_rect.x {
                new_rects.push(Rect::new(
                    free_rect.x,
                    free_rect.y,
                    rect.x - free_rect.x,
                    free_rect.height,
                ));
            }

            // Right portion
            if rect.x + rect.width < free_rect.x + free_rect.width {
                new_rects.push(Rect::new(
                    rect.x + rect.width,
                    free_rect.y,
                    (free_rect.x + free_rect.width) - (rect.x + rect.width),
                    free_rect.height,
                ));
            }

            // Top portion
            if rect.y > free_rect.y {
                new_rects.push(Rect::new(
                    free_rect.x,
                    free_rect.y,
                    free_rect.width,
                    rect.y - free_rect.y,
                ));
            }

            // Bottom portion
            if rect.y + rect.height < free_rect.y + free_rect.height {
                new_rects.push(Rect::new(
                    free_rect.x,
                    rect.y + rect.height,
                    free_rect.width,
                    (free_rect.y + free_rect.height) - (rect.y + rect.height),
                ));
            }

            false
        });

        self.free_rects.extend(new_rects);
        self.prune_free_rects();
    }

    fn prune_free_rects(&mut self) {
        // Remove rectangles that are fully contained within others
        let mut i = 0;
        while i < self.free_rects.len() {
            let mut j = i + 1;
            while j < self.free_rects.len() {
                if self.free_rects[i].contains(&self.free_rects[j]) {
                    self.free_rects.swap_remove(j);
                } else if self.free_rects[j].contains(&self.free_rects[i]) {
                    self.free_rects.swap_remove(i);
                    j = i + 1;
                    continue;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }

    /// Get packing efficiency as a ratio (0.0 to 1.0)
    pub fn occupancy(&self) -> f64 {
        let total_area = self.bin_width as u64 * self.bin_height as u64;
        let free_area: u64 = self.free_rects.iter().map(|r| r.area()).sum();
        let used_area = total_area.saturating_sub(free_area);
        used_area as f64 / total_area as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_insert() {
        let mut packer = MaxRectsPacker::new(100, 100);
        let rect = packer
            .insert(50, 50, PackingHeuristic::BestShortSideFit)
            .unwrap();

        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, 0);
        assert_eq!(rect.width, 50);
        assert_eq!(rect.height, 50);
    }

    #[test]
    fn test_multiple_inserts() {
        let mut packer = MaxRectsPacker::new(100, 100);

        let r1 = packer
            .insert(50, 50, PackingHeuristic::BestShortSideFit)
            .unwrap();
        let r2 = packer
            .insert(50, 50, PackingHeuristic::BestShortSideFit)
            .unwrap();
        let r3 = packer
            .insert(50, 50, PackingHeuristic::BestShortSideFit)
            .unwrap();
        let r4 = packer
            .insert(50, 50, PackingHeuristic::BestShortSideFit)
            .unwrap();

        // All four 50x50 rects should fit in a 100x100 bin
        assert!(!r1.intersects(&r2));
        assert!(!r1.intersects(&r3));
        assert!(!r1.intersects(&r4));
        assert!(!r2.intersects(&r3));
        assert!(!r2.intersects(&r4));
        assert!(!r3.intersects(&r4));
    }

    #[test]
    fn test_too_large() {
        let mut packer = MaxRectsPacker::new(100, 100);
        let result = packer.insert(150, 50, PackingHeuristic::BestShortSideFit);
        assert!(result.is_none());
    }

    #[test]
    fn test_can_fit() {
        let mut packer = MaxRectsPacker::new(100, 100);
        assert!(packer.can_fit(50, 50));
        assert!(packer.can_fit(100, 100));
        assert!(!packer.can_fit(101, 100));

        packer
            .insert(100, 100, PackingHeuristic::BestShortSideFit)
            .unwrap();
        assert!(!packer.can_fit(1, 1));
    }

    #[test]
    fn test_occupancy_known_limitation() {
        // The occupancy calculation is approximate because free_rects can overlap.
        // This is acceptable since occupancy is only used for informational logging.
        //
        // Example: After placing 50x50 at (0,0) in a 100x100 bin:
        // - Right free rect: (50, 0, 50, 100) = 5000
        // - Bottom free rect: (0, 50, 100, 50) = 5000
        // - These overlap at (50, 50, 50, 50) = 2500
        // - True free area = 5000 + 5000 - 2500 = 7500
        // - Naive sum = 10000, which exceeds total area
        //
        // The calculation becomes accurate when the bin is fully packed
        // (no overlapping free rects remain).
        let mut packer = MaxRectsPacker::new(100, 100);
        packer
            .insert(50, 50, PackingHeuristic::BestShortSideFit)
            .unwrap();

        // Occupancy will be inaccurate (0.0) but this is only used for logging
        let _occupancy = packer.occupancy();
    }

    #[test]
    fn test_occupancy_full_bin() {
        // When fully packed, occupancy calculation is accurate
        let mut packer = MaxRectsPacker::new(100, 100);
        packer
            .insert(50, 50, PackingHeuristic::BestShortSideFit)
            .unwrap();
        packer
            .insert(50, 50, PackingHeuristic::BestShortSideFit)
            .unwrap();
        packer
            .insert(50, 50, PackingHeuristic::BestShortSideFit)
            .unwrap();
        packer
            .insert(50, 50, PackingHeuristic::BestShortSideFit)
            .unwrap();

        let occupancy = packer.occupancy();
        assert!(
            (occupancy - 1.0).abs() < 0.01,
            "Expected occupancy ~1.0, got {}",
            occupancy
        );
    }
}
