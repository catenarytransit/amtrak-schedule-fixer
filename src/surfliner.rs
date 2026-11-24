//! Temporary, date-bounded Pacific Surfliner emergency timetable.
//! Source: Pacific Surfliner emergency bus bridge timetable, revised 2026-11-16.
//! https://www.pacificsurfliner.com/globalassets/pdfs/schedules/13338768279_pacific-surfliner-timetable_sb-slo-emergency-bus-bridge-schedule_rev-11-16-26.pdf
//! End date: operator travel advisory (Oct 17-Dec 7, 2026).
//! https://www.pacificsurfliner.com/plan-your-trip/alerts/travel-advisories/
//!
//! The PDF uses Pacific local time; Amtrak's GTFS agency clock is Eastern.
//! Only the affected service dates are changed. Existing matching trips (and
//! their realtime IDs) are preferred over synthetic trips. Regular numbered
//! Amtrak Connection buses are not rewritten by this patch.

use jiff::{civil::{Date, Weekday, date}, Span};
use std::{collections::{BTreeMap, HashMap, HashSet}, error::Error, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
type Row = HashMap<String, String>;
const BRIDGE_ROUTE: &str = "catenary-surfliner-bus-bridge-202611";
const PREFIX: &str = "catenary-surfliner-202611";

fn first_day() -> Date { date(2026, 11, 16) }
fn last_day() -> Date { date(2026, 12, 7) }
fn key(day: Date) -> String { day.strftime("%Y%m%d").to_string() }
fn shift(day: Date, days: i64) -> Date { day + Span::new().days(days) }

fn value<'a>(r: &'a Row, k: &str) -> &'a str {
    r.get(k).map(String::as_str).unwrap_or("")
}
fn set(r: &mut Row, k: &str, v: impl ToString) {
    r.insert(k.to_owned(), v.to_string());
}

