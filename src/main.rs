use bytesize::ByteSize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;

// MARK: - Constants

const GTFS_URL: &str = "https://content.amtrak.com/content/gtfs/GTFS.zip";

const DOWNLOAD_AND_UNZIP_INIT: bool = true;

const TRIP_SHORT_NAMES_WITH_CALENDAR_FIXES: [&str; 4] = ["1", "2", "343", "422"];
// Keep the earliest published timetable variant active during the recent
// history window. Backdate service availability, not stop_times; realtime
// departure instants must still match the published timetable.
const HISTORICAL_TRAINS: [&str; 9] = ["1", "2", "5", "6", "41", "4", "8", "42", "517"];
const HISTORY_START: &str = "2026-10-17";

const AGENCY_NAMES_TO_REMOVE: [&str; 3] = [
    // MARC is removed because the Maryland Transit Administration feed should be used instead
    "MARC",
    // Maple Leaf is removed because route_id 68, which includes both the US and Canadian halves, should be used instead
    "Via Rail Canada",
    // Shore Line East is removed because we are using the SLE schedules from PDF
    "Shore Line East",
];

const ROUTE_LONG_NAMES_TO_REMOVE: [&str; 1] = [
    "Capitol Corridor", // Capitol Corridor is removed because the 511.org GTFS feed should be used instead
];

const LBO_STOP_ID: &str = "LBO";
const LBO_STOP_NAME: &str = "Los Baños Memorial Hospital";
const LBO_LAT: f64 = 37.064091;
const LBO_LON: f64 = -120.861860;

const EWR_STOP_ID: &str = "EWR";
const EWR_LAT: f64 = 40.704444;
const EWR_LON: f64 = -74.190556;

