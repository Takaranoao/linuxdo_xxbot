use anyhow::Result;
#[cfg(test)]
use chrono::TimeZone;
use chrono::{DateTime, Utc};

pub struct Schedule {
    cron: saffron::Cron,
}

impl Schedule {
    pub fn parse(expr: &str) -> Result<Self> {
        let cron: saffron::Cron = expr
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid cron expression {expr:?}: {e:?}"))?;
        Ok(Self { cron })
    }

    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.cron.next_after(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn every_five_minutes_advances_to_next_slot() {
        let s = Schedule::parse("*/5 * * * *").unwrap();
        let next = s.next_after(at(2026, 4, 26, 10, 0)).unwrap();
        assert_eq!(next, at(2026, 4, 26, 10, 5));
    }

    #[test]
    fn every_five_minutes_skips_when_already_on_slot() {
        let s = Schedule::parse("*/5 * * * *").unwrap();
        let now = at(2026, 4, 26, 10, 5);
        let next = s.next_after(now).unwrap();
        assert!(next > now);
    }

    #[test]
    fn daily_at_specific_minute() {
        let s = Schedule::parse("30 9 * * *").unwrap();
        let next = s.next_after(at(2026, 4, 26, 10, 0)).unwrap();
        assert_eq!(next, at(2026, 4, 27, 9, 30));
    }

    #[test]
    fn invalid_expression_errors() {
        assert!(Schedule::parse("not a cron").is_err());
    }
}