struct Table {
    headers: Vec<String>,
    rows: Vec<Row>,
}
impl Table {
    fn read(root: &Path, name: &str) -> Result<Self> {
        let mut reader = csv::Reader::from_path(root.join(name))?;
        let headers: Vec<String> = reader.headers()?.iter().map(str::to_owned).collect();
        let rows = reader.records().map(|rec| {
            rec.map(|rec| headers.iter().cloned().zip(rec.iter().map(str::to_owned)).collect())
        }).collect::<std::result::Result<Vec<Row>, _>>()?;
        Ok(Self { headers, rows })
    }
    fn dates(root: &Path) -> Result<Self> {
        let path = root.join("calendar_dates.txt");
        if path.exists() && std::fs::metadata(&path)?.len() > 0 {
            Self::read(root, "calendar_dates.txt")
        } else {
            Ok(Self { headers: ["service_id", "date", "exception_type"].map(str::to_owned).to_vec(), rows: vec![] })
        }
    }
    fn write(&self, root: &Path, name: &str) -> Result<()> {
        let mut writer = csv::Writer::from_path(root.join(name))?;
        writer.write_record(&self.headers)?;
        for row in &self.rows {
            writer.write_record(self.headers.iter().map(|h| value(row, h)))?;
        }
        writer.flush()?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct Call {
    stop: &'static str,
    arrival: i64,
    departure: i64,
    explicit_arrival: bool,
}
#[derive(Clone, Debug)]
struct Leg {
    number: &'static str,
    bus: bool,
    calls: Vec<Call>,
}

fn hhmm(t: &str) -> i64 {
    let n: i64 = t.parse().expect("invalid published HHMM");
    assert!(n % 100 < 60);
    (n / 100 * 60 + n % 100) * 60 + 3 * 3600
}
fn calls(spec: &'static str) -> Vec<Call> {
    spec.split_whitespace().map(|entry| {
        let (stop, t) = entry.split_once(':').unwrap();
        let (a, d) = t.split_once('/').unwrap_or((t, t));
        Call { stop, arrival: hhmm(a), departure: hhmm(d), explicit_arrival: a != d }
    }).collect()
}

// Pages 1-2, blue rail columns. A/B denotes a separately printed arrival
// and departure. 24xx/25xx denote the following Pacific civil day.
const TRAINS: &[(&str, &str)] = &[
    ("562", "LAX:0610 FUL:0641 ANA:0649 SNA:0701 IRV:0712 SNC:0726 OSD:0805 SOL:0820 OLT:0851 SAN:0904"),
    ("564", "LAX:0710 FUL:0741 ANA:0749 SNA:0801 IRV:0812 SNC:0825 SNP:0838 OSD:0908 SOL:0923 OLT:0954 SAN:1007"),
    ("566", "LAX:0810 FUL:0841 ANA:0849 SNA:0901 IRV:0912 SNC:0925 SNP:0938 OSD:1008 SOL:1023 OLT:1054 SAN:1107"),
    ("770", "GTA:0639 SBA:0650/0653 CPN:0708 VEC:0739 OXN:0757 CML:0810 MPK:0824 SIM:0841 CWT:0853 NRG:0859 VNC:0909 BUR:0917 BBK:0922 GDL:0929 LAX:0946/1010 FUL:1041 ANA:1049 SNA:1101 IRV:1112 SNC:1125 SNP:1138 OSD:1208 SOL:1223 OLT:1254 SAN:1307"),
    ("572", "LAX:1110 FUL:1141 ANA:1149 SNA:1201 IRV:1212 SNC:1225 SNP:1238 OSD:1308 SOL:1323 OLT:1354 SAN:1407"),
    ("774", "GTA:0831 SBA:0849 CPN:0904 VEC:0938 OXN:0955 CML:1009 MPK:1023 SIM:1039 CWT:1052 VNC:1107 BUR:1115 GDL:1126 LAX:1143/1210 FUL:1241 ANA:1249 SNA:1301 IRV:1312 SNC:1325 SNP:1338 OSD:1408 SOL:1423 OLT:1454 SAN:1507"),
    ("580", "LAX:1510 FUL:1541 ANA:1549 SNA:1601 IRV:1612 SNC:1625 SNP:1638 OSD:1708 SOL:1723 OLT:1754 SAN:1807"),
    ("782", "GTA:1229 SBA:1242/1245 CPN:1305 VEC:1332 OXN:1348 CML:1401 MPK:1415 SIM:1431 CWT:1446 NRG:1453 VNC:1506 BUR:1514 BBK:1520 GDL:1528 LAX:1545/1610 FUL:1641 ANA:1649 SNA:1701 IRV:1712 SNC:1725 SNP:1738 OSD:1808 SOL:1823 OLT:1854 SAN:1907"),
    ("584", "LAX:1710 FUL:1741 ANA:1749 SNA:1801 IRV:1812 SNC:1825 SNP:1838 OSD:1908 SOL:1923 OLT:1954 SAN:2007"),
    ("786", "GTA:1447 SBA:1504 CPN:1519 VEC:1543 OXN:1602 CML:1614 MPK:1625 SIM:1641 CWT:1654 VNC:1713 BUR:1721 GDL:1734 LAX:1750/1810 FUL:1841 ANA:1849 SNA:1901 IRV:1912 SNC:1925 SNP:1938 OSD:2008 SOL:2023 OLT:2054 SAN:2107"),
    ("588", "LAX:1910 FUL:1941 ANA:1949 SNA:2001 IRV:2012 SNC:2025 SNP:2038 OSD:2108 SOL:2123 OLT:2154 SAN:2207"),
    ("790", "GTA:1625 SBA:1637/1640 CPN:1657 VEC:1726 OXN:1742 CML:1753 MPK:1806 SIM:1824 CWT:1840 VNC:1853 BUR:1908 GDL:1920 LAX:1943/2010 FUL:2041 ANA:2049 SNA:2101 IRV:2112 SNC:2125 SNP:2138 OSD:2205 SOL:2223 OLT:2253 SAN:2307"),
    ("794", "GTA:1838 SBA:1857/1900 CPN:1916 VEC:1944 OXN:2003 CML:2014 MPK:2024 SIM:2041 CWT:2058 VNC:2109 BUR:2117 GDL:2128 LAX:2146/2210 FUL:2241 ANA:2249 SNA:2301 IRV:2312 SNC:2326 OSD:2405 SOL:2420 OLT:2451 SAN:2504"),
    ("757", "LAX:0513 GDL:0526 BBK:0533 BUR:0539 VNC:0547 NRG:0555 CWT:0602 SIM:0614 MPK:0631 CML:0643 OXN:0657 VEC:0711 CPN:0737 SBA:0756/0759 GTA:0815"),
    ("761", "SAN:0401 OLT:0410 SOL:0440 OSD:0457 SNC:0534 IRV:0549 SNA:0601 ANA:0610 FUL:0618 LAX:0657/0713 GDL:0726 BBK:0733 BUR:0739 VNC:0747 NRG:0755 CWT:0802 SIM:0814 MPK:0827 CML:0841 OXN:0856 VEC:0910 CPN:0936 SBA:0955/0958 GTA:1009"),
    ("765", "SAN:0601 OLT:0610 SOL:0640 OSD:0657 SNP:0720 SNC:0737 IRV:0752 SNA:0804 ANA:0813 FUL:0821 LAX:0857/0913 GDL:0926 BUR:0936 VNC:0944 CWT:0959 SIM:1011 MPK:1025 CML:1039 OXN:1050 VEC:1104 CPN:1130 SBA:1147/1150 GTA:1203"),
    ("567", "SAN:0701 OLT:0710 SOL:0740 OSD:0757 SNP:0820 SNC:0837 IRV:0852 SNA:0904 ANA:0913 FUL:0921 LAX:0957"),
    ("769", "SAN:0801 OLT:0810 SOL:0840 OSD:0857 SNP:0920 SNC:0937 IRV:0952 SNA:1004 ANA:1013 FUL:1021 LAX:1057/1113 GDL:1126 BUR:1136 VNC:1144 CWT:1159 SIM:1211 MPK:1223 CML:1236 OXN:1251 VEC:1305 CPN:1331 SBA:1346/1349 GTA:1402"),
    ("573", "SAN:1001 OLT:1010 SOL:1040 OSD:1057 SNP:1120 SNC:1137 IRV:1152 SNA:1204 ANA:1213 FUL:1221 LAX:1257"),
    ("777", "SAN:1201 OLT:1210 SOL:1240 OSD:1257 SNP:1320 SNC:1337 IRV:1352 SNA:1404 ANA:1413 FUL:1421 LAX:1457/1513 GDL:1526 BBK:1533 BUR:1539 VNC:1547 NRG:1555 CWT:1601 SIM:1613 MPK:1632 CML:1647 OXN:1658 VEC:1712 CPN:1733 SBA:1745/1748 GTA:1802"),
    ("579", "SAN:1301 OLT:1310 SOL:1342 OSD:1357 SNP:1420 SNC:1437 IRV:1452 SNA:1504 ANA:1513 FUL:1521 LAX:1557"),
    ("581", "SAN:1401 OLT:1410 SOL:1440 OSD:1457 SNP:1520 SNC:1537 IRV:1552 SNA:1604 ANA:1613 FUL:1621 LAX:1657"),
    ("785", "SAN:1601 OLT:1610 SOL:1640 OSD:1657 SNP:1720 SNC:1737 IRV:1752 SNA:1804 ANA:1813 FUL:1821 LAX:1857/1913 GDL:1926 BUR:1936 VNC:1944 CWT:1959 SIM:2011 MPK:2025 CML:2047 OXN:2059 VEC:2113 CPN:2135 SBA:2156/2159 GTA:2212"),
    ("587", "SAN:1701 OLT:1710 SOL:1740 OSD:1757 SNP:1823 SNC:1836 IRV:1853 SNA:1906 ANA:1917 FUL:1925 LAX:2004"),
    ("591", "SAN:1901 OLT:1910 SOL:1940 OSD:1957 SNP:2020 SNC:2037 IRV:2052 SNA:2104 ANA:2113 FUL:2121 LAX:2157"),
    ("593", "SAN:2001 OLT:2010 SOL:2040 OSD:2057 SNP:2120 SNC:2137 IRV:2152 SNA:2204 ANA:2213 FUL:2221 LAX:2257"),
    ("595", "SAN:2101 OLT:2110 SOL:2140 OSD:2157 SNC:2234 IRV:2249 SNA:2301 ANA:2310 FUL:2318 LAX:2357"),
];

// Yellow bridge columns only. The PDF prints an LPS time for 774 but
// explicitly says there is no bus service to Lompoc or Goleta. Do not
// synthesize an LPS stop; accept an existing otherwise matching bus with
// the printed LPS call rather than deleting it. No GTA bridge bus calls.
const BRIDGES: &[(&str, &str)] = &[
    ("757", "SBA:0759 GUA:0955 GVB:1025 SLO:1053"),
    ("765", "SBA:1150 GUA:1339 GVB:1409 SLO:1437"),
    ("777", "SBA:1748 GUA:1954 GVB:2024 SLO:2052"),
    ("774", "SLO:0611 GVB:0636 GUA:0656 SBA:0839"),
    ("786", "SLO:1212 GVB:1237 GUA:1257 SBA:1447"),
    ("794", "SLO:1617 GVB:1642 GUA:1702 SBA:1850"),
];

fn legs() -> Vec<Leg> {
    TRAINS.iter().map(|&(number, spec)| Leg { number, bus: false, calls: calls(spec) })
        .chain(BRIDGES.iter().map(|&(number, spec)| Leg { number, bus: true, calls: calls(spec) }))
        .collect()
}

fn seconds(t: &str) -> Option<i64> {
    let mut it = t.split(':');
    let h: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let s: i64 = it.next()?.parse().ok()?;
    if h < 0 || !(0..60).contains(&m) || !(0..60).contains(&s) || it.next().is_some() { return None; }
    Some(h * 3600 + m * 60 + s)
}
fn clock(s: i64) -> String { format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) }
fn event(row: &Row, arrival: bool) -> Option<i64> {
    seconds(value(row, if arrival { "arrival_time" } else { "departure_time" }))
}

fn rail(route: &Row) -> bool {
    let t = value(route, "route_type").parse::<u16>().unwrap_or(0);
    (t == 2 || (100..=117).contains(&t))
        && value(route, "route_long_name").to_ascii_lowercase().contains("pacific surfliner")
}
fn bus(route: &Row) -> bool {
    let t = value(route, "route_type").parse::<u16>().unwrap_or(0);
    t == 3 || (700..=799).contains(&t)
}

struct Feed {
    routes: Table,
    trips: Table,
    times: Table,
    calendar: Table,
    dates: Table,
    stops: HashMap<String, Row>,
    rail_routes: HashSet<String>,
    bus_routes: HashSet<String>,
    rail_route_id: String,
    agency_id: String,
}
impl Feed {
    fn read(root: &Path) -> Result<Self> {
        let routes = Table::read(root, "routes.txt")?;
        let mut rail_routes: Vec<_> = routes.rows.iter().filter(|r| rail(r)).collect();
        rail_routes.sort_by(|a, b| value(a, "route_id").cmp(value(b, "route_id")));
        let base = rail_routes.first().ok_or("Pacific Surfliner rail route not found")?;
        let rail_route_id = value(base, "route_id").to_owned();
        let agency_id = value(base, "agency_id").to_owned();
        let agencies = Table::read(root, "agency.txt")?;
        let agency = agencies.rows.iter().find(|r| value(r, "agency_id") == agency_id)
            .or_else(|| if agency_id.is_empty() && agencies.rows.len() == 1 { agencies.rows.first() } else { None })
            .ok_or("Pacific Surfliner agency not found")?;
        if value(agency, "agency_timezone") != "America/New_York" {
            return Err("Surfliner patch expects Amtrak's America/New_York GTFS agency clock".into());
        }
        let bus_routes = routes.rows.iter().filter(|r| bus(r) && value(r, "agency_id") == agency_id)
            .map(|r| value(r, "route_id").to_owned()).collect();
        let rail_routes = rail_routes.iter().map(|r| value(r, "route_id").to_owned()).collect();
        Ok(Self {
            routes, rail_routes, bus_routes, rail_route_id, agency_id,
            trips: Table::read(root, "trips.txt")?,
            times: Table::read(root, "stop_times.txt")?,
            calendar: Table::read(root, "calendar.txt")?,
            dates: Table::dates(root)?,
            stops: Table::read(root, "stops.txt")?.rows.into_iter()
                .map(|r| (value(&r, "stop_id").to_owned(), r)).collect(),
        })
    }
}

struct Index<'a> {
    times: HashMap<&'a str, Vec<&'a Row>>,
    calendars: HashMap<&'a str, &'a Row>,
    exceptions: HashMap<(&'a str, &'a str), &'a str>,
}
impl<'a> Index<'a> {
    fn new(feed: &'a Feed) -> Self {
        let mut times: HashMap<&str, Vec<&Row>> = HashMap::new();
        for row in &feed.times.rows {
            times.entry(value(row, "trip_id")).or_default().push(row);
        }
        for rows in times.values_mut() {
            rows.sort_by_key(|r| value(r, "stop_sequence").parse::<u32>().unwrap_or(u32::MAX));
        }
        Self {
            times,
            calendars: feed.calendar.rows.iter().map(|r| (value(r, "service_id"), r)).collect(),
            exceptions: feed.dates.rows.iter().map(|r| ((value(r, "service_id"), value(r, "date")), value(r, "exception_type"))).collect(),
        }
    }
    fn active(&self, trip: &Row, day: Date) -> bool {
        let service = value(trip, "service_id");
        let day_key = key(day);
        if let Some(kind) = self.exceptions.get(&(service, day_key.as_str())) {
            return *kind == "1";
        }
        let Some(c) = self.calendars.get(service) else { return false; };
        let weekday = match day.weekday() {
            Weekday::Monday => "monday", Weekday::Tuesday => "tuesday",
            Weekday::Wednesday => "wednesday", Weekday::Thursday => "thursday",
            Weekday::Friday => "friday", Weekday::Saturday => "saturday",
            Weekday::Sunday => "sunday",
        };
        day_key.as_str() >= value(c, "start_date") && day_key.as_str() <= value(c, "end_date") && value(c, weekday) == "1"
    }
    fn rows(&self, trip: &Row) -> &[&'a Row] {
        self.times.get(value(trip, "trip_id")).map(Vec::as_slice).unwrap_or(&[])
    }
}

