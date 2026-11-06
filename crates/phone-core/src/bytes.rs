pub fn bounded_add(current: usize, incoming: usize, limit: usize) -> Option<usize> {
    current
        .checked_add(incoming)
        .filter(|total| *total <= limit)
}
pub fn nonempty_within(length: usize, limit: usize) -> bool {
    length != 0 && length <= limit
}
