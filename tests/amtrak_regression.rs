//! Live-feed regression checks. Run with scripts/test-amtrak-no-pfaedle.sh.
//! The executable first writes the corrected GTFS feed to ./amtrak-gtfs.

use jiff::{SignedDuration, civil::date, tz::TimeZoneDatabase};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    path::Path,
};

#[derive(Deserialize)]
struct Trip {
    trip_id: String,
    service_id: String,
    route_id: String,
    trip_short_name: Option<String>,
}
#[derive(Deserialize)]
struct StopTime {
    trip_id: String,
    stop_id: String,
    stop_sequence: u32,
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
    let Some(c) = calendars.get(service) else {
        return false;
    };
    if ymd < c.start_date.as_str() || ymd > c.end_date.as_str() {
        return false;
    }
    [
        c.monday,
        c.tuesday,
        c.wednesday,
        c.thursday,
        c.friday,
        c.saturday,
        c.sunday,
    ][weekday]
        == 1
}

#[test]
#[ignore = "downloads live Amtrak data; run scripts/test-amtrak-no-pfaedle.sh"]
fn amtrak_november_2026_los_angeles_schedule() -> Result<(), Box<dyn Error>> {
    let root = Path::new("amtrak-gtfs");
    let trips: Vec<Trip> = csv_rows(&root.join("trips.txt"))?;
    let stop_times: Vec<StopTime> = csv_rows(&root.join("stop_times.txt"))?;
    let calendars: HashMap<_, _> = csv_rows::<Calendar>(&root.join("calendar.txt"))?
        .into_iter()
        .map(|c| (c.service_id.clone(), c))
        .collect();
    let exception_file = root.join("calendar_dates.txt");
    let exceptions: HashMap<_, _> = if exception_file.exists() {
        csv_rows::<CalendarDate>(&exception_file)?
            .into_iter()
            .map(|d| ((d.service_id, d.date), d.exception_type))
            .collect()
    } else {
        HashMap::new()
    };

    let la = TimeZoneDatabase::bundled().get("America/Los_Angeles")?;
    let eastern = TimeZoneDatabase::bundled().get("America/New_York")?;
    let desired_departures = [
        date(2026, 11, 15).at(22, 0, 0, 0).to_zoned_datetime(la.clone())?,
        date(2026, 11, 18).at(22, 0, 0, 0).to_zoned_datetime(la.clone())?,
        date(2026, 11, 20).at(22, 0, 0, 0).to_zoned_datetime(la.clone())?,
    ];
    let central = TimeZoneDatabase::bundled().get("America/Chicago")?;
    let desired_new_orleans_departures = [
        date(2026, 11, 16)
            .at(9, 0, 0, 0)
            .to_zoned_datetime(central.clone())?,
        date(2026, 11, 18)
            .at(9, 0, 0, 0)
            .to_zoned_datetime(central.clone())?,
        date(2026, 11, 21)
            .at(9, 0, 0, 0)
            .to_zoned_datetime(central.clone())?,
    ];
    let desired_arrival = date(2026, 11, 16).at(5, 35, 0, 0).to_zoned_datetime(la.clone())?;

    // Amtrak's published GTFS times are in its agency timezone (Eastern),
    // including stop_times at Los Angeles. GTFS permits hours >= 24, so a
    // trip's service day and its local civil date need not be the same.
    let candidate_days = [
        (date(2026, 11, 14), "20261114", 5usize),
        (date(2026, 11, 15), "20261115", 6usize),
        (date(2026, 11, 16), "20261116", 0usize),
        (date(2026, 11, 17), "20261117", 1usize),
        (date(2026, 11, 18), "20261118", 2usize),
        (date(2026, 11, 19), "20261119", 3usize),
        (date(2026, 11, 20), "20261120", 4usize),
        (date(2026, 11, 21), "20261121", 5usize),
        (date(2026, 11, 22), "20261122", 6usize),
    ];
    let by_trip: HashMap<_, _> = trips.iter().map(|t| (t.trip_id.as_str(), t)).collect();
    let mut found_departure = [false; 3];
    let mut found_new_orleans_departure = [false; 3];
    let mut found_arrival = HashSet::new();

    for st in &stop_times {
        if st.stop_id != "LAX" && st.stop_id != "NOL" {
            continue;
        }
        let Some(trip) = by_trip.get(st.trip_id.as_str()) else {
            continue;
        };
        if trip.trip_short_name.as_deref() != Some("1")
            && trip.trip_short_name.as_deref() != Some("2")
        {
            continue;
        }
        for (day, ymd, weekday) in candidate_days {
            if !active_on(&trip.service_id, ymd, weekday, &calendars, &exceptions) {
                continue;
            }
            let origin = day.at(0, 0, 0, 0).to_zoned_datetime(eastern.clone())?;
            if st.stop_id == "NOL" && trip.trip_short_name.as_deref() == Some("1") {
                if let Some(seconds) = st.departure_time.as_deref().and_then(seconds_from_gtfs) {
                    let instant = origin.checked_add(SignedDuration::from_secs(seconds))?;
                    for (index, expected) in desired_new_orleans_departures.iter().enumerate() {
                        if instant.timestamp() == expected.timestamp() {
                            found_new_orleans_departure[index] = true;
                        }
                    }
                }
            }
            if st.stop_id == "LAX" && trip.trip_short_name.as_deref() == Some("2") {
                if let Some(seconds) = st.departure_time.as_deref().and_then(seconds_from_gtfs) {
                    let instant = origin.checked_add(SignedDuration::from_secs(seconds))?;
                    for (index, expected) in desired_departures.iter().enumerate() {
                        if instant.timestamp() == expected.timestamp() {
                            found_departure[index] = true;
                        }
                    }
                }
            }
            if st.stop_id == "LAX" && trip.trip_short_name.as_deref() == Some("1") {
                if let Some(seconds) = st.arrival_time.as_deref().and_then(seconds_from_gtfs) {
                    let instant = origin.checked_add(SignedDuration::from_secs(seconds))?;
                    if instant.timestamp() == desired_arrival.timestamp() {
                        found_arrival.insert(trip.trip_id.as_str());
                    }
                }
            }
        }
    }
    for (index, found) in found_departure.iter().enumerate() {
        assert!(
            *found,
            "Missing Sunset Limited 2 LAX departure at {}",
            desired_departures[index]
        );
    }
    for (index, found) in found_new_orleans_departure.iter().enumerate() {
        assert!(
            *found,
            "Missing Sunset Limited 1 NOL departure at {}",
            desired_new_orleans_departures[index]
        );
    }
    assert!(
        !found_arrival.is_empty(),
        "No active train 1 arrival at LAX at 05:35 America/Los_Angeles on 2026-11-16"
    );
    Ok(())
}

