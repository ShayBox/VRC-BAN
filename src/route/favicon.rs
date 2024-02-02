use cached::proc_macro::once;
use reqwest::{Client, Error};
use rocket::{response::status::BadRequest, State};

const FILE: &str = "file_c8b49c10-4ef9-4db8-9cf7-aabce8286a6e/1";

#[derive(Clone, Hash, Responder)]
#[response(status = 200, content_type = "image/png")]
pub struct Icon(Vec<u8>);

#[allow(clippy::needless_pass_by_value)]
fn bad_request(error: Error) -> BadRequest<String> {
    BadRequest(error.to_string())
}

#[get("/favicon.ico")]
#[once(time = 43_200, result = true, sync_writes = true)]
pub async fn favicon(client: &State<Client>) -> Result<Icon, BadRequest<String>> {
    let url = format!("https://api.vrchat.cloud/api/1/file/{FILE}");
    let response = client.get(url).send().await.map_err(bad_request)?;
    println!("   >> Favicon: {}", response.status());

    let bytes = response.bytes().await.map_err(bad_request)?;
    let icon = Icon(bytes.to_vec());

    Ok(icon)
}
