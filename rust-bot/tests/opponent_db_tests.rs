use mushroom_bot::opponent_db::{KnownFingerprint, OpponentDB, OpponentFingerprint};
use mushroom_bot::types::{FeatureVector8, KnownStyle};

#[test]
fn opponent_db_rejects_tiny_payloads() {
    let mut db = OpponentDB::new();
    assert!(!db.load(&[1, 2, 3, 4]));
    assert!(!db.is_loaded());
}

#[test]
fn fingerprint_match_returns_expected_style() {
    let fp = KnownFingerprint {
        id: 1,
        style: KnownStyle::CordycepsAttack as u32,
        side_mask: 2,
        min_moves: 1,
        confidence_threshold: 50,
        margin_to_second: 0,
        mean: FeatureVector8 { dim: [128, 64, 0, 0, 0, 0, 0, 0] },
        var: FeatureVector8 { dim: [128; 8] },
        prior_config_id: 0,
    };

    let observed = OpponentFingerprint {
        move_count: 1,
        total_area: 1,
        medium_count: 0,
        large_count: 0,
        tall_count: 0,
        wide_count: 0,
        steal_seen: 0,
        pass_seen: 0,
        barrier_freq: 0,
        we_are_first: false,
        ..Default::default()
    };

    let mut confidence = 0.0;
    let mut margin = 0.0;
    let style = observed.match_fingerprint(&[fp], &mut confidence, &mut margin);

    assert_eq!(style, KnownStyle::CordycepsAttack);
    assert!(confidence >= 0.5);
    assert!(margin >= 0.0);
}
