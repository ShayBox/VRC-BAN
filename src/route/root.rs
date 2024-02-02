use rocket::response::Redirect;

#[get("/")]
pub fn root() -> Redirect {
    Redirect::temporary("/leaderboard")
}
