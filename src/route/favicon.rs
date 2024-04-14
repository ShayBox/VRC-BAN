use cached::proc_macro::once;
use reqwest::Client;
use rocket::{response::status::BadRequest, State};

const FILE: &str = "file_458fd3ea-d81a-4ab4-8626-56c860dda723/1";

#[derive(Clone, Hash, Responder)]
#[response(status = 200, content_type = "image/png")]
pub struct Icon(Vec<u8>);

#[get("/favicon.ico")]
#[once(result = true, sync_writes = true)]
pub async fn favicon(client: &State<Client>) -> Result<Icon, BadRequest<String>> {
    let url = format!("https://api.vrchat.cloud/api/1/file/{FILE}");
    let response = client.get(url).send().await.map_err(crate::bad_request)?;
    let bytes = response.bytes().await.map_err(crate::bad_request)?;
    let icon = Icon(bytes.to_vec());

    Ok(icon)
}
