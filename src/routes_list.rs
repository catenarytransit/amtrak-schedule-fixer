use std::error::Error;

use serde::Deserialize;
use serde::Serialize;

pub const ROUTE_LIST_URL: &str = "https://www.amtrak.com/services/routes-list.json";

#[derive(Deserialize)]
pub struct ParentToThrowAWay {
    #[serde(rename = "RoutesList")]
    pub routes_list: Vec<AmtrakRouteInfo>,
}

#[derive(Deserialize)]
pub struct AmtrakRouteInfo {
    #[serde(rename = "routeCode")]
    pub route_code: String,
    #[serde(rename = "cityServed")]
    pub city_served: String,
    #[serde(rename = "routeName")]
    pub route_name: String,
}

pub async fn fetch_and_decode_routes(
    client: reqwest::Client,
) -> Result<Vec<AmtrakRouteInfo>, Box<dyn Error + Sync + Send>> {
    let response = client.get(ROUTE_LIST_URL).send().await?;

    let text_decode = response.text().await?;

    let data = serde_json::from_str::<ParentToThrowAWay>(&text_decode)?;

    Ok(data.routes_list)
}
