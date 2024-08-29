use rocket::response::Redirect;

#[get("/")]
#[must_use]
pub fn root() -> Redirect {
    Redirect::temporary("/leaderboard")
}
