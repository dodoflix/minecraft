//! `net.minecraft.core.Cursor3D`: x-fastest, then y, then z walk over an inclusive box.

#[derive(Clone, Debug)]
pub struct Cursor3D {
    origin_x: i32,
    origin_y: i32,
    origin_z: i32,
    width: i32,
    height: i32,
    depth: i32,
    end: i32,
    index: i32,
    x: i32,
    y: i32,
    z: i32,
}

impl Cursor3D {
    pub const TYPE_INSIDE: i32 = 0;
    pub const TYPE_FACE: i32 = 1;
    pub const TYPE_EDGE: i32 = 2;
    pub const TYPE_CORNER: i32 = 3;

    pub const fn new(
        min_x: i32,
        min_y: i32,
        min_z: i32,
        max_x: i32,
        max_y: i32,
        max_z: i32,
    ) -> Cursor3D {
        let width = max_x.wrapping_sub(min_x).wrapping_add(1);
        let height = max_y.wrapping_sub(min_y).wrapping_add(1);
        let depth = max_z.wrapping_sub(min_z).wrapping_add(1);
        Cursor3D {
            origin_x: min_x,
            origin_y: min_y,
            origin_z: min_z,
            width,
            height,
            depth,
            end: width.wrapping_mul(height).wrapping_mul(depth),
            index: 0,
            x: 0,
            y: 0,
            z: 0,
        }
    }

    pub const fn advance(&mut self) -> bool {
        if self.index == self.end {
            false
        } else {
            // end != 0 here, so width and height are non-zero
            self.x = self.index.wrapping_rem(self.width);
            let slice = self.index.wrapping_div(self.width);
            self.y = slice.wrapping_rem(self.height);
            self.z = slice.wrapping_div(self.height);
            self.index = self.index.wrapping_add(1);
            true
        }
    }

    pub const fn next_x(&self) -> i32 {
        self.origin_x.wrapping_add(self.x)
    }

    pub const fn next_y(&self) -> i32 {
        self.origin_y.wrapping_add(self.y)
    }

    pub const fn next_z(&self) -> i32 {
        self.origin_z.wrapping_add(self.z)
    }

    /// 0 = inside, 1 = face, 2 = edge, 3 = corner of the box.
    pub const fn get_next_type(&self) -> i32 {
        let mut ty = 0;
        if self.x == 0 || self.x == self.width.wrapping_sub(1) {
            ty += 1;
        }
        if self.y == 0 || self.y == self.height.wrapping_sub(1) {
            ty += 1;
        }
        if self.z == 0 || self.z == self.depth.wrapping_sub(1) {
            ty += 1;
        }
        ty
    }
}
