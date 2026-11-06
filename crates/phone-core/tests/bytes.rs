#[test]
fn input_lengths_cover_zero_the_limit_and_values_larger_than_the_backing_proof_array() {
    for length in (0..=1024).chain([usize::MAX]) {
        for limit in [0, 1, 32, 256, usize::MAX] {
            assert_eq!(
                phone_core::bytes::nonempty_within(length, limit),
                length != 0 && length <= limit
            );
        }
    }
}

#[test]
fn byte_budgets_refuse_overflow_and_accept_exactly_the_declared_limit() {
    for current in (0..=16).chain([usize::MAX - 1, usize::MAX]) {
        for incoming in (0..=16).chain([usize::MAX - 1, usize::MAX]) {
            for limit in (0..=16).chain([usize::MAX - 1, usize::MAX]) {
                let wide = current as u128 + incoming as u128;
                let expected = if wide <= limit as u128 {
                    Some(wide as usize)
                } else {
                    None
                };
                assert_eq!(
                    phone_core::bytes::bounded_add(current, incoming, limit),
                    expected,
                    "current={current}, incoming={incoming}, limit={limit}"
                );
            }
        }
    }
}