// Return GTFS service-day displacement relative to the published Pacific
// date, supporting source variants using 00:xx instead of 24:xx.
fn matching_shift(leg: &Leg, rows: &[&Row]) -> Option<i64> {
    let filtered: Vec<_> = rows.iter().copied().filter(|r| {
        !(leg.bus && leg.number == "774" && value(r, "stop_id") == "LPS")
    }).collect();
    let n = leg.calls.len();
    if filtered.len() < n || (!leg.bus && filtered.len() != n) { return None; }
    for window in filtered.windows(n) {
        let Some(start) = event(window[0], false) else { continue; };
        let delta = leg.calls[0].departure - start;
        if delta % 86400 != 0 { continue; }
        let matches = window.iter().zip(&leg.calls).enumerate().all(|(i, (row, call))| {
            let last = i + 1 == n;
            value(row, "stop_id") == call.stop
                && event(row, last).is_some_and(|s| s + delta == if last { call.arrival } else { call.departure })
                && (!call.explicit_arrival || event(row, true).is_some_and(|s| s + delta == call.arrival))
                // Explicitly prohibited pickup/dropoff cannot represent the
                // bridge transfer even if the stop-time clock matches.
                && (i != 0 || value(row, "pickup_type") != "1")
                && (!last || value(row, "drop_off_type") != "1")
        });
        if matches { return Some(delta / 86400); }
    }
    None
}

