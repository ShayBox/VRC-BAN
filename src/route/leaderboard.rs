use cached::proc_macro::once;
use chrono::Utc;
use indexmap::IndexMap;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use rocket::{response::status::BadRequest, State};
use vrc::{
    api_client::{ApiClient, ApiError, AuthenticatedVRC},
    query::{GroupAuditLogs, User},
};

use crate::Config;

#[allow(clippy::needless_pass_by_value)]
fn bad_request(error: ApiError) -> BadRequest<String> {
    BadRequest(error.to_string())
}

/// # Leaderboard
///
/// # Errors
///
/// # Panics
#[get("/leaderboard")]
#[once(time = 14_400, result = true, sync_writes = true)]
pub async fn leaderboard(
    config: &State<Config>,
    vrchat: &State<AuthenticatedVRC>,
) -> Result<Markup, BadRequest<String>> {
    let mut logs = Vec::new();
    let mut query = GroupAuditLogs {
        id:     config.group_id_audit.clone(),
        n:      Some(100),
        offset: Some(0),
    };

    loop {
        let audit_logs = vrchat.query(query.clone()).await.map_err(bad_request)?;
        if audit_logs.results.is_empty() {
            break; // total_count % 100 || logs.len !>= total_count
        }

        logs.extend(audit_logs.results);
        if logs.len() >= audit_logs.total_count as usize {
            break;
        }

        if let Some(offset) = query.offset {
            query.offset = Some(offset + 100);
        }
    }

    let mut logs_by_actor_id = IndexMap::new();

    for log in logs {
        match log.event_type.as_ref() {
            "group.user.ban" => {
                logs_by_actor_id
                    .entry(log.actor_id.clone())
                    .or_insert(Vec::new())
                    .push(log);
            }
            "group.user.unban" => {
                if let Some(logs) = logs_by_actor_id.get_mut(&log.actor_id) {
                    logs.retain(|log1| log1.target_id != log.target_id);
                }
            }
            _ => continue,
        }
    }

    logs_by_actor_id.sort_by(|_, logs1, _, logs2| logs2.len().cmp(&logs1.len()));

    #[allow(clippy::cast_possible_truncation)]
    Ok(html!(
        (DOCTYPE)

        html lang="en" data-bs-theme="dark" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";

                title { "Leaderboard | The Stoner Booth" }

                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.2/dist/css/bootstrap.min.css";
                link rel="stylesheet" href="http://fonts.googleapis.com/css?family=Dosis";
            }

            body class="text-center" style="font-family: Dosis" {
                header class="position-absolute top-0 start-50 translate-middle-x" {
                    h1 { "The Stoner Booth" }
                    h2 { "Staff Leaderboard" }
                }

                main class="position-absolute top-50 start-50 translate-middle" {
                    a href="https://discord.shaybox.com" { "Got banned and aren't a child? Join the Discord" }

                    table class="table table-striped table-bordered" {
                        thead {
                            tr {
                                th scope="col" { "#" }
                                th scope="col" { "%" }
                                th scope="col" { "Display Name" }
                                th scope="col" { "Bans" }
                            }
                        }

                        tbody class="table-group-divider" {
                            @let total = logs_by_actor_id.values().flatten().count() as u32;
                            @for (i, (actor_id, logs)) in logs_by_actor_id.into_iter().enumerate() {
                                @let bans = logs.len() as u32;
                                @let percent = (f64::from(bans) / f64::from(total)) * 100.0;
                                @let query = User{ id: actor_id.clone() };
                                @let user = vrchat.query(query).await.map_err(bad_request)?;
                                @let name = &user.as_user().base.display_name;
                                @let style = match i {
                                    0 => "color: #d6af36; font-weight: bold",
                                    1 => "color: #a77044; font-weight: bold",
                                    2 => "color: #a7a7ad; font-weight: bold",
                                    _ => "color: grey",
                                };

                                tr {
                                    th style=(style) scope="row" { (i + 1) }
                                    td style=(style) { (format!("{percent:.1}")) }
                                    td style=(style) { (name) }
                                    td style=(style) { (bans) }
                                }
                            }
                        }

                        tbody class="table-group-divider" {
                            tr {
                                th scope="row" { "#" }
                                td { "100.0" }
                                td { "Total" }
                                td { (total) }
                            }
                        }
                    }

                    p id="last" { (Utc::now().to_rfc3339()) }
                    p id="next" { "Updates every 4 hours" }
                }

                footer class="position-absolute bottom-0 start-50 translate-middle-x" {
                    p data-bs-toggle="tooltip" data-bs-placement="top" title="Queers" { "Cheers!" }
                }

                script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.2/dist/js/bootstrap.bundle.min.js" {}
                script {(PreEscaped("
                    const last = document.getElementById('last');
                    const next = document.getElementById('next');
                    const date = new Date(last.textContent);
                    const loop = () => {
                        const lastMs = new Date() - date;
                        const lastHrs = Math.floor((lastMs % 86400000) / 3600000);
                        const lastMins = Math.round(((lastMs % 86400000) % 3600000) / 60000);
                        const lastHours = lastHrs > 0 ? `${lastHrs}h ` : '';

                        const nextMs = 14400 * 1000 - lastMs;
                        const nextHrs = Math.floor((nextMs % 86400000) / 3600000);
                        const nextMins = Math.round(((nextMs % 86400000) % 3600000) / 60000);
                        const nextHours = nextHrs > 0 ? `${nextHrs}h ` : '';

                        last.innerHTML = `Last Update ${lastHours}${lastMins}m ago`;
                        next.innerHTML = `Next Update ${nextHours}${nextMins}m`;

                        if (nextMs <= 0) {
                            location.reload();
                        }
                    };

                    setInterval(loop, 1000 * 60);
                    loop();
                "))}
            }
        }
    ))
}
