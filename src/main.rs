use bytesize::ByteSize;
use chrono::Datelike;
use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

mod routes_list;

const GTFS_URL: &str = "https://content.amtrak.com/content/gtfs/GTFS.zip";

async fn get_route_data(
    route: &str,
    start_date: &chrono::NaiveDate,
    end_date: &chrono::NaiveDate,
    client: Option<reqwest::Client>,
) {
}

const DOWNLOAD_AND_UNZIP_INIT: bool = true;

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
        let read = zipped_file.read_to_end(&mut buf)?;

        zip_extract::extract(Cursor::new(buf), &target_dir, true)?;
    }

    //fetch the amtrak route list from their website

    //let routes_list_from_website = routes_list::fetch_and_decode_routes(client.clone()).await?;

    //println!("{} routes found on their website", routes_list_from_website.len());

    println!("Reading official GTFS file");

    let gtfs_initial_read = gtfs_structures::Gtfs::from_path(&target_dir)?;

    println!("Read took {:?}", gtfs_initial_read.read_duration);

    let mut possible_trip_ids_to_fix: Vec<String> = vec![];

    let mut surfliner_services_to_cancel: Vec<String> = vec![];

    let mut calendar_id_to_route_ids: HashMap<String, HashSet<String>> = HashMap::new();

    for (trip_id, trip) in gtfs_initial_read.trips.iter() {
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

            let service = gtfs_initial_read
                .calendar
                .get(trip.service_id.as_str())
                .unwrap();

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

                    if route.long_name.as_ref().unwrap() != "Pacific Surfliner" {
                        possible_trip_ids_to_fix.push(trip_id.clone());
                    }
                }
            }

            if route.long_name.as_ref().unwrap() == "Pacific Surfliner" {
                // println!("Surfliner {}", trip.trip_headsign.as_ref().unwrap());

                // println!("{:?}", service);

                surfliner_services_to_cancel.push(service.id.clone());
            }

            calendar_id_to_route_ids
                .entry(service.id.clone())
                .and_modify(|x| {
                    x.insert(route.id.clone());
                })
                .or_insert(HashSet::from_iter(vec![route.id.clone()]));
        }
    }

    surfliner_services_to_cancel.sort();
    surfliner_services_to_cancel.dedup();

    let gtfs_raw = gtfs_structures::RawGtfs::from_path(&target_dir)?;

    let mut trip_wtr = csv::Writer::from_path("./amtrak-gtfs/trips.txt")?;
    let mut calendar_wtr = csv::Writer::from_path("./amtrak-gtfs/calendar.txt")?;

    let mut calendars_to_write = gtfs_raw.calendar.unwrap().unwrap();

    let trips_to_process = gtfs_raw.trips.unwrap();

    for trip in trips_to_process {
        let mut trip = trip;

        let calendar = gtfs_initial_read.calendar.get(&trip.service_id).unwrap();

        if possible_trip_ids_to_fix.contains(&trip.id) {
            let new_calendar = make_calendar_for_trip_short_name(
                &trip.id,
                &trip.trip_short_name.as_ref().unwrap(),
                calendar.clone(),
            );

            if let Some(new_calendar) = new_calendar {
                trip.service_id = new_calendar.id.clone();

                calendars_to_write.push(new_calendar);
            }
        }

        trip_wtr.serialize(trip);
    }

    //write everything back to the files
    for calendar_raw in calendars_to_write {
        calendar_wtr.serialize(calendar_raw);
    }

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

fn calendar_to_string_to_add(calendar: &gtfs_structures::Calendar) -> String {
    format!(
        "{},{},{},{},{},{},{},{},{},{}",
        calendar.id,
        calendar.monday,
        calendar.tuesday,
        calendar.wednesday,
        calendar.thursday,
        calendar.friday,
        calendar.saturday,
        calendar.sunday,
        naive_date_to_gtfs_str(&calendar.start_date),
        naive_date_to_gtfs_str(&calendar.end_date)
    )
}

fn naive_date_to_gtfs_str(date: &chrono::NaiveDate) -> String {
    format!("{}{}{}", date.year(), date.month(), date.day())
}
