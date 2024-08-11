use cached::proc_macro::once;
use chrono::Utc;
use derive_config::DeriveJsonConfig;
use indexmap::IndexMap;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use rocket::{
    response::status::BadRequest, time::OffsetDateTime, tokio::sync::Mutex, Either, State,
};
use vrc::{
    api_client::{ApiClient, AuthenticatedVRC},
    id::User as UserID,
    query::{GroupAuditLogs, Pagination, User},
};

use crate::{AuditLogs, Config};

/// # Leaderboard
///
/// # Errors
#[get("/leaderboard")]
#[once(time = 14_400, result = true, sync_writes = true)]
pub async fn leaderboard(
    config: &State<Config>,
    audits: &State<Mutex<AuditLogs>>,
    vrchat: &State<AuthenticatedVRC>,
) -> Result<Markup, BadRequest<String>> {
    /* Keep fetching logs until we find logs we already have saved */
    let logs = {
        let mut audits = audits.lock().await;
        let mut query = GroupAuditLogs {
            id: config.group_id_audit.clone(),
            pagination: Pagination {
                limit: 100,
                offset: 0,
            },
        };

        loop {
            let audit_logs = vrchat
                .query(query.clone())
                .await
                .map_err(crate::bad_request)?;

            if audit_logs.results.is_empty() {
                break; // total_count % 100
            }

            let len = audits.0.len();
            audits.0.extend(audit_logs.results);

            if !audit_logs.has_next || audits.0.len() == len {
                break; // Last page or no unique entries
            }

            query.pagination.offset += 100;
        }

        audits.save().map_err(crate::bad_request)?;
        audits.0.clone()
    };

    /* Merge actors and count total actions */
    let mut all_logs_by_actor_id = IndexMap::new();
    for log in logs {
        match log.event_type.as_ref() {
            "group.user.ban" | "group.instance.kick" | "group.instance.warn" => {
                #[rustfmt::skip] // Merge alt accounts
                let id = match log.actor_id {
                    Either::Left(ref id) => &id.to_string(),
                    Either::Right(ref id) => id,
                };

                let id = match id.as_str() {
                    // ~WhiteBoy~ -> -WhiteBoy-
                    "usr_01a387da-e758-451f-96e5-e3a7282c7197" => {
                        "usr_71ddbbc1-c70f-4b4a-a0fc-e87f57038393"
                    }
                    // ZealWolf d978 -> Zeal Wolf
                    "usr_a4cec242-f798-4d53-aa69-b85e19e9d978" => {
                        "usr_275004c5-5532-47e6-a543-2ebf88229bdf"
                    }
                    // TheVoiceBox | FemBox -> ShayBox
                    "usr_5dc9c86d-2de7-4c10-b11d-8dd1335270de"
                    | "usr_98139f06-9b7e-4a2c-b7b0-8459b51dddbb" => {
                        "usr_2e8e2b0c-df4e-499f-bbf0-ddc5f3841488"
                    }
                    "vrc_admin" => "Vote Kick",
                    id => id,
                };

                all_logs_by_actor_id
                    .entry(Into::<UserID>::into(id))
                    .or_insert(Vec::new())
                    .push(log);
            }
            "group.user.unban" => {
                let actor_id = &log.actor_id.left().expect("Incorrect event type");
                if let Some(logs) = all_logs_by_actor_id.get_mut(actor_id) {
                    logs.retain(|log1| log1.target_id != log.target_id);
                }
            }
            _ => continue,
        }
    }

    /* List new logs within the last 24 hours */
    let mut new_logs_by_actor_id = all_logs_by_actor_id.clone();
    for logs in new_logs_by_actor_id.values_mut() {
        logs.retain(|log| (OffsetDateTime::now_utc() - log.created_at).whole_hours() <= 24);
    }

    /* Sort the leaderboard */
    all_logs_by_actor_id.sort_by(|_, logs1, _, logs2| logs2.len().cmp(&logs1.len()));

    #[allow(clippy::cast_possible_truncation)]
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

                title { "Leaderboard | Stoner Booth" }

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

                            tbody class="table-group-divider" {
                                @let [ // What did I just write...
                                    [all_bans, all_kicks, all_warns],
                                    [new_bans, new_kicks, new_warns]
                                ] = [&all_logs_by_actor_id, &new_logs_by_actor_id].map(|logs| {
                                    [
                                        "group.user.ban",
                                        "group.instance.kick",
                                        "group.instance.warn"
                                    ].map(|event_type| logs.values().flatten().filter(|log| log.event_type == event_type).count() as u32)
                                });
                                @let total_total = all_bans + all_kicks + all_warns;
                                @for (i, (actor_id, logs)) in all_logs_by_actor_id.into_iter().enumerate() {
                                    @let name = vrchat.query(User{ id: actor_id.clone() }).await.map_err(crate::bad_request).map_or_else(|_| actor_id.to_string(), |user| user.as_user().base.display_name.clone());
                                    @let [bans, kicks, warns] = [
                                        "group.user.ban",
                                        "group.instance.kick",
                                        "group.instance.warn"
                                    ].map(|event_type| logs.iter().filter(|log| log.event_type == event_type).count() as u32);
                                    @let [new_bans, new_kicks, new_warns] = [
                                        "group.user.ban",
                                        "group.instance.kick",
                                        "group.instance.warn"
                                    ].map(|event_type| new_logs_by_actor_id.get(&actor_id).map_or(0, |logs| logs.iter().filter(|log| log.event_type == event_type).count()));
                                    @let total = bans + kicks + warns;
                                    @let percent = (f64::from(total - warns) / f64::from(total_total - all_warns)) * 100.0;
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
                                        td style=(style) { (new_bans) }
                                        td style=(style) { (kicks) }
                                        td style=(style) { (new_kicks) }
                                        td style=(style) { (warns) }
                                        td style=(style) { (new_warns) }
                                        td style=(style) { (total) }
                                    }
                                }
                            }

                            tbody class="table-group-divider" {
                                tr {
                                    th scope="row" { "#" }
                                    td { "100.0" }
                                    td { "Total" }
                                    td { (all_bans) }
                                    td { (new_bans) }
                                    td { (all_kicks) }
                                    td { (new_kicks) }
                                    td { (all_warns) }
                                    td { (new_warns) }
                                    td { (total_total) }
                                }
                            }
                        }
                    }
                }

                footer class="position-absolute bottom-0 start-50 translate-middle-x" {
                    p data-bs-toggle="tooltip" data-bs-placement="top" title="Updates every four hours" id="last" { (Utc::now().to_rfc3339()) }
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

                        last.innerHTML = `Last Update ${lastHours}${lastMins}m ago  |  Next Update in ${nextHours}${nextMins}m`;

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