// MARK: - main()
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let client = reqwest::Client::new();
    let mut dest = File::create("./amtrak-gtfs.zip")?;

    let target_dir = PathBuf::from("./amtrak-gtfs");

    if DOWNLOAD_AND_UNZIP_INIT {
        let response = client.get(GTFS_URL).send().await?;

        println!("Finished downloading Amtrak GTFS file");

        let data = response.bytes().await?;

        let download_byte_size = ByteSize::b(data.len() as u64);
        println!("{:?} downloaded", download_byte_size);

        dest.write_all(data.as_ref())?;

        let mut zipped_file = File::open("./amtrak-gtfs.zip")?;

        let mut buf: Vec<u8> = vec![];

        // read bytes and pass back error if unable to read
        zipped_file.read_to_end(&mut buf)?;

        zip_extract::extract(Cursor::new(buf), &target_dir, true)?;
    }

    println!("Reading official GTFS file");

    let gtfs_initial_read = gtfs_structures::Gtfs::from_path(&target_dir)?;

    println!("Read took {:?}", gtfs_initial_read.read_duration);

    let gtfs_raw = gtfs_structures::RawGtfs::from_path(&target_dir)?;

    let agencies = gtfs_raw.agencies.unwrap();
    let routes = gtfs_raw.routes.unwrap();
    let trips = gtfs_raw.trips.unwrap();
    let stop_times = gtfs_raw.stop_times.unwrap();
    let stops = gtfs_raw.stops.unwrap();
    let mut calendars_to_write = gtfs_raw.calendar.unwrap().unwrap();
    let mut feed_info = gtfs_raw.feed_info.unwrap().unwrap();

    // MARK: - Removal of broken shapes
    let mut route_ids_to_remove_shapes_from: HashSet<String> = HashSet::new();

    // TODO: Document why California Zephyr and Floridian are broken
    for (route_id, route) in gtfs_initial_read.routes.iter() {
        if let Some(long_name) = &route.long_name {
            if long_name.as_str() == "California Zephyr" || long_name.as_str() == "Floridian" {
                route_ids_to_remove_shapes_from.insert(route_id.clone());
            }
        }
    }

    // Check shapes for jumps of more than 0.1 degrees longitude or latitude
    let threshold_degree_broken: f64 = 0.1;

    let mut broken_shape_ids: HashSet<String> = HashSet::new();

    for (shape_id, shape) in gtfs_initial_read.shapes.iter() {
        let mut is_line_too_stupidly_broken = false;

        for (idx, point) in shape.iter().enumerate().skip(1) {
            if (shape[idx - 1].longitude - point.longitude).abs() > threshold_degree_broken
                || (shape[idx - 1].latitude - point.latitude).abs() > threshold_degree_broken
            {
                is_line_too_stupidly_broken = true;
                break;
            }
        }

        if is_line_too_stupidly_broken {
            broken_shape_ids.insert(shape_id.clone());
        }
    }

    // MARK: - Check trips for departure after midnight in non-Eastern timezones
    //
    // If a trip departs at:
    // - 00:00~01:00 in Central Time
    // - 00:00~02:00 in Mountain Time
    // - 00:00~03:00 in Pacific Time
    // then it is flagged.
    // Flagged trips are only logged; nothing is done to them.
    //
    // TODO: Do we need to deal with daylight savings time and weird timezones such as Arizona and Indiana?
    for (_, trip) in gtfs_initial_read.trips.iter() {
        if gtfs_initial_read
            .routes
            .get(trip.route_id.as_str())
            .unwrap()
            .route_type
            == gtfs_structures::RouteType::Rail
        {
            let first_stop_time = &trip.stop_times[0];

            let departure_from_midnight = first_stop_time.departure_time.unwrap();

            let route = gtfs_initial_read
                .routes
                .get(trip.route_id.as_str())
                .unwrap();

            let initial_timezone_str = first_stop_time.stop.as_ref().timezone.as_ref().unwrap();

            jiff::tz::TimeZone::get(initial_timezone_str)?;

            if initial_timezone_str != "America/New_York" {
                let soonest_hr_to_break = match initial_timezone_str.as_str() {
                    "America/Chicago" => 1,
                    "America/Denver" => 2,
                    "America/Los_Angeles" => 3,
                    _ => continue, // Do not panic on other valid IANA time zones.
                };

                if departure_from_midnight <= (soonest_hr_to_break * 3600) {
                    println!(
                        "Potentially broken: {} {} to {}",
                        trip.trip_short_name.as_ref().unwrap(),
                        route.long_name.as_ref().unwrap(),
                        trip.trip_headsign.as_ref().unwrap()
                    );

                    // println!("{:#?}", service);
                }
            }
        }
    }

    // MARK: - Remove problematic agencies and routes
    let mut agency_id_to_name: HashMap<String, String> = HashMap::new();
    for agency in &agencies {
        if let Some(agency_id) = &agency.id {
            agency_id_to_name.insert(agency_id.clone(), agency.name.clone()); // adjust fields: id/name
        }
    }

    let remove_agency_ids: HashSet<String> = agencies
        .iter()
        .filter(|a| AGENCY_NAMES_TO_REMOVE.contains(&a.name.as_str()))
        .filter_map(|a| a.id.clone())
        .collect();

    // Identify routes to remove
    let mut route_ids_to_remove: HashSet<String> = HashSet::new();

    for route in &routes {
        if let Some(agency_id) = route.agency_id.as_ref() {
            // Remove routes by agency ID
            if remove_agency_ids.contains(agency_id) {
                route_ids_to_remove.insert(route.id.clone());
                continue;
            }
        }

        // Remove routes by long name
        if let Some(route_long_name) = &route.long_name {
            if ROUTE_LONG_NAMES_TO_REMOVE.contains(&route_long_name.as_str()) {
                route_ids_to_remove.insert(route.id.clone());
            }
        }
    }

    // MARK: - Rewrite GTFS files
    let mut agency_wtr = csv::Writer::from_path("./amtrak-gtfs/agency.txt")?;
    let mut route_wtr = csv::Writer::from_path("./amtrak-gtfs/routes.txt")?;
    let mut trip_wtr = csv::Writer::from_path("./amtrak-gtfs/trips.txt")?;
    let mut stop_time_wtr = csv::Writer::from_path("./amtrak-gtfs/stop_times.txt")?;
    let mut stop_wtr = csv::Writer::from_path("./amtrak-gtfs/stops.txt")?;

    // Give each selected variant a private service ID to avoid collisions.
    let mut cloned_exceptions: Vec<(String, String, String)> = Vec::new();
    let mut history_variants: Vec<(String, String, String, String)> = Vec::new();

    // MARK: - Process trips
    let mut kept_trip_ids: HashSet<String> = HashSet::new();
    for trip in trips {
        let mut trip = trip;

        // Remove all bad and broken shapes
        if route_ids_to_remove_shapes_from.contains(&trip.route_id) {
            trip.shape_id = None;
        }

        if let Some(shape_id) = &trip.shape_id {
            if broken_shape_ids.contains(shape_id) {
                trip.shape_id = None;
            }
        }

        // Apply calendar
        let calendar = gtfs_initial_read.calendar.get(&trip.service_id).unwrap();

        if route_ids_to_remove.contains(&trip.route_id) {
            continue;
        }
        let short_name = trip.trip_short_name.as_deref().unwrap_or("");
        let corrected_calendar = if TRIP_SHORT_NAMES_WITH_CALENDAR_FIXES.contains(&short_name) {
            make_calendar_for_trip_short_name(&trip.id, short_name, calendar.clone())
        } else if HISTORICAL_TRAINS.contains(&short_name) {
            let mut private = calendar.clone();
            private.id = format!("catenary-history-{}", trip.id);
            // The earliest Pennsylvanian 42 timetable variant is marked
            // Tuesday-only in Amtrak GTFS despite the daily service. Keep
            // this correction scoped to train 42; the normal variant switch
            // still ends the old calendar when the new variant starts.
            if short_name == "42" {
                private.monday = true;
                private.tuesday = true;
                private.wednesday = true;
                private.thursday = true;
                private.friday = true;
                private.saturday = true;
                private.sunday = true;
            }
            Some(private)
        } else {
            None
        };
        if let Some(new_calendar) = corrected_calendar {
            let original_service = trip.service_id.clone();
            trip.service_id = new_calendar.id.clone();
            if HISTORICAL_TRAINS.contains(&short_name) {
                if let Some(original_trip) = gtfs_initial_read.trips.get(&trip.id) {
                    if let Some(first) = original_trip.stop_times.first() {
                        history_variants.push((
                            trip.route_id.clone(),
                            short_name.to_string(),
                            first.stop.id.clone(),
                            new_calendar.id.clone(),
                        ));
                    }
                }
            }
            if let Some(exceptions) = gtfs_initial_read.calendar_dates.get(&original_service) {
                for entry in exceptions {
                    let kind = if entry.exception_type == gtfs_structures::Exception::Added {
                        "1"
                    } else {
                        "2"
                    };
                    cloned_exceptions.push((
                        new_calendar.id.clone(),
                        entry.date.to_string().replace("-", ""),
                        kind.to_string(),
                    ));
                }
            }
            calendars_to_write.push(new_calendar);
        }

        // Don't output this trip if it's on a removed route
        if route_ids_to_remove.contains(&trip.route_id) {
            continue;
        }

        kept_trip_ids.insert(trip.id.clone());
        trip_wtr.serialize(trip)?;
    }
    trip_wtr.flush()?;

    // MARK: - Process other stuff
    for agency in agencies {
        if agency
            .id
            .as_ref()
            .is_some_and(|id| remove_agency_ids.contains(id))
        {
            continue;
        }
        agency_wtr.serialize(agency)?;
    }
    agency_wtr.flush()?;

    for stop in stops {
        let mut stop = stop;
        // Vancouver Pacific Central Station is in America/Vancouver, not
        // Mountain time. Use an IANA zone so GTFS consumers respect both
        // PST and PDT rather than applying a fixed UTC offset.
        if stop.id == "VAC" {
            stop.timezone = Some("America/Vancouver".to_string());
        }
        // Fix Los Baños bus stop
        // Currently it is listed as being a random house in Flin Flon, MB
        if stop.id.as_str() == LBO_STOP_ID {
            stop.name = Some(LBO_STOP_NAME.to_string());
            stop.latitude = Some(LBO_LAT);
            stop.longitude = Some(LBO_LON);
        }

        // Fix the EWR station location
        // Currently it is listed as EWR's P4 parking garage rather than the actual station
        if stop.id.as_str() == EWR_STOP_ID {
            stop.latitude = Some(EWR_LAT);
            stop.longitude = Some(EWR_LON);
        }

        stop_wtr.serialize(stop)?;
    }
    stop_wtr.flush()?;

    for stop_time in stop_times {
        if kept_trip_ids.contains(stop_time.trip_id.as_str()) {
            stop_time_wtr.serialize(stop_time)?;
        }
    }
    stop_time_wtr.flush()?;

    for route in routes {
        if route_ids_to_remove.contains(&route.id) {
            continue;
        }

        route_wtr.serialize(route)?;
    }
    route_wtr.flush()?;

    // A single earliest variant is eligible for backfill within each
    // route/train/origin group. Tied starts are ambiguous and left unchanged.
    let history_start = HISTORY_START.parse::<jiff::civil::Date>()?;
    let mut groups: HashMap<(String, String, String), Vec<String>> = HashMap::new();
    for (route, train, origin, service_id) in history_variants {
        groups
            .entry((route, train, origin))
            .or_default()
            .push(service_id);
    }
    for ((route, train, origin), ids) in groups {
        let mut variants: Vec<_> = calendars_to_write
            .iter()
            .enumerate()
            .filter(|(_, cal)| ids.contains(&cal.id))
            .map(|(index, cal)| (index, cal.start_date))
            .collect();
        variants.sort_by_key(|(_, date)| *date);
        let Some(&(index, earliest)) = variants.first() else {
            continue;
        };
        if variants.get(1).is_some_and(|(_, next)| *next == earliest) {
            println!("Ambiguous history for {route} train {train}, origin {origin}; skipping");
            continue;
        }
        if earliest.to_string().as_str() <= HISTORY_START {
            continue;
        }
        let first_date = earliest.to_string().replace("-", "");
        let selected_id = calendars_to_write[index].id.clone();
        if cloned_exceptions.iter().any(|(id, date, _)| {
            id == &selected_id && date.as_str() >= "20261017" && date < &first_date
        }) {
            println!("Existing historical exception for {route} train {train}; skipping");
            continue;
        }
        calendars_to_write[index].start_date = history_start.to_string().parse()?;
        println!(
            "Extended {route} train {train} ({origin}) via {selected_id} back to {HISTORY_START}"
        );
    }
    let mut calendar_wtr = csv::Writer::from_path("./amtrak-gtfs/calendar.txt")?;
    for calendar in calendars_to_write {
        calendar_wtr.serialize(calendar)?;
    }
    calendar_wtr.flush()?;

    // Preserve original exceptions and copy them to the new private IDs.
    if !cloned_exceptions.is_empty() {
        let dates_path = target_dir.join("calendar_dates.txt");
        let needs_header = !dates_path.exists() || std::fs::metadata(&dates_path)?.len() == 0;
        let output = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&dates_path)?;
        let mut writer = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(output);
        if needs_header {
            writer.write_record(["service_id", "date", "exception_type"])?;
        }
        let mut seen = HashSet::new();
        for (id, date, kind) in cloned_exceptions {
            if seen.insert((id.clone(), date.clone())) {
                writer.write_record([id, date, kind])?;
            }
        }
        writer.flush()?;
    }

    let feed_info_path = target_dir.join("feed_info.txt");
    if feed_info_path.exists() {
        let mut rdr = csv::Reader::from_path(&feed_info_path)?;
        let headers = rdr.headers()?.clone();
        let start_idx = headers.iter().position(|h| h == "feed_start_date");
        let history_start_gtfs = HISTORY_START.replace("-", "");

        let mut records: Vec<csv::StringRecord> = Vec::new();
        for result in rdr.records() {
            let mut record = result?;
            if let Some(idx) = start_idx {
                if let Some(current_start) = record.get(idx) {
                    if current_start.is_empty() || current_start > history_start_gtfs.as_str() {
                        let mut updated: Vec<String> = record.iter().map(String::from).collect();
                        updated[idx] = history_start_gtfs.clone();
                        record = csv::StringRecord::from(updated);
                    }
                }
            }
            records.push(record);
        }

        let mut wtr = csv::Writer::from_path(&feed_info_path)?;
        wtr.write_record(&headers)?;
        for record in records {
            wtr.write_record(&record)?;
        }
        wtr.flush()?;
    }

    Ok(())
}

