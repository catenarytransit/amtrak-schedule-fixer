# Amtrak GTFS feed fixer

## Changes applied to the feed

This tool downloads Amtrak’s official GTFS feed and applies the following modifications:

* Fixes the following stops:
  * `LBO` (Los Baños Memorial Hospital bus stop)
    * Previously it was located at an incorrect position in Flin Flon, Manitoba
  * `EWR` (Newark Airport)
    * Previously it was located at the P4 parking garage of Newark Airport rather than at the railway station
* Completely removes all services operated by
  * `MARC` - MARC is removed because the [Maryland Transit Administration feed](https://www.mta.maryland.gov/developer-resources) should be used instead
  * `Via Rail Canada`
    * The Via Rail Canada Maple Leaf route only includes the Canadian half
    * Meanwhile, the Amtrak Maple Leaf route includes both halves
* Completely removes the following routes:
  * `Capitol Corridor`
    * Capitol Corridor is removed because the [511.org GTFS feed](https://511.org/open-data/transit) should be used instead
* Removes broken shapes
  * `California Zephyr` and `Floridian` have their shapes removed, as they are known to be incorrect
  * All shapes with more than 0.1° coordinate jump between consecutive points are removed
  * The broken shapes are just removed from the trips; we do not currently remove them from `shapes.txt`
* Fixes Shore Line East
  * The color and name of the routes are corrected
  * All trips with train numbers starting with `9` are removed, as such trips do not exist in the [PDF timetable](https://shorelineeast.com/wp-content/uploads/2025/09/SLE-Oct.-5-website-Schedule_R2_09262025.pdf)

## Potentially broken trip logging

Checks trips for departure after midnight in non-Eastern timezones.

If a trip departs at:
- 00:00~01:00 in Central Time
- 00:00~02:00 in Mountain Time
- 00:00~03:00 in Pacific Time
then it is flagged.

Flagged trips are only logged; nothing is done to them.

## Other notes

* Gold Runner (formerly San Joaquins) is no longer included in the official Amtrak GTFS timetable. Instead, it should be retrieved from Trillium: `https://data.trilliumtransit.com/gtfs/sanjoaquins-ca-us/sanjoaquins-ca-us.zip`
  * See entry [`mdb-2295`](https://mobilitydatabase.org/feeds/gtfs/mdb-2295) in Mobility Database.

## TODO

* [ ] We need to understand how the realtime data for Maple Leaf interacts with the GTFS timetable.

## Usage

```bash
cargo run
```

Note that additional steps, such as running `pfaedle`, are recommended. See `.github/workflows/update.yml` for details. Pfaedle 

## Reproducible regression tests (without pfaedle)

Run `bash scripts/test-amtrak-no-pfaedle.sh` from any directory. The script runs
the fixer, which downloads the live official GTFS feed, applies corrections,
then runs the integration test and produces `amtrak-gtfs-tested-no-pfaedle.zip`.
No pfaedle installation or OSM download is necessary.

The GitHub Actions workflow `.github/workflows/test-without-pfaedle.yml`
runs on pull requests, pushes to main, manual dispatch, and reusable workflow
calls. It uploads the processed feed as a short-lived artifact.

### November 15–16, 2026 Los Angeles test

The integration test checks that the *processed* feed has active trips with:
- Train short name `2`, departing stop `LAX` at **22:00 PST on Sunday, November 15, 2026**.
- Train short name `1`, arriving at stop `LAX` at **05:35 PST on Monday, November 16, 2026**.

It interprets GTFS stop times relative to the Amtrak agency time zone
(`America/New_York`), checks calendar validity and `calendar_dates.txt`
exceptions, supports hour values over 24, and compares actual instants after
time-zone conversion using Jiff and its bundled IANA time-zone database.
The test is marked ignored for regular offline `cargo test` because it
requires a freshly processed GTFS feed; the script explicitly includes it.
The test intentionally fails if the published feed or corrected calendar does
not contain the requested services, rather than synthesizing departures.