#[test]
fn cascades_vancouver_bc_timezone_respects_permanent_pacific_time() -> Result<(), Box<dyn Error>> {
    let vancouver = TimeZoneDatabase::bundled().get("America/Vancouver")?;
    let eastern = TimeZoneDatabase::bundled().get("America/New_York")?;
    let us_pacific = TimeZoneDatabase::bundled().get("America/Los_Angeles")?;
    // BC adopted permanent UTC-7 in 2026; the US still falls back.
    // https://news.gov.bc.ca/releases/2026AG0013-000209
    for (day, eastern_hour, us_hour, utc) in [
        (date(2025, 11, 16), 11, 8, "2025-11-16T16:25:00Z"),
        (date(2026, 7, 16), 11, 8, "2026-07-16T15:25:00Z"),
        (date(2026, 10, 31), 11, 8, "2026-10-31T15:25:00Z"),
        (date(2026, 11, 1), 10, 7, "2026-11-01T15:25:00Z"),
        (date(2026, 11, 16), 10, 7, "2026-11-16T15:25:00Z"),
        (date(2027, 1, 16), 10, 7, "2027-01-16T15:25:00Z"),
    ] {
        let local = day.at(8, 25, 0, 0).to_zoned_datetime(vancouver.clone())?;
        let eastern_stop = day.at(eastern_hour, 25, 0, 0).to_zoned_datetime(eastern.clone())?;
        let us_stop = day.at(us_hour, 25, 0, 0).to_zoned_datetime(us_pacific.clone())?;
        assert_eq!(local.timestamp().to_string(), utc, "Vancouver on {day}");
        assert_eq!(local.timestamp(), eastern_stop.timestamp(), "Eastern GTFS on {day}");
        assert_eq!(local.timestamp(), us_stop.timestamp(), "US Pacific on {day}");
    }
    Ok(())
}

