use cached::proc_macro::once;
use maud::{html, Markup, DOCTYPE};
use rocket::{response::status::BadRequest, State};

use crate::logsdb::LogsDB;

/// # Leaderboard
///
/// # Errors
#[get("/leaderboard")]
#[once(time = 1800, result = true, sync_writes = true)]
pub async fn leaderboard(logsdb: &State<LogsDB>) -> Result<Markup, BadRequest<String>> {
    let mut staff_stats = logsdb.get_staff_stats().await.map_err(crate::bad_request)?;
    staff_stats.sort_by_cached_key(|stats| stats.all_bans + stats.all_kick + stats.all_warn);
    staff_stats.reverse();

    Ok(html!(
        (DOCTYPE)

        html lang="en" data-bs-theme="dark" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";

                meta property="og:title" content="Stoner Booth";
                meta property="og:description" content="Staff Ban Leaderboard";
                meta property="og:type" content="website";
                meta property="og:url" content="https://stonerbooth.com/leaderboard";
                meta property="og:image" content="https://api.vrchat.cloud/api/1/file/file_03796aa7-32f8-48ad-a8fe-f72aae939c4c/1/file";
                meta property="og:width" content="2000";
                meta property="og:height" content="1125";

                title { "Staff Ban Leaderboard" }

                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.2/dist/css/bootstrap.min.css";
                link rel="stylesheet" href="http://fonts.googleapis.com/css?family=Dosis";
            }

            body class="text-center" style="font-family: Dosis" {
                header class="position-absolute top-0 start-50 translate-middle-x" {
                    h1 { "Staff Leaderboard" }
                }

                main class="position-absolute top-50 start-50 translate-middle" {
                    a href="https://discord.stonerbooth.com" { "Got banned, aren't a child, and want a second chance? Join the Discord" }

                    div class="table-responsive" style="max-height: 88vh" {
                        table class="table table-striped table-bordered" {
                            thead {
                                tr {
                                    th scope="col" { "#" }
                                    th scope="col" { "%" }
                                    th scope="col" { "Display Name" }
                                    th scope="col" { "Bans" }
                                    th scope="col" { "24h" }
                                    th scope="col" { "Kicks" }
                                    th scope="col" { "24h" }
                                    th scope="col" { "Warns" }
                                    th scope="col" { "24h" }
                                    th scope="col" { "Total" }
                                }
                            }

                            @let total_total = staff_stats.iter().map(|s| s.all_bans + s.all_kick + s.all_warn).sum::<i32>();
                            tbody class="table-group-divider" {
                                @for (i, stats) in staff_stats.iter().enumerate() {
                                    @let all_total = stats.all_bans + stats.all_kick + stats.all_warn;
                                    @let percent = (f64::from(all_total) / f64::from(total_total)) * 100.0;
                                    @let style = match i {
                                        0 => "color: #d6af36; font-weight: bold",
                                        1 => "color: #a77044; font-weight: bold",
                                        2 => "color: #a7a7ad; font-weight: bold",
                                        _ => "color: grey",
                                    };

                                    tr {
                                        th style=(style) scope="row" { (i + 1) }
                                        td style=(style) { (format!("{percent:.1}")) }
                                        td style=(style) { (stats.username) }
                                        td style=(style) { (stats.all_bans) }
                                        td style=(style) { (stats.new_bans) }
                                        td style=(style) { (stats.all_kick) }
                                        td style=(style) { (stats.new_kick) }
                                        td style=(style) { (stats.all_warn) }
                                        td style=(style) { (stats.new_warn) }
                                        td style=(style) { (all_total) }
                                    }
                                }
                            }

                            tbody class="table-group-divider" {
                                tr {
                                    th scope="row" { "#" }
                                    td { "100.0" }
                                    td { "Total" }
                                    td { (staff_stats.iter().map(|s| s.all_bans).sum::<i32>()) }
                                    td { (staff_stats.iter().map(|s| s.new_bans).sum::<i32>()) }
                                    td { (staff_stats.iter().map(|s| s.all_kick).sum::<i32>()) }
                                    td { (staff_stats.iter().map(|s| s.new_kick).sum::<i32>()) }
                                    td { (staff_stats.iter().map(|s| s.all_warn).sum::<i32>()) }
                                    td { (staff_stats.iter().map(|s| s.new_warn).sum::<i32>()) }
                                    td { (total_total) }
                                }
                            }
                        }
                    }
                }

                script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.2/dist/js/bootstrap.bundle.min.js" {}
            }
        }
    ))
}