fn local_service_shift(rows: &[&Row]) -> i64 {
    -rows.first().and_then(|r| event(r, false)).map(|s| (s - 3 * 3600).div_euclid(86400)).unwrap_or(0)
}

fn candidates<'a>(feed: &'a Feed, leg: &Leg) -> Vec<&'a Row> {
    feed.trips.rows.iter().filter(|trip| {
        let route = value(trip, "route_id");
        if leg.bus { feed.bus_routes.contains(route) }
        else { feed.rail_routes.contains(route) && value(trip, "trip_short_name") == leg.number }
    }).collect()
}

type Changes = BTreeMap<String, BTreeMap<String, &'static str>>;
fn add_change(changes: &mut Changes, trip: &Row, day: Date, kind: &'static str) {
    changes.entry(value(trip, "trip_id").to_owned()).or_default().insert(key(day), kind);
}

fn dates_between(start: Date, end: Date) -> impl Iterator<Item = Date> {
    std::iter::successors(Some(start), move |d| (*d < end).then(|| shift(*d, 1)))
}

// Check each published trip independently, preserving correct active
// upstream trips. A rail variant that still runs north of Goleta, or has
// the wrong published timing, is disabled only on affected service dates.
pub fn apply(root: &Path) -> Result<()> {
    let mut feed = Feed::read(root)?;
    let mut changes = Changes::new();
    let mut missing: Vec<(Leg, Vec<String>, Option<String>)> = vec![];
    {
        let index = Index::new(&feed);
        for leg in legs() {
            let candidates = candidates(&feed, &leg);
            let exact: Vec<_> = candidates.iter().filter_map(|trip| {
                matching_shift(&leg, index.rows(trip)).map(|s| (*trip, s))
            }).collect();
            let mut missing_days = vec![];
            for day in dates_between(first_day(), last_day()) {
                let active_exact: Vec<_> = exact.iter().filter(|(t, s)| index.active(t, shift(day, *s))).collect();
                if leg.bus && !active_exact.is_empty() { continue; }
                // Prefer an active matching upstream rail variant; otherwise
                // reactivate a matching inactive variant before synthesizing.
                let selected = active_exact.first().map(|entry| **entry)
                    .or_else(|| exact.first().copied());
                if let Some((trip, s)) = selected {
                    if !index.active(trip, shift(day, s)) {
                        add_change(&mut changes, trip, shift(day, s), "1");
                    }
                } else {
                    missing_days.push(key(day));
                }
                if !leg.bus {
                    for trip in &candidates {
                        if selected.is_some_and(|(t, _)| value(t, "trip_id") == value(trip, "trip_id")) { continue; }
                        let service_day = shift(day, local_service_shift(index.rows(trip)));
                        if index.active(trip, service_day) {
                            add_change(&mut changes, trip, service_day, "2");
                        }
                    }
                }
            }
            if !missing_days.is_empty() {
                let template_route = if leg.bus { None } else {
                    candidates.first().map(|r| value(r, "route_id").to_owned())
                };
                missing.push((leg, missing_days, template_route));
            }
        }
    }
    if changes.is_empty() && missing.is_empty() {
        println!("Pacific Surfliner emergency timetable already matches; no override needed");
        return Ok(());
    }

    // Validate all required stop IDs before writing anything.
    for (leg, _, _) in &missing {
        for call in &leg.calls {
            if !feed.stops.contains_key(call.stop) {
                return Err(format!("Missing Surfliner stop {}", call.stop).into());
            }
        }
    }

    // Clone each altered trip's original calendar and exceptions into a
    // private service. Never edit a shared service_id, since unrelated trains
    // may use it. Dates outside the patch retain their original availability.
    let mut occupied_services: HashSet<String> = feed.calendar.rows.iter().chain(&feed.dates.rows)
        .map(|r| value(r, "service_id").to_owned()).chain(feed.trips.rows.iter().map(|r| value(r, "service_id").to_owned())).collect();
    for (trip_id, edits) in changes {
        let trip = feed.trips.rows.iter_mut().find(|t| value(t, "trip_id") == trip_id).unwrap();
        let old = value(trip, "service_id").to_owned();
        let service = unique(&format!("{PREFIX}-service-{trip_id}"), &mut occupied_services);
        if let Some(original) = feed.calendar.rows.iter().find(|r| value(r, "service_id") == old) {
            let mut row = original.clone();
            set(&mut row, "service_id", &service);
            feed.calendar.rows.push(row);
        }
        let mut exceptions: BTreeMap<String, String> = feed.dates.rows.iter()
            .filter(|r| value(r, "service_id") == old)
            .map(|r| (value(r, "date").to_owned(), value(r, "exception_type").to_owned())).collect();
        for (day, kind) in edits { exceptions.insert(day, kind.to_owned()); }
        for (day, kind) in exceptions { add_exception(&mut feed.dates, &service, &day, &kind); }
        set(trip, "service_id", &service);
        println!("Surfliner: corrected service dates for {trip_id}");
    }

    if missing.iter().any(|(leg, _, _)| leg.bus) && !feed.bus_routes.contains(BRIDGE_ROUTE) {
        if feed.routes.rows.iter().any(|r| value(r, "route_id") == BRIDGE_ROUTE) {
            return Err("Surfliner bridge route ID collision".into());
        }
        let mut route = Row::new();
        set(&mut route, "route_id", BRIDGE_ROUTE);
        set(&mut route, "agency_id", &feed.agency_id);
        set(&mut route, "route_short_name", "Surfliner Bus");
        set(&mut route, "route_long_name", "Pacific Surfliner Bus Bridge");
        set(&mut route, "route_type", 3);
        feed.routes.rows.push(route);
    }

    let mut trip_ids: HashSet<String> = feed.trips.rows.iter().map(|r| value(r, "trip_id").to_owned()).collect();
    for (leg, days, template_route) in missing {
        let mode = if leg.bus { "bus" } else { "rail" };
        let id = unique(&format!("{PREFIX}-{mode}-{}", leg.number), &mut trip_ids);
        let service = unique(&format!("{id}-service"), &mut occupied_services);
        // Exception-only service is valid GTFS, but include a zero-weekday
        // calendar row for consumers expecting every service in calendar.txt.
        let mut cal = Row::new();
        set(&mut cal, "service_id", &service);
        set(&mut cal, "start_date", days.first().unwrap());
        set(&mut cal, "end_date", days.last().unwrap());
        for d in ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"] {
            set(&mut cal, d, 0);
        }
        feed.calendar.rows.push(cal);
        for day in &days { add_exception(&mut feed.dates, &service, day, "1"); }

        let mut trip = Row::new();
        let route = if leg.bus { BRIDGE_ROUTE } else { template_route.as_deref().unwrap_or(&feed.rail_route_id) };
        set(&mut trip, "route_id", route);
        set(&mut trip, "service_id", &service);
        set(&mut trip, "trip_id", &id);
        let short_name = if leg.bus { format!("{}-bus", leg.number) } else { leg.number.to_owned() };
        set(&mut trip, "trip_short_name", short_name);
        let last = leg.calls.last().unwrap().stop;
        let headsign = match last {
            "SAN" => "San Diego", "LAX" => "Los Angeles", "GTA" => "Goleta",
            "SBA" => "Santa Barbara", "SLO" => "San Luis Obispo", _ => last,
        };
        set(&mut trip, "trip_headsign", headsign);
        // Do not copy shapes, block IDs or stale through-rail distances.
        feed.trips.rows.push(trip);
        for (i, call) in leg.calls.iter().enumerate() {
            let mut row = Row::new();
            set(&mut row, "trip_id", &id);
            set(&mut row, "stop_id", call.stop);
            set(&mut row, "stop_sequence", i + 1);
            set(&mut row, "arrival_time", clock(call.arrival));
            set(&mut row, "departure_time", clock(call.departure));
            set(&mut row, "pickup_type", if i + 1 == leg.calls.len() { 1 } else { 0 });
            set(&mut row, "drop_off_type", if i == 0 { 1 } else { 0 });
            set(&mut row, "timepoint", 1);
            feed.times.rows.push(row);
        }
        println!("Surfliner: added {mode} {} for {} missing dates", leg.number, days.len());
    }
    feed.calendar.write(root, "calendar.txt")?;
    feed.dates.write(root, "calendar_dates.txt")?;
    feed.routes.write(root, "routes.txt")?;
    feed.trips.write(root, "trips.txt")?;
    feed.times.write(root, "stop_times.txt")?;
    Ok(())
}

