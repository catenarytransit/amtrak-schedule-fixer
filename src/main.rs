use bytesize::ByteSize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use rgb::RGB8;

const GTFS_URL: &str = "https://content.amtrak.com/content/gtfs/GTFS.zip";

const DOWNLOAD_AND_UNZIP_INIT: bool = true;

const TRIP_SHORT_NAMES_WITH_CALENDAR_FIXES: [&str; 3] = ["2", "343", "422"];

const AGENCY_NAMES_TO_REMOVE: [&str; 2] = [
    "MARC",
    // MARC is removed because the Maryland Transit Administration feed should be used instead
    "Via Rail Canada"
    // Maple Leaf is removed because route_id 68, which includes both the US and Canadian halves, should be used instead
];

const ROUTE_LONG_NAMES_TO_REMOVE: [&str; 1] = [
    "Capitol Corridor"
    // Capitol Corridor is removed because the 511.org GTFS feed should be used instead
];

const SLE_AGENCY_NAME: &str = "Shore Line East";
const SLE_NEW_SHORT_NAME: &str = "SLE";
const SLE_NEW_LONG_NAME: &str = "Shore Line East";
const SLE_NEW_COLOR: RGB8 = RGB8 {r: 0xEA, g: 0x0D, b:0x2A}; // EA0D2A

const LBO_STOP_ID: &str = "LBO";
const LBO_STOP_NAME: &str = "Los Baños Memorial Hospital";
const LBO_LAT: f64 = 37.064091;
const LBO_LON: f64 = -120.861860;

const EWR_STOP_ID: &str = "EWR";
const EWR_LAT: f64 = 40.704444;
const EWR_LON: f64 = -74.190556;

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

    // MARK - Removal of broken shapes
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

    // MARK - Check trips for departure after midnight in non-Eastern timezones
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

            let initial_timezone = chrono_tz::Tz::from_str(initial_timezone_str).unwrap();

            if initial_timezone != chrono_tz::Tz::America__New_York {
                let soonest_hr_to_break = match initial_timezone {
                    chrono_tz::Tz::America__Chicago => 1,
                    chrono_tz::Tz::America__Denver => 2,
                    chrono_tz::Tz::America__Los_Angeles => 3,
                    _ => unreachable!(),
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

    // MARK - Remove problematic agencies and routes
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

    // Identify routes to remove and SLE routes
    let mut route_ids_to_remove: HashSet<String> = HashSet::new();
    let mut sle_route_ids: HashSet<String> = HashSet::new();

    for route in &routes {
        if let Some(agency_id) = route.agency_id.as_ref() {
            // Remove routes by agency ID
            if remove_agency_ids.contains(agency_id) {
                route_ids_to_remove.insert(route.id.clone());
                continue;
            }

            // Identify SLE routes
            if agency_id_to_name
                .get(agency_id)
                .is_some_and(|n| n.as_str() == SLE_AGENCY_NAME)
            {
                sle_route_ids.insert(route.id.clone());
            }
        }

        // Remove routes by long name
        if let Some(route_long_name) = &route.long_name {
            if ROUTE_LONG_NAMES_TO_REMOVE.contains(&route_long_name.as_str()) {
                route_ids_to_remove.insert(route.id.clone());
            }
        }
    }

    // MARK - Rewrite GTFS files
    let mut agency_wtr = csv::Writer::from_path("./amtrak-gtfs/agency.txt")?;
    let mut route_wtr = csv::Writer::from_path("./amtrak-gtfs/routes.txt")?;
    let mut trip_wtr = csv::Writer::from_path("./amtrak-gtfs/trips.txt")?;
    let mut stop_time_wtr = csv::Writer::from_path("./amtrak-gtfs/stop_times.txt")?;
    let mut stop_wtr = csv::Writer::from_path("./amtrak-gtfs/stops.txt")?;
    let mut calendar_wtr = csv::Writer::from_path("./amtrak-gtfs/calendar.txt")?;

    // MARK - Process trips
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

        // Fix the calendar for possibly broken trips
        if let Some(trip_short_name) = &trip.trip_short_name && TRIP_SHORT_NAMES_WITH_CALENDAR_FIXES.contains(&trip_short_name.as_str()) {
            let new_calendar = make_calendar_for_trip_short_name(
                &trip.id,
                &trip_short_name,
                calendar.clone(),
            );

            if let Some(new_calendar) = new_calendar {
                trip.service_id = new_calendar.id.clone();

                calendars_to_write.push(new_calendar);
            }
        }

        // Don't output this trip if it's on a removed route
        if route_ids_to_remove.contains(&trip.route_id) {
            continue;
        }

        // Remove SLE trips whose short name starts with '9'
        // This is because these trips do not actually exist, according to the SLE PDF timetable
        if sle_route_ids.contains(&trip.route_id) {
            if let Some(trip_short_name) = trip.trip_short_name.as_deref() {
                if trip_short_name.starts_with('9') {
                    continue;
                }
            }
        }

        kept_trip_ids.insert(trip.id.clone());
        trip_wtr.serialize(trip)?;
    }
    trip_wtr.flush()?;

    for agency in agencies {
        if agency.id.as_ref().is_some_and(|id| remove_agency_ids.contains(id)) {
            continue;
        }
        agency_wtr.serialize(agency)?;
    }
    agency_wtr.flush()?;

    for stop in stops {
        let mut stop = stop;
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
        let mut route = route;
        if route_ids_to_remove.contains(&route.id) {
            continue;
        }

        // Fix SLE
        if sle_route_ids.contains(&route.id) {
            route.long_name = Some(SLE_NEW_LONG_NAME.to_string());
            route.short_name = Some(SLE_NEW_SHORT_NAME.to_string());
            route.color = Some(SLE_NEW_COLOR);
        }

        route_wtr.serialize(route)?;
    }
    route_wtr.flush()?;

    for calendar_raw in calendars_to_write {
        calendar_wtr.serialize(calendar_raw)?;
    }
    calendar_wtr.flush()?;

    Ok(())
}

fn make_calendar_for_trip_short_name(
    trip_id: &str,
    trip_short_name: &str,
    calendar: gtfs_structures::Calendar,
) -> Option<gtfs_structures::Calendar> {
    let id = format!("catenary-{}-{}", trip_short_name, trip_id);

    match trip_short_name {
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
