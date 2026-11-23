#[test]
fn unavailable_reception_preserves_ringing_and_explicit_block_rules() {
    use phone_core::{Route, Rule, plugins::route_with_optional_reception};
    for (rule, available, expected) in [
        (Rule::Allow, false, Route::Ring),
        (Rule::Block, false, Route::End),
        (Rule::Screen, false, Route::Ring),
        (Rule::Allow, true, Route::Ring),
        (Rule::Block, true, Route::End),
        (Rule::Screen, true, Route::Screen),
    ] {
        assert_eq!(route_with_optional_reception(rule, available), expected);
    }
}
