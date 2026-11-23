use crate::{Route, Rule};

pub fn route_with_optional_reception(rule: Rule, available: bool) -> Route {
    match (rule, available) {
        (Rule::Screen, false) => Route::Ring,
        _ => crate::initial_route(rule),
    }
}
