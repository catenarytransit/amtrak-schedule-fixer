//! Rolling service-date backfill for the daily Coast Starlight.
use jiff::{Span, civil::Date};

pub(crate) const HISTORY_DAYS: i64 = 3;

/// Add missing dates strictly before the first published timetable variant.
/// The caller supplies private service IDs and their copied GTFS exceptions.
/// Existing exceptions on any variant take precedence; tied earliest
/// variants are ambiguous and must not be activated speculatively.
pub(crate) fn historical_additions(
    today: Date,
    variants: &[(String, Date)],
    exceptions: &[(String, String, String)],
) -> Vec<(String, String, String)> {
    let Some(earliest) = variants.iter().map(|(_, start)| *start).min() else {
        return Vec::new();
    };
    let mut candidates = variants.iter().filter(|(_, start)| *start == earliest);
    let Some((service_id, _)) = candidates.next() else {
        return Vec::new();
    };
    if candidates.next().is_some() {
        return Vec::new();
    }

    (0..=HISTORY_DAYS)
        .rev()
        .filter_map(|offset| {
            let day = today - Span::new().days(offset);
            if day >= earliest {
                return None;
            }
            let ymd = day.strftime("%Y%m%d").to_string();
            if exceptions.iter().any(|(id, date, _)| {
                date == &ymd && variants.iter().any(|(variant, _)| variant == id)
            }) {
                return None;
            }
            Some((service_id.clone(), ymd, "1".to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn daily_history_crosses_month_boundary_without_changing_future_variants() {
        let today = date(2026, 12, 1);
        let variants = vec![
            ("future".to_string(), date(2026, 12, 5)),
            ("first".to_string(), today),
        ];
        let additions = historical_additions(today, &variants, &[]);
        assert_eq!(
            additions,
            ["20261128", "20261129", "20261130"]
                .map(|day| ("first".to_string(), day.to_string(), "1".to_string()))
                .to_vec()
        );
        // Running on a feed with the added exceptions must not duplicate them.
        assert!(historical_additions(today, &variants, &additions).is_empty());
    }

    #[test]
    fn respects_existing_service_additions_and_removals() {
        let today = date(2026, 12, 1);
        let variants = vec![
            ("first".to_string(), today),
            ("later".to_string(), date(2026, 12, 5)),
        ];
        let exceptions = vec![
            ("first".to_string(), "20261128".to_string(), "2".to_string()),
            ("later".to_string(), "20261129".to_string(), "1".to_string()),
            ("unrelated".to_string(), "20261130".to_string(), "2".to_string()),
        ];
        assert_eq!(
            historical_additions(today, &variants, &exceptions),
            vec![("first".to_string(), "20261130".to_string(), "1".to_string())]
        );
    }

    #[test]
    fn only_backfills_dates_before_the_published_calendar() {
        let today = date(2026, 12, 1);
        let variants = vec![("first".to_string(), date(2026, 11, 29))];
        assert_eq!(
            historical_additions(today, &variants, &[]),
            vec![("first".to_string(), "20261128".to_string(), "1".to_string())]
        );
    }

    #[test]
    fn ambiguous_earliest_variants_are_not_duplicated() {
        let today = date(2026, 12, 1);
        let variants = vec![("a".to_string(), today), ("b".to_string(), today)];
        assert!(historical_additions(today, &variants, &[]).is_empty());
    }
}
