/// A rectangle for packing operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// Check if this rectangle intersects with another
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    /// Check if this rectangle fully contains another
    pub fn contains(&self, other: &Rect) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.x + self.width >= other.x + other.width
            && self.y + self.height >= other.y + other.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intersects() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        let c = Rect::new(20, 20, 10, 10);

        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn test_contains() {
        let outer = Rect::new(0, 0, 20, 20);
        let inner = Rect::new(5, 5, 5, 5);
        let partial = Rect::new(15, 15, 10, 10);

        assert!(outer.contains(&inner));
        assert!(!inner.contains(&outer));
        assert!(!outer.contains(&partial));
    }
}