// MARK: - make_calendar_for_trip_short_name()
fn make_calendar_for_trip_short_name(
    trip_id: &str,
    trip_short_name: &str,
    calendar: gtfs_structures::Calendar,
) -> Option<gtfs_structures::Calendar> {
    let id = format!("catenary-{}-{}", trip_short_name, trip_id);

    match trip_short_name {
        "1" => Some(gtfs_structures::Calendar {
            id,
            monday: true,
            tuesday: false,
            wednesday: true,
            thursday: false,
            friday: false,
            saturday: true,
            sunday: false,
            start_date: calendar.start_date,
            end_date: calendar.end_date,
        }),
        "2" => Some(gtfs_structures::Calendar {
            id,
            monday: true,
            tuesday: false,
            wednesday: false,
            thursday: true,
            friday: false,
            saturday: true,
            sunday: false,
            start_date: calendar.start_date,
            end_date: calendar.end_date,
        }),
        "343" => Some(gtfs_structures::Calendar {
            id,
            monday: false,
            tuesday: false,
            wednesday: false,
            thursday: false,
            friday: false,
            saturday: true,
            sunday: false,
            start_date: calendar.start_date,
            end_date: calendar.end_date,
        }),
        "422" => Some(gtfs_structures::Calendar {
            id,
            monday: true,
            tuesday: false,
            wednesday: false,
            thursday: true,
            friday: false,
            saturday: true,
            sunday: false,
            start_date: calendar.start_date,
            end_date: calendar.end_date,
        }),
        _ => None,
    }
}
