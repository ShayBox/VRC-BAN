use cached::proc_macro::once;
use rocket::State;
use vrc::api_client::AuthenticatedVRC;

const _FILE: &str = "file_c8b49c10-4ef9-4db8-9cf7-aabce8286a6e";

#[derive(Clone, Hash, Responder)]
#[response(status = 200, content_type = "image/x-icon")]
pub struct Icon(Vec<u8>);

#[once]
#[get("/favicon.ico")]
pub fn favicon(_vrchat: &State<AuthenticatedVRC>) -> Icon {
    // TODO

    Icon(vec![])
}
