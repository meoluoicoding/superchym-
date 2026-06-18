use mushroom_bot::types::{
    classify_orientation, classify_region, classify_shape, move_area, opponent, Move, Orientation,
    RegionTag, ShapeClass, FIRST_PLAYER, PASS_MOVE, SECOND_PLAYER,
};

#[test]
fn pass_move_and_opponent_helpers_work() {
    assert!(PASS_MOVE.is_pass());
    assert_eq!(opponent(FIRST_PLAYER), SECOND_PLAYER);
    assert_eq!(opponent(SECOND_PLAYER), FIRST_PLAYER);

    let mv = Move { r1: 0, c1: 1, r2: 0, c2: 2, priority: 7 };
    assert!(!mv.is_pass());
    assert!(mv.coords_eq(&Move { priority: 0, ..mv }));
}

#[test]
fn shape_orientation_region_helpers_are_stable() {
    assert_eq!(classify_shape(0, 0, 0, 1), ShapeClass::Rect1x2);
    assert_eq!(classify_shape(0, 0, 2, 0), ShapeClass::Rect3x1);
    assert_eq!(classify_orientation(0, 0, 2, 0), Orientation::Portrait);
    assert_eq!(classify_orientation(0, 0, 0, 2), Orientation::Landscape);
    assert_eq!(classify_region(0, 0, 0, 1), RegionTag::Corner);
    assert_eq!(classify_region(0, 5, 0, 6), RegionTag::Edge);
    assert_eq!(classify_region(4, 6, 5, 7), RegionTag::CenterInner);
    assert_eq!(move_area(2, 3, 4, 6), 12);
}