fn unique(base: &str, occupied: &mut HashSet<String>) -> String {
    let mut id = base.to_owned();
    let mut n = 1;
    while !occupied.insert(id.clone()) {
        n += 1;
        id = format!("{base}-{n}");
    }
    id
}
fn add_exception(dates: &mut Table, service: &str, day: &str, kind: &str) {
    let mut row = Row::new();
    set(&mut row, "service_id", service);
    set(&mut row, "date", day);
    set(&mut row, "exception_type", kind);
    dates.rows.push(row);
}

/// Verify the processed output for the requested dates (local Pacific dates).
#[cfg(test)]
pub fn verify(root: &Path, start: Date, end: Date) -> Result<()> {
    let feed = Feed::read(root)?;
    let index = Index::new(&feed);
    for leg in legs() {
        let candidates = candidates(&feed, &leg);
        let exact: Vec<_> = candidates.iter().filter_map(|t| matching_shift(&leg, index.rows(t)).map(|s| (*t, s))).collect();
        for day in dates_between(start, end) {
            let active = exact.iter().filter(|(t, s)| index.active(t, shift(day, *s))).count();
            if active == 0 || (!leg.bus && active != 1) {
                return Err(format!("Surfliner {} {} on {day}: expected one matching trip, found {active}", leg.number, if leg.bus { "bus" } else { "rail" }).into());
            }
            if !leg.bus {
                let all_active = candidates.iter().filter(|t| index.active(t, shift(day, local_service_shift(index.rows(t))))).count();
                if all_active != 1 {
                    return Err(format!("Surfliner {} on {day}: {all_active} active rail variants", leg.number).into());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timetable_has_all_daily_trains_and_separate_bridges() {
        assert_eq!(TRAINS.len(), 27);
        assert_eq!(BRIDGES.len(), 6);
        for leg in legs() {
            assert!(leg.calls.len() >= 2);
            for pair in leg.calls.windows(2) {
                assert!(pair[0].departure < pair[1].arrival, "nonmonotonic {}", leg.number);
            }
            assert!(leg.calls.iter().all(|c| c.arrival <= c.departure));
            if leg.bus { assert!(!leg.calls.iter().any(|c| c.stop == "GTA" || c.stop == "LPS")); }
            else { assert!(!leg.calls.iter().any(|c| ["SLO", "GVB", "GUA", "LPS"].contains(&c.stop))); }
        }
        let all = legs();
        let rail777 = all.iter().find(|l| l.number == "777" && !l.bus).unwrap();
        let bus777 = all.iter().find(|l| l.number == "777" && l.bus).unwrap();
        assert_eq!(clock(rail777.calls[0].departure), "15:01:00");
        assert_eq!(clock(bus777.calls[0].departure), "20:48:00");
        assert_eq!(clock(bus777.calls.last().unwrap().arrival), "23:52:00");
        let rail786 = all.iter().find(|l| l.number == "786" && !l.bus).unwrap();
        let bus786 = all.iter().find(|l| l.number == "786" && l.bus).unwrap();
        assert_eq!(clock(rail786.calls.iter().find(|c| c.stop == "SBA").unwrap().departure), "18:04:00");
        assert_eq!(clock(bus786.calls[0].departure), "15:12:00");
        assert_eq!(clock(bus786.calls.last().unwrap().arrival), "17:47:00");
        assert_eq!(clock(all.iter().find(|l| l.number == "794" && !l.bus).unwrap().calls.last().unwrap().arrival), "28:04:00");
    }

    #[test]
    fn repairs_missing_services_without_mutating_shared_calendars_and_is_idempotent() -> Result<()> {
        let root = std::env::temp_dir().join(format!("surfliner-fixture-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()));
        std::fs::create_dir_all(&root)?;
        let outcome = (|| -> Result<()> {
            let mut stops = String::from("stop_id,stop_name\n");
            let mut ids = HashSet::new();
            for leg in legs() {
                for c in leg.calls {
                    if ids.insert(c.stop) { stops.push_str(&format!("{},{}\n", c.stop, c.stop)); }
                }
            }
            std::fs::write(root.join("stops.txt"), stops)?;
            std::fs::write(root.join("agency.txt"), "agency_id,agency_timezone\nAMTK,America/New_York\n")?;
            std::fs::write(root.join("routes.txt"), "route_id,agency_id,route_short_name,route_long_name,route_type\nR,AMTK,,Pacific Surfliner,2\nOTHER,AMTK,,Other,2\n")?;
            std::fs::write(root.join("trips.txt"), "route_id,service_id,trip_id,trip_short_name,trip_headsign,shape_id\nR,shared,bad777,777,San Luis Obispo,old\nOTHER,shared,unrelated,42,Elsewhere,\nR,inactive,good786,786,San Diego,\n")?;
            std::fs::write(root.join("calendar.txt"), "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nshared,1,1,1,1,1,1,1,20261101,20261231\ninactive,0,0,0,0,0,0,0,20261101,20261231\n")?;
            std::fs::write(root.join("calendar_dates.txt"), "service_id,date,exception_type\nshared,20261209,2\n")?;
            let mut times = String::from("trip_id,stop_id,stop_sequence,arrival_time,departure_time,pickup_type,drop_off_type,timepoint\nbad777,SAN,1,15:01:00,15:01:00,0,1,1\nbad777,SLO,2,23:52:00,23:52:00,1,0,1\n");
            let good = legs().into_iter().find(|l| l.number == "786" && !l.bus).unwrap();
            for (i, c) in good.calls.iter().enumerate() {
                times.push_str(&format!("good786,{},{},{},{},0,0,1\n", c.stop, i + 1, clock(c.arrival), clock(c.departure)));
            }
            std::fs::write(root.join("stop_times.txt"), times)?;
            apply(&root)?;
            verify(&root, first_day(), last_day())?;
            let feed = Feed::read(&root)?;
            let index = Index::new(&feed);
            let find = |id| feed.trips.rows.iter().find(|t| value(t, "trip_id") == id).unwrap();
            assert_eq!(value(find("unrelated"), "service_id"), "shared");
            assert!(index.active(find("unrelated"), date(2026, 11, 23)));
            assert!(!index.active(find("bad777"), date(2026, 11, 23)));
            assert!(index.active(find("bad777"), date(2026, 12, 8)));
            assert!(!index.active(find("bad777"), date(2026, 12, 9)));
            assert!(index.active(find("good786"), date(2026, 11, 23)));
            assert!(!index.active(find("good786"), date(2026, 12, 8)));
            for t in &feed.trips.rows {
                if value(t, "trip_id").starts_with(PREFIX) {
                    assert!(!index.active(t, date(2026, 11, 15)));
                    assert!(!index.active(t, date(2026, 12, 8)));
                }
            }
            let files = ["routes.txt", "trips.txt", "stop_times.txt", "calendar.txt", "calendar_dates.txt"];
            let before = files.iter().map(|f| std::fs::read(root.join(f))).collect::<std::io::Result<Vec<_>>>()?;
            apply(&root)?;
            let after = files.iter().map(|f| std::fs::read(root.join(f))).collect::<std::io::Result<Vec<_>>>()?;
            assert_eq!(before, after, "already correct feed must not be rewritten");
            Ok(())
        })();
        std::fs::remove_dir_all(&root)?;
        outcome
    }
}
