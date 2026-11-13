// Rules shared between `logic {}` scripts (through `extern fn`) and any
// Rust code of the project — one implementation, typed at the boundary.

/// A rough estimate in minutes from a task's wording.
pub fn guess_estimate(text: &str) -> i64 {
    let words = text.split_whitespace().count() as i64;
    (words * 10).clamp(15, 120)
}

/// How many events an HTTP DELETE removed, from its status.
pub fn if_removed(status: i64) -> i64 {
    if (200..300).contains(&status) { 1 } else { 0 }
}

/// Whether an instant falls on a weekend in the app's time zone.
pub fn is_weekend(at: chrono::DateTime<chrono::Utc>) -> bool {
    use chrono::Datelike;
    matches!(crate::tz::local(at).weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun)
}
