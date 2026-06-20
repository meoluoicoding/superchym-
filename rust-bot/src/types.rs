// types.rs — from types.hpp

pub const ROWS: usize = 10;
pub const COLS: usize = 17;

pub const FIRST_PLAYER: i8 = 1;
pub const SECOND_PLAYER: i8 = -1;
pub const NO_OWNER: i8 = 0;

pub type ValueGrid = [[i8; COLS]; ROWS];
pub type OwnerGrid = [[i8; COLS]; ROWS];
pub type MQualityGrid = [[i8; COLS]; ROWS];

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct GeometryConfig {
    pub corner_weight: i16,
    pub edge_weight: i16,
    pub center_weight: i16,
    pub connectivity_weight: i16,
    pub steal_weight: i16,
    pub barrier_weight: i16,
    pub compact_bonus: i16,
    pub risk_penalty: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ShapeClass {
    Rect1x2 = 0,
    Rect2x1 = 1,
    Rect1x3 = 2,
    Rect3x1 = 3,
    Rect2x2 = 4,
    Rect1x4 = 5,
    Rect4x1 = 6,
    RectOther = 7,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Orientation {
    Square = 0,
    Portrait = 1,
    Landscape = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RegionTag {
    Corner = 0,
    Edge = 1,
    CenterOuter = 2,
    CenterInner = 3,
    None = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum KnownStyle {
    Unknown = 0,
    CordycepsAttack = 1,
    CordycepsDefense = 2,
    CordycepsBalanced = 3,
    RustOld = 4,
    RustUpdate = 5,
}

#[derive(Clone, Debug, Default)]
pub struct MovePriorConfig {
    pub shape_boost: [i16; 8],
    pub medium_rect_boost: i16,
    pub barrier_boost: i16,
    pub connection_boost: i16,
    pub dead_cell_risk_penalty: i16,
    pub side_boost_first: i16,
    pub side_boost_second: i16,
    pub max_total_adjustment: u16,
    pub confidence_min: u8,
}

impl MovePriorConfig {
    pub fn new_default() -> Self {
        MovePriorConfig {
            shape_boost: [0; 8],
            medium_rect_boost: 0,
            barrier_boost: 0,
            connection_boost: 0,
            dead_cell_risk_penalty: 0,
            side_boost_first: 0,
            side_boost_second: 0,
            max_total_adjustment: 3000,
            confidence_min: 60,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(align(16))]
pub struct FeatureVector8 {
    pub dim: [i16; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Move {
    pub r1: i32,
    pub c1: i32,
    pub r2: i32,
    pub c2: i32,
    pub priority: i32,
}

impl Move {
    pub const fn pass() -> Self {
        Move { r1: -1, c1: -1, r2: -1, c2: -1, priority: 0 }
    }

    pub fn is_pass(&self) -> bool {
        self.r1 == -1 && self.c1 == -1 && self.r2 == -1 && self.c2 == -1
    }

    pub fn coords_eq(&self, other: &Move) -> bool {
        self.r1 == other.r1 && self.c1 == other.c1
            && self.r2 == other.r2 && self.c2 == other.c2
    }
}

pub const PASS_MOVE: Move = Move::pass();

#[inline]
pub const fn opponent(player: i8) -> i8 {
    -player
}

#[inline]
pub fn classify_shape(r1: i32, c1: i32, r2: i32, c2: i32) -> ShapeClass {
    let h = r2 - r1 + 1;
    let w = c2 - c1 + 1;
    match (h, w) {
        (1, 2) => ShapeClass::Rect1x2,
        (2, 1) => ShapeClass::Rect2x1,
        (1, 3) => ShapeClass::Rect1x3,
        (3, 1) => ShapeClass::Rect3x1,
        (2, 2) => ShapeClass::Rect2x2,
        (1, 4) => ShapeClass::Rect1x4,
        (4, 1) => ShapeClass::Rect4x1,
        _ => ShapeClass::RectOther,
    }
}

#[inline]
pub fn classify_orientation(r1: i32, c1: i32, r2: i32, c2: i32) -> Orientation {
    let h = r2 - r1 + 1;
    let w = c2 - c1 + 1;
    if h == w {
        Orientation::Square
    } else if h > w {
        Orientation::Portrait
    } else {
        Orientation::Landscape
    }
}

#[inline]
pub fn classify_region(r1: i32, c1: i32, r2: i32, c2: i32) -> RegionTag {
    let near_corner = (r1 <= 1 && c1 <= 1)
        || (r1 <= 1 && c2 >= COLS as i32 - 2)
        || (r2 >= ROWS as i32 - 2 && c1 <= 1)
        || (r2 >= ROWS as i32 - 2 && c2 >= COLS as i32 - 2);
    if near_corner {
        return RegionTag::Corner;
    }
    let near_edge = r1 == 0 || r2 == ROWS as i32 - 1 || c1 == 0 || c2 == COLS as i32 - 1;
    if near_edge {
        return RegionTag::Edge;
    }
    let cr = (r1 + r2) / 2;
    let cc = (c1 + c2) / 2;
    if cr >= 3 && cr <= 6 && cc >= 5 && cc <= 11 {
        RegionTag::CenterInner
    } else {
        RegionTag::CenterOuter
    }
}

#[inline]
pub fn move_area(r1: i32, c1: i32, r2: i32, c2: i32) -> i32 {
    (r2 - r1 + 1) * (c2 - c1 + 1)
}