#[test]
#[ignore = "requires processed Amtrak GTFS; run scripts/test-amtrak-no-pfaedle.sh"]
fn cascades_cross_border_trains_match_the_published_timetable() -> Result<(), Box<dyn Error>> {
    use jiff::civil::Time;

    #[derive(Deserialize)]
    struct Stop {
        stop_id: String,
        stop_timezone: Option<String>,
    }

    let root = Path::new("amtrak-gtfs");
    let trips: Vec<Trip> = csv_rows(&root.join("trips.txt"))?;
    let times: Vec<StopTime> = csv_rows(&root.join("stop_times.txt"))?;
    let calendars: HashMap<_, _> = csv_rows::<Calendar>(&root.join("calendar.txt"))?
        .into_iter()
        .map(|c| (c.service_id.clone(), c))
        .collect();
    let exceptions: HashMap<_, _> = if root.join("calendar_dates.txt").exists() {
        csv_rows::<CalendarDate>(&root.join("calendar_dates.txt"))?
            .into_iter()
            .map(|d| ((d.service_id, d.date), d.exception_type))
            .collect()
    } else {
        HashMap::new()
    };
    let stops = csv_rows::<Stop>(&root.join("stops.txt"))?;
    for (id, zone) in [("VAC", "America/Vancouver"), ("VAN", "America/Los_Angeles")] {
        assert_eq!(
            stops
                .iter()
                .find(|stop| stop.stop_id == id)
                .and_then(|stop| stop.stop_timezone.as_deref()),
            Some(zone),
            "incorrect timezone in stops.txt for {id}",
        );
    }

    let db = TimeZoneDatabase::bundled();
    let eastern = db.get("America/New_York")?;
    let vancouver = db.get("America/Vancouver")?;
    let us_pacific = db.get("America/Los_Angeles")?;

    // Daily timetable effective November 1, 2026. Times are station-local.
    let numbers = ["516", "517", "518", "519"];
    let timetable = [
        ("VAC", ["13:30", "08:25", "23:00", "17:55"]),
        ("BEL", ["10:37", "09:14", "20:07", "18:44"]),
        ("MVW", ["10:04", "09:43", "19:34", "19:13"]),
        ("STW", ["09:51", "09:57", "19:21", "19:27"]),
        ("EVR", ["09:21", "10:37", "18:51", "20:07"]),
        ("EDM", ["08:56", "11:00", "18:26", "20:30"]),
        ("SEA", ["08:30", "11:40", "18:00", "21:10"]),
    ];
    for local_day in [date(2026, 11, 21), date(2026, 11, 24)] {
        for (column, number) in numbers.into_iter().enumerate() {
            let northbound = column % 2 == 0;
            for (stop_id, published_times) in timetable {
                let zone = if stop_id == "VAC" {
                    &vancouver
                } else {
                    &us_pacific
                };
                let clock = published_times[column].parse::<Time>()?;
                let expected = local_day
                    .to_datetime(clock)
                    .to_zoned_datetime(zone.clone())?
                    .timestamp();
                let use_arrival =
                    (northbound && stop_id == "VAC") || (!northbound && stop_id == "SEA");
                let eastern_day = eastern.to_datetime(expected).date();
                let mut matching_rows = 0;
                // A GTFS stop_time can exceed 24:00, including train 518's northbound segment.
                for back in 0..=2 {
                    let service_day = eastern_day - jiff::Span::new().days(back);
                    let ymd = service_day.strftime("%Y%m%d").to_string();
                    let weekday = service_day.weekday().to_monday_zero_offset() as usize;
                    let midnight = service_day
                        .at(0, 0, 0, 0)
                        .to_zoned_datetime(eastern.clone())?;
                    for trip in trips.iter().filter(|trip| {
                        trip.trip_short_name.as_deref() == Some(number)
                            && active_on(&trip.service_id, &ymd, weekday, &calendars, &exceptions)
                    }) {
                        for stop in times
                            .iter()
                            .filter(|stop| stop.trip_id == trip.trip_id && stop.stop_id == stop_id)
                        {
                            let raw = if use_arrival {
                                &stop.arrival_time
                            } else {
                                &stop.departure_time
                            };
                            if let Some(seconds) = raw.as_deref().and_then(seconds_from_gtfs) {
                                let actual =
                                    midnight.checked_add(SignedDuration::from_secs(seconds))?;
                                matching_rows += usize::from(actual.timestamp() == expected);
                            }
                        }
                    }
                }
                assert_eq!(
                    matching_rows, 1,
                    "train {number} at {stop_id} must have one event at {expected} on {local_day}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn bundled_database_matches_latest_iana_release() {
    if let Ok(latest) = std::env::var("LATEST_IANA_TZDB_VERSION") {
        assert_eq!(jiff_tzdb::VERSION, latest.trim());
    }
}

#[test]
fn gtfs_times_allow_service_day_overflow() {
    assert_eq!(seconds_from_gtfs("25:00:00"), Some(90_000));
    assert_eq!(seconds_from_gtfs("05:35:00"), Some(20_100));
    assert_eq!(seconds_from_gtfs("24:00:00"), Some(86_400));
    assert_eq!(seconds_from_gtfs("05:60:00"), None);
}

#[test]
#[ignore = "requires processed Amtrak GTFS; run scripts/test-amtrak-no-pfaedle.sh"]
fn historical_long_distance_trips_have_coverage_without_variant_collisions()
-> Result<(), Box<dyn Error>> {
    let root = Path::new("amtrak-gtfs");
    let trips: Vec<Trip> = csv_rows(&root.join("trips.txt"))?;
    let times: Vec<StopTime> = csv_rows(&root.join("stop_times.txt"))?;
    let calendars: HashMap<_, _> = csv_rows::<Calendar>(&root.join("calendar.txt"))?
        .into_iter()
        .map(|c| (c.service_id.clone(), c))
        .collect();
    let exceptions: HashMap<_, _> = if root.join("calendar_dates.txt").exists() {
        csv_rows::<CalendarDate>(&root.join("calendar_dates.txt"))?
            .into_iter()
            .map(|d| ((d.service_id, d.date), d.exception_type))
            .collect()
    } else {
        HashMap::new()
    };
    let mut origins = HashMap::<String, String>::new();
    for stop in times {
        origins.entry(stop.trip_id).or_insert(stop.stop_id);
    }
    // Amtrak agency calendar is Eastern. Historical fixtures from the failing
    // November 16 live feed must remain active, without overlapping variants.
    for (number, day) in [
        ("2", "20261116"),
        ("5", "20261114"),
        ("6", "20261114"),
        ("41", "20261114"),
        ("4", "20261114"),
        ("8", "20261114"),
        // Pennsylvanian 42 previously started on November 17 in the
        // published feed, leaving its November 16 service unrepresented.
        ("42", "20261116"),
        // Cascades 517 has a winter variant starting November 15.
        ("517", "20261116"),
    ] {
        let day_date = jiff::civil::Date::strptime("%Y%m%d", day)?;
        let weekday = match day_date.weekday() {
            jiff::civil::Weekday::Monday => 0,
            jiff::civil::Weekday::Tuesday => 1,
            jiff::civil::Weekday::Wednesday => 2,
            jiff::civil::Weekday::Thursday => 3,
            jiff::civil::Weekday::Friday => 4,
            jiff::civil::Weekday::Saturday => 5,
            jiff::civil::Weekday::Sunday => 6,
        };
        let active = trips
            .iter()
            .filter(|t| t.trip_short_name.as_deref() == Some(number))
            .filter(|t| active_on(&t.service_id, day, weekday, &calendars, &exceptions))
            .count();
        assert!(
            active > 0,
            "No active historical service for train {number} on {day}"
        );
    }
    // Across the entire required 30-day history window, the same train,
    // route and origin must not run twice because of backfilled variants.
    for offset in 0..=30i64 {
        let day = date(2026, 10, 17) + jiff::Span::new().days(offset);
        let day_string = day.strftime("%Y%m%d").to_string();
        let weekday = match day.weekday() {
            jiff::civil::Weekday::Monday => 0,
            jiff::civil::Weekday::Tuesday => 1,
            jiff::civil::Weekday::Wednesday => 2,
            jiff::civil::Weekday::Thursday => 3,
            jiff::civil::Weekday::Friday => 4,
            jiff::civil::Weekday::Saturday => 5,
            jiff::civil::Weekday::Sunday => 6,
        };
        let mut active = HashSet::new();
        for trip in trips.iter().filter(|t| {
            matches!(
                t.trip_short_name.as_deref(),
                Some("1" | "2" | "5" | "6" | "41" | "4" | "8" | "42" | "517")
            )
        }) {
            if active_on(
                &trip.service_id,
                &day_string,
                weekday,
                &calendars,
                &exceptions,
            ) {
                let key = (
                    trip.route_id.clone(),
                    trip.trip_short_name.clone(),
                    origins.get(&trip.trip_id).cloned(),
                );
                assert!(
                    active.insert(key.clone()),
                    "Duplicate active train variant on {day_string}: {key:?}"
                );
            }
        }
    }
    Ok(())
}

#[path = "../src/surfliner.rs"]
mod surfliner;

#[test]
#[ignore = "requires processed Amtrak GTFS; run scripts/test-amtrak-no-pfaedle.sh"]
fn daily_surfliner_emergency_schedule_november_23_to_29() -> Result<(), Box<dyn Error + Send + Sync>> {
    surfliner::verify(Path::new("amtrak-gtfs"), date(2026, 11, 23), date(2026, 11, 29))
}

#[path = "../src/coast_starlight.rs"]
mod coast_starlight;

#[test]
#[ignore = "requires processed Amtrak GTFS; run scripts/test-amtrak-no-pfaedle.sh"]
fn coast_starlight_daily_service_includes_recent_history() -> Result<(), Box<dyn Error>> {
    let root = Path::new("amtrak-gtfs");
    let trips: Vec<Trip> = csv_rows(&root.join("trips.txt"))?;
    let times: Vec<StopTime> = csv_rows(&root.join("stop_times.txt"))?;
    let calendars: HashMap<_, _> = csv_rows::<Calendar>(&root.join("calendar.txt"))?
        .into_iter()
        .map(|c| (c.service_id.clone(), c))
        .collect();
    let exceptions: HashMap<_, _> = if root.join("calendar_dates.txt").exists() {
        csv_rows::<CalendarDate>(&root.join("calendar_dates.txt"))?
            .into_iter()
            .map(|d| ((d.service_id, d.date), d.exception_type))
            .collect()
    } else {
        HashMap::new()
    };

    // Use stop_sequence rather than assuming CSV rows are ordered.
    let mut endpoints: HashMap<&str, (&StopTime, &StopTime)> = HashMap::new();
    for stop in &times {
        endpoints
            .entry(stop.trip_id.as_str())
            .and_modify(|(first, last)| {
                if stop.stop_sequence < first.stop_sequence {
                    *first = stop;
                }
                if stop.stop_sequence > last.stop_sequence {
                    *last = stop;
                }
            })
            .or_insert((stop, stop));
    }

    let eastern = TimeZoneDatabase::bundled().get("America/New_York")?;
    let today = eastern.to_datetime(jiff::Timestamp::now()).date();

    for (number, origin, terminal) in [("11", "SEA", "LAX"), ("14", "LAX", "SEA")] {
        for offset in 0..=coast_starlight::HISTORY_DAYS {
            let day = today - jiff::Span::new().days(offset);
            let ymd = day.strftime("%Y%m%d").to_string();
            let weekday = match day.weekday() {
                jiff::civil::Weekday::Monday => 0,
                jiff::civil::Weekday::Tuesday => 1,
                jiff::civil::Weekday::Wednesday => 2,
                jiff::civil::Weekday::Thursday => 3,
                jiff::civil::Weekday::Friday => 4,
                jiff::civil::Weekday::Saturday => 5,
                jiff::civil::Weekday::Sunday => 6,
            };
            let active: Vec<_> = trips
                .iter()
                .filter(|trip| trip.trip_short_name.as_deref() == Some(number))
                .filter(|trip| active_on(&trip.service_id, &ymd, weekday, &calendars, &exceptions))
                .filter_map(|trip| endpoints.get(trip.trip_id.as_str()))
                .filter(|(first, last)| first.stop_id == origin && last.stop_id == terminal)
                .collect();
            assert_eq!(
                active.len(),
                1,
                "Coast Starlight {number} ({origin} -> {terminal}) on {day}: expected exactly one active full-route trip"
            );
            let (first, last) = *active[0];
            let departure = first.departure_time.as_deref().and_then(seconds_from_gtfs);
            let arrival = last.arrival_time.as_deref().and_then(seconds_from_gtfs);
            assert!(
                matches!((departure, arrival), (Some(dep), Some(arr)) if arr > dep && arr >= 86400),
                "Coast Starlight {number} on {day} must retain its overnight stop times"
            );
            println!("Coast Starlight {number} on {day}: {}", first.trip_id);
        }
    }
    Ok(())
}
