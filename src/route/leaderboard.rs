use std::collections::HashMap;

use cached::proc_macro::once;
use maud::{html, Markup, DOCTYPE};
use rocket::{response::status::BadRequest, State};
use vrc::{
    api_client::{ApiClient, ApiError, AuthenticatedVRC},
    id::User as UserID,
    model::GroupAuditLog,
    query::{GroupAuditLogs, User},
};

use crate::Config;

fn bad_request(error: ApiError) -> BadRequest<String> {
    BadRequest(error.to_string())
}

/// # Leaderboard
///
/// # Errors
///
/// # Panics
#[get("/leaderboard")]
#[once(time = 43_200, result = true, sync_writes = true)]
pub async fn leaderboard(
    config: &State<Config>,
    vrchat: &State<AuthenticatedVRC>,
) -> Result<Markup, BadRequest<String>> {
    let mut offset = 0;
    let mut logs = Vec::new();

    loop {
        let query = GroupAuditLogs {
            id:     config.group_id_audit.clone(),
            n:      Some(100),
            offset: Some(offset),
        };

        let audit_logs = vrchat.query(query).await.map_err(bad_request)?;

        logs.extend(audit_logs.results);

        if logs.len() >= audit_logs.total_count {
            break;
        }

        offset += 100;
    }

    let mut count_by_actor_id = logs
        .iter()
        .filter(|&log| log.event_type == "group.user.ban" || log.event_type == "group.user.unban")
        .cloned()
        .fold(
            // Group logs by actor and target IDs
            HashMap::<(UserID, Option<UserID>), Vec<GroupAuditLog>>::new(),
            |mut map, log| {
                let key = (log.actor_id.clone(), log.target_id.clone());
                map.entry(key).or_default().push(log);
                map
            },
        )
        .into_iter()
        .filter_map(|(_, logs)| {
            // Filter out groups that have both "ban" and "unban" events
            if logs.iter().any(|log| log.event_type == "group.user.ban")
                && logs.iter().any(|log| log.event_type == "group.user.unban")
            {
                None
            } else {
                Some(logs)
            }
        })
        .flatten()
        .fold(HashMap::<UserID, usize>::new(), |mut map, log| {
            // Count filtered logs per actor_id
            *map.entry(log.actor_id).or_insert(0) += 1;
            map
        })
        .into_iter()
        .collect::<Vec<_>>();

    count_by_actor_id.sort_by(|(_, count1), (_, count2)| count2.cmp(count1));

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

            body style="font-family: Dosis" {
                header class="position-absolute top-0 start-50 translate-middle-x" {
                    h1 class="text-center" { "The Stoner Booth" }
                    h2 class="text-center" { "Staff Leaderboard" }

                }

                main class="position-absolute top-50 start-50 translate-middle" {
                    table class="table table-striped table-bordered" {
                        thead {
                            tr {
                                th class="text-center" scope="col" { "#" }
                                th class="text-center" scope="col" { "Display Name" }
                                th class="text-center" scope="col" { "Bans" }
                            }
                        }

                        tbody class="table-group-divider" {
                            @for (i, (actor_id, count)) in count_by_actor_id.iter().enumerate() {
                                @let query = User{ id: actor_id.clone() };
                                @let user = vrchat.query(query).await.map_err(bad_request)?;
                                @let name = &user.as_user().base.display_name;

                                tr {
                                    th class="text-center" scope="row" { (i + 1) }
                                    td class="text-center" { (name) }
                                    td class="text-center" { (count) }
                                }
                            }
                        }
                    }

                    p class="text-center" { "Updates every 12 hours" }
                }

                footer class="position-absolute bottom-0 start-50 translate-middle-x" {
                    p data-bs-toggle="tooltip" data-bs-placement="top" title="Queers" { "Cheers!" }
                }

                script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.2/dist/js/bootstrap.bundle.min.js" {}
            }
        }
    ))
}
