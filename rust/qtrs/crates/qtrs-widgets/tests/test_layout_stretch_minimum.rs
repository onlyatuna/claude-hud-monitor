//! Equal-stretch columns where some need more than their share (`qGeomCalc`): the others must
//! give the space back, otherwise the last column is pushed out of the layout.
use qtrs_widgets::layout::distribute_1d_space;
use qtrs_widgets::size_policy::Policy;

const NO_MAX: i32 = i32::MAX;

#[test]
fn items_above_their_share_are_pinned_and_the_rest_shrinks_to_fit() {
    // Three equal-stretch columns in 630 px; the first two need 218 and 227 (their shares are 210).
    let items = [
        (0, 218, NO_MAX, Policy::Preferred, 1),
        (0, 227, NO_MAX, Policy::Preferred, 1),
        (0, 175, NO_MAX, Policy::Preferred, 1),
    ];
    let sizes = distribute_1d_space(&items, 630);
    assert_eq!(sizes[0], 218);
    assert_eq!(sizes[1], 227);
    assert_eq!(sizes.iter().sum::<i32>(), 630, "columns must exactly fill the span: {sizes:?}");
    assert!(sizes[2] >= 175);
}

#[test]
fn pinning_one_item_can_pin_the_next() {
    // After the first is pinned at 300 the others get 150 each, which is below the second's 160.
    let items = [
        (0, 300, NO_MAX, Policy::Preferred, 1),
        (0, 160, NO_MAX, Policy::Preferred, 1),
        (0, 0, NO_MAX, Policy::Preferred, 1),
    ];
    let sizes = distribute_1d_space(&items, 600);
    assert_eq!(sizes, vec![300, 160, 140]);
}

#[test]
fn equal_stretch_without_minimum_pressure_stays_equal() {
    let items = [(0, 0, NO_MAX, Policy::Preferred, 1); 3];
    assert_eq!(distribute_1d_space(&items, 600), vec![200, 200, 200]);
}

#[test]
fn when_minimums_exceed_the_span_each_item_keeps_its_minimum() {
    let items = [
        (0, 300, NO_MAX, Policy::Preferred, 1),
        (0, 300, NO_MAX, Policy::Preferred, 1),
    ];
    assert_eq!(distribute_1d_space(&items, 400), vec![300, 300]);
}
