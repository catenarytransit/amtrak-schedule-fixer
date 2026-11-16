//! Live-feed regression checks. Run with scripts/test-amtrak-no-pfaedle.sh.
//! The executable first writes the corrected GTFS feed to ./amtrak-gtfs.

use jiff::{civil::date, tz::TimeZone, SignedDuration};
use serde::Deserialize;
use std::{collections::{HashMap, HashSet}, error::Error, path::Path};

#[derive(Deserialize)]
struct Trip {
    trip_id: String,
    service_id: String,
    trip_short_name: Option<String>,
}
#[derive(Deserialize)]
struct StopTime {
    trip_id: String,
    stop_id: String,
    arrival_time: Option<String>,
    departure_time: Option<String>,
}
#[derive(Deserialize)]
struct Calendar {
    service_id: String,
    monday: u8,
    tuesday: u8,
    wednesday: u8,
    thursday: u8,
    friday: u8,
    saturday: u8,
    sunday: u8,
    start_date: String,
    end_date: String,
}
#[derive(Deserialize)]
struct CalendarDate {
    service_id: String,
    date: String,
    exception_type: u8,
}

fn csv_rows<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, Box<dyn Error>> {
    let mut reader = csv::Reader::from_path(path)?;
    Ok(reader.deserialize().collect::<Result<Vec<T>, _>>()?)
}

fn seconds_from_gtfs(value: &str) -> Option<i64> {
    let mut fields = value.split(':');
    let h: i64 = fields.next()?.parse().ok()?;
    let m: i64 = fields.next()?.parse().ok()?;
    let s: i64 = fields.next()?.parse().ok()?;
    if fields.next().is_some() || h < 0 || !(0..60).contains(&m) || !(0..60).contains(&s) {
        return None;
    }
    Some(h * 3600 + m * 60 + s)
}

fn active_on(
    service: &str,
    ymd: &str,
    weekday: usize,
    calendars: &HashMap<String, Calendar>,
    exceptions: &HashMap<(String, String), u8>,
) -> bool {
    if let Some(&kind) = exceptions.get(&(service.to_owned(), ymd.to_owned())) {
        return kind == 1;
    }
    let Some(c) = calendars.get(service) else { return false; };
    if ymd < c.start_date.as_str() || ymd > c.end_date.as_str() {
        return false;
    }
    [c.monday, c.tuesday, c.wednesday, c.thursday, c.friday, c.saturday, c.sunday][weekday] == 1
}

#[test]
#[ignore = "downloads live Amtrak data; run scripts/test-amtrak-no-pfaedle.sh"]
fn amtrak_november_2026_los_angeles_schedule() -> Result<(), Box<dyn Error>> {
    let root = Path::new("amtrak-gtfs");
    let trips: Vec<Trip> = csv_rows(&root.join("trips.txt"))?;
    let stop_times: Vec<StopTime> = csv_rows(&root.join("stop_times.txt"))?;
    let calendars: HashMap<_, _> = csv_rows::<Calendar>(&root.join("calendar.txt"))?
        .into_iter().map(|c| (c.service_id.clone(), c)).collect();
    let exception_file = root.join("calendar_dates.txt");
    let exceptions: HashMap<_, _> = if exception_file.exists() {
        csv_rows::<CalendarDate>(&exception_file)?.into_iter()
            .map(|d| ((d.service_id, d.date), d.exception_type)).collect()
    } else { HashMap::new() };

    let la = TimeZone::get("America/Los_Angeles")?;
    let eastern = TimeZone::get("America/New_York")?;
    let desired_departure = date(2026, 11, 15).at(22, 0, 0, 0).to_zoned(la.clone())?;
    let desired_arrival = date(2026, 11, 16).at(5, 35, 0, 0).to_zoned(la.clone())?;

    // Amtrak's published GTFS times are in its agency timezone (Eastern),
    // including stop_times at Los Angeles. GTFS permits hours >= 24, so a
    // trip's service day and its local civil date need not be the same.
    let candidate_days = [
        (date(2026, 11, 14), "20261114", 5usize),
        (date(2026, 11, 15), "20261115", 6usize),
        (date(2026, 11, 16), "20261116", 0usize),
    ];
    let by_trip: HashMap<_, _> = trips.iter().map(|t| (t.trip_id.as_str(), t)).collect();
    let mut found_departure = HashSet::new();
    let mut found_arrival = HashSet::new();

    for st in &stop_times {
        if st.stop_id != "LAX" { continue; }
        let Some(trip) = by_trip.get(st.trip_id.as_str()) else { continue; };
        if trip.trip_short_name.as_deref() != Some("1")
            && trip.trip_short_name.as_deref() != Some("2") { continue; }
        for (day, ymd, weekday) in candidate_days {
            if !active_on(&trip.service_id, ymd, weekday, &calendars, &exceptions) { continue; }
            let origin = day.at(0, 0, 0, 0).to_zoned(eastern.clone())?;
            if trip.trip_short_name.as_deref() == Some("2") {
                if let Some(seconds) = st.departure_time.as_deref().and_then(seconds_from_gtfs) {
                    let instant = origin.checked_add(SignedDuration::from_secs(seconds))?;
                    if instant.timestamp() == desired_departure.timestamp() {
                        found_departure.insert(trip.trip_id.as_str());
                    }
                }
            }
            if trip.trip_short_name.as_deref() == Some("1") {
                if let Some(seconds) = st.arrival_time.as_deref().and_then(seconds_from_gtfs) {
                    let instant = origin.checked_add(SignedDuration::from_secs(seconds))?;
                    if instant.timestamp() == desired_arrival.timestamp() {
                        found_arrival.insert(trip.trip_id.as_str());
                    }
                }
            }
        }
    }
    assert!(!found_departure.is_empty(),
        "No active train 2 departure from LAX at 22:00 America/Los_Angeles on 2026-11-15");
    assert!(!found_arrival.is_empty(),
        "No active train 1 arrival at LAX at 05:35 America/Los_Angeles on 2026-11-16");
    Ok(())
}

#[test]
fn gtfs_times_allow_service_day_overflow() {
    assert_eq!(seconds_from_gtfs("25:00:00"), Some(90_000));
    assert_eq!(seconds_from_gtfs("05:35:00"), Some(20_100));
    assert_eq!(seconds_from_gtfs("24:00:00"), Some(86_400));
    assert_eq!(seconds_from_gtfs("05:60:00"), None);
}
