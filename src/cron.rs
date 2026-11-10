//! Five-field cron expressions (`minute hour day-of-month month
//! day-of-week`), parsed at compile time into bit masks the generated
//! scheduler matches against — the generated code never parses cron text.
//!
//! Each field accepts `*`, `n`, `a-b`, `*/s`, `a-b/s`, `n/s` and comma
//! lists of those. Months and weekdays also accept three-letter English
//! names (`jan`, `mon`, case-insensitive); weekday `7` is Sunday like `0`.
//! When both day-of-month and day-of-week are restricted, a day matching
//! either one fires (classic cron semantics).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronMask {
    /// Bit `m` set: minute `m` (0-59) matches.
    pub minutes: u64,
    /// Bits 0-23.
    pub hours: u32,
    /// Bits 1-31.
    pub days: u32,
    /// Bits 1-12.
    pub months: u16,
    /// Bits 0-6, 0 = Sunday.
    pub weekdays: u8,
    /// Day-of-month was `*` (so only weekdays restrict days).
    pub any_day: bool,
    /// Day-of-week was `*`.
    pub any_weekday: bool,
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

fn value(text: &str, names: &[&str], offset: u32, field: &str) -> Result<u32, String> {
    if let Some(i) = names.iter().position(|n| n.eq_ignore_ascii_case(text)) {
        return Ok(i as u32 + offset);
    }
    text.parse::<u32>()
        .map_err(|_| format!("'{}' is not a valid {} value", text, field))
}

/// Parses one field into the set of matching values in `min..=max`.
fn field(text: &str, min: u32, max: u32, names: &[&str], name_offset: u32, field_name: &str) -> Result<Vec<u32>, String> {
    let mut values = Vec::new();
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => {
                let step: u32 = step
                    .parse()
                    .map_err(|_| format!("'{}' is not a valid step in the {} field", step, field_name))?;
                if step == 0 {
                    return Err(format!("step 0 in the {} field", field_name));
                }
                (range, step)
            }
            None => (part, 1),
        };
        let (start, end) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            (value(a, names, name_offset, field_name)?, value(b, names, name_offset, field_name)?)
        } else {
            let start = value(range, names, name_offset, field_name)?;
            // `n/s` means "from n to the end, every s".
            (start, if part.contains('/') { max } else { start })
        };
        if start < min || end > max || start > end {
            return Err(format!(
                "'{}' is out of range for the {} field ({}-{})",
                part, field_name, min, max
            ));
        }
        values.extend((start..=end).step_by(step as usize));
    }
    Ok(values)
}

pub fn parse(expression: &str) -> Result<CronMask, String> {
    let parts: Vec<&str> = expression.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(format!(
            "cron \"{}\" must have 5 fields (minute hour day-of-month month day-of-week), found {}",
            expression,
            parts.len()
        ));
    }
    let context = |e: String| format!("cron \"{}\": {}", expression, e);
    let minutes = field(parts[0], 0, 59, &[], 0, "minute").map_err(context)?;
    let hours = field(parts[1], 0, 23, &[], 0, "hour").map_err(context)?;
    let days = field(parts[2], 1, 31, &[], 0, "day-of-month").map_err(context)?;
    let months = field(parts[3], 1, 12, &MONTHS, 1, "month").map_err(context)?;
    let weekdays = field(parts[4], 0, 7, &WEEKDAYS, 0, "day-of-week").map_err(context)?;

    Ok(CronMask {
        minutes: minutes.iter().fold(0, |m, v| m | 1u64 << v),
        hours: hours.iter().fold(0, |m, v| m | 1u32 << v),
        days: days.iter().fold(0, |m, v| m | 1u32 << v),
        months: months.iter().fold(0, |m, v| m | 1u16 << v),
        weekdays: weekdays.iter().fold(0, |m, v| m | 1u8 << (v % 7)),
        any_day: parts[2] == "*",
        any_weekday: parts[4] == "*",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fifteen_minutes() {
        let m = parse("*/15 * * * *").unwrap();
        assert_eq!(m.minutes, 1 | 1 << 15 | 1 << 30 | 1 << 45);
        assert_eq!(m.hours, (1 << 24) - 1);
        assert!(m.any_day && m.any_weekday);
    }

    #[test]
    fn names_lists_ranges_and_sunday_seven() {
        let m = parse("30 7 1,15 jan-mar mon-fri,7").unwrap();
        assert_eq!(m.minutes, 1 << 30);
        assert_eq!(m.hours, 1 << 7);
        assert_eq!(m.days, 1 << 1 | 1 << 15);
        assert_eq!(m.months, 1 << 1 | 1 << 2 | 1 << 3);
        assert_eq!(m.weekdays, 0b0111111);
        assert!(!m.any_day && !m.any_weekday);
    }

    #[test]
    fn invalid_expressions_are_explained() {
        assert!(parse("* * * *").unwrap_err().contains("must have 5 fields"));
        assert!(parse("60 * * * *").unwrap_err().contains("out of range for the minute field"));
        assert!(parse("* * * * funday").unwrap_err().contains("not a valid day-of-week"));
        assert!(parse("*/0 * * * *").unwrap_err().contains("step 0"));
    }
}
