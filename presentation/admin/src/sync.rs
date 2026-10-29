use crate::AdminAction;
use crate::SYNC_PATH;
use crate::admin_page;
use crate::components::alert::AlertVariant;
use crate::components::alert::alert;
use crate::components::alert::alert_description;
use crate::components::alert::alert_title;
use crate::components::alert_dialog::alert_dialog;
use crate::components::badge::BadgeVariant;
use crate::components::badge::badge;
use crate::components::button::ButtonSize;
use crate::components::button::ButtonVariant;
use crate::components::button::button;
use crate::components::card::card;
use crate::components::card::card_content;
use crate::components::card::card_description;
use crate::components::card::card_header;
use crate::components::card::card_title;
use crate::components::dialog::dialog;
use crate::components::dialog::dialog_content;
use crate::components::dialog::dialog_description;
use crate::components::dialog::dialog_footer;
use crate::components::dialog::dialog_header;
use crate::components::dialog::dialog_title;
use crate::components::progress::progress;
use crate::components::table::table;
use crate::components::table::table_body;
use crate::components::table::table_cell;
use crate::components::table::table_head;
use crate::components::table::table_header;
use crate::components::table::table_row;
use crate::local_time::TimeFormat;
use crate::local_time::local_time;
use crate::log_admin_action;
use crate::require_admin;
use crate::verify_csrf;
use application::attributes::SELFLESS_ADMIN_ACTION;
use application::attributes::SELFLESS_LIBRARY_SOURCE;
use application::sync_status::ApiCall;
use application::sync_status::FailedRow;
use application::sync_status::Problem;
use application::sync_status::ProblemItem;
use application::sync_status::ReconcileProgress;
use application::sync_status::SourceStatus;
use application::sync_status::SyncStatus;
use application::sync_status::VisibleCounts;
use domain::ReconcileResult;
use domain::Source;
use domain::SyncProblemKind;
use infrastructure::AppState;
use jiff::SignedDuration;
use serde::Deserialize;
use std::error::Error;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::RouterBuilder;
use topcoat::router::content::Form;
use topcoat::router::error::not_found;
use topcoat::router::href;
use topcoat::router::page;
use topcoat::router::path_param;
use topcoat::view::View;
use topcoat::view::attributes;
use topcoat::view::component;
use topcoat::view::view;
use uuid::Uuid;

const SYNC_STATUS_ID: &str = "sync-status";
const SYNC_STATUS_TARGET: &str = "#sync-status";

path_param!(delivery_id: Uuid, error = not_found);
path_param!(source_name);

pub fn register(router: RouterBuilder) -> RouterBuilder {
    router
        .page(sync)
        .page(sources)
        .page(sync_now)
        .page(retry)
        .page(confirm_removal)
}

#[derive(Deserialize)]
struct SyncAction {
    #[serde(default)]
    csrf: String,
}

struct Notice {
    variant: AlertVariant,
    title: &'static str,
    description: String,
}

#[page("/admin/sync")]
async fn sync(cx: &Cx) -> topcoat::Result<impl View> {
    require_admin(cx).await?;
    let status = application::sync_status::call(app_context::<AppState>(cx)).await?;
    Ok(view! {
        admin_page(
            path: SYNC_PATH,
            title: "Sync status",
            sync_status(status: &status, notice: None, polling: false)
        )
    })
}

#[page("/admin/sync/sources")]
async fn sources(cx: &Cx) -> topcoat::Result<impl View> {
    require_admin(cx).await?;
    let status = application::sync_status::call(app_context::<AppState>(cx)).await?;
    Ok(view! { source_cards(status: &status, polling: false) })
}

#[page(POST "/admin/sync/start")]
async fn sync_now(cx: &Cx, Form(form): Form<SyncAction>) -> topcoat::Result<impl View> {
    verify_csrf(cx, &form.csrf)?;
    require_admin(cx).await?;
    let state = app_context::<AppState>(cx);
    application::sync_now::call(state);
    log_source_action(AdminAction::SyncStart, None);
    let status = application::sync_status::call(state).await?;
    let notice = Notice {
        variant: AlertVariant::Neutral,
        title: "Reconcile started",
        description: "Radarr and Sonarr are reconciling. The cards update until they finish."
            .to_owned(),
    };
    Ok(view! { sync_status(status: &status, notice: Some(notice), polling: true) })
}

#[page(POST "/admin/sync/failed-deliveries/{delivery_id}/retry")]
async fn retry(cx: &Cx, Form(form): Form<SyncAction>) -> topcoat::Result<impl View> {
    verify_csrf(cx, &form.csrf)?;
    require_admin(cx).await?;
    let id = *path_param::<DeliveryId>(cx)?;
    let state = app_context::<AppState>(cx);
    let notice = match application::retry_failed_row::call(state, id).await {
        Ok(()) => {
            log_admin_action(AdminAction::SyncRetry, id);
            Notice {
                variant: AlertVariant::Neutral,
                title: "Retrying the delivery",
                description: "selfless processes it again now.".to_owned(),
            }
        }
        Err(error) => {
            tracing::error!(error = &error as &dyn Error, "failed to retry a delivery");
            Notice {
                variant: AlertVariant::Destructive,
                title: "Retry failed",
                description: "The delivery stays failed. Try again in a moment.".to_owned(),
            }
        }
    };
    let status = application::sync_status::call(state).await?;
    Ok(view! { sync_status(status: &status, notice: Some(notice), polling: false) })
}

#[page(POST "/admin/sync/{source_name}/confirm-removal")]
async fn confirm_removal(cx: &Cx, Form(form): Form<SyncAction>) -> topcoat::Result<impl View> {
    verify_csrf(cx, &form.csrf)?;
    require_admin(cx).await?;
    let source = match path_param::<SourceName>(cx) {
        "radarr" => Source::Radarr,
        "sonarr" => Source::Sonarr,
        _ => return Err(not_found().into()),
    };
    let state = app_context::<AppState>(cx);
    let label = source_label(source);
    let notice = match application::confirm_held_removal::call(state, source).await {
        Ok(()) => {
            log_source_action(AdminAction::SyncConfirmRemoval, Some(source));
            Notice {
                variant: AlertVariant::Neutral,
                title: "Removal confirmed",
                description: format!("The items {label} no longer lists are removed."),
            }
        }
        Err(error) => {
            tracing::error!(
                error = &error as &dyn Error,
                "failed to confirm a held removal"
            );
            Notice {
                variant: AlertVariant::Destructive,
                title: "Removal failed",
                description: format!(
                    "Nothing was removed. Check {label}'s API health and try again."
                ),
            }
        }
    };
    let status = application::sync_status::call(state).await?;
    Ok(view! { sync_status(status: &status, notice: Some(notice), polling: false) })
}

fn log_source_action(action: AdminAction, source: Option<Source>) {
    tracing::info!(
        { SELFLESS_ADMIN_ACTION } = action.name(),
        { SELFLESS_LIBRARY_SOURCE } = source.map(Source::name),
        "admin action"
    );
}

#[component]
async fn sync_status(
    status: &SyncStatus,
    notice: Option<Notice>,
    polling: bool,
) -> topcoat::Result<impl View> {
    Ok(view! {
        <div id=(SYNC_STATUS_ID) class="flex flex-col gap-6">
            <div>
                button(
                    attrs: attributes! {
                        type="button"
                        hx-post=(href!(sync_now))
                        hx-target=(SYNC_STATUS_TARGET)
                        hx-swap="outerHTML"
                    },
                    "Sync now"
                )
            </div>
            match notice {
                Some(notice) => alert(
                    variant: notice.variant,
                    attrs: attributes! {
                        role=(if notice.variant == AlertVariant::Destructive { "alert" } else { "status" })
                    },
                    alert_title((notice.title))
                    alert_description((notice.description))
                ),
                None => "",
            }
            source_cards(status: status, polling: polling)
            failed_deliveries(rows: &status.failed_rows)
            sync_problems(problems: &status.problems)
        </div>
    })
}

#[component]
async fn source_cards(status: &SyncStatus, polling: bool) -> topcoat::Result<impl View> {
    let polling = polling
        || status.radarr.reconcile_progress.is_some()
        || status.sonarr.reconcile_progress.is_some();
    Ok(view! {
        <div
            class="grid gap-6 md:grid-cols-2"
            if polling {
                hx-get=(href!(sources))
                hx-trigger="every 5s"
                hx-swap="outerHTML"
            }
        >
            source_card(source: Source::Radarr, status: &status.radarr)
            source_card(source: Source::Sonarr, status: &status.sonarr)
        </div>
    })
}

#[component]
async fn source_card(source: Source, status: &SourceStatus) -> topcoat::Result<impl View> {
    let label = source_label(source);
    Ok(view! {
        <section aria-label=(label)>
            card(
                attrs: attributes! { class="h-full" },
                card_header(
                    <div class="flex items-center justify-between gap-2">
                        card_title((label))
                        match &status.last_api_call {
                            Some(ApiCall::Succeeded) => badge(variant: BadgeVariant::Secondary, "API working"),
                            Some(ApiCall::Failed(_)) => badge(variant: BadgeVariant::Destructive, "API failing"),
                            None => badge(variant: BadgeVariant::Outline, "API not called yet"),
                        }
                    </div>
                    match &status.last_api_call {
                        Some(ApiCall::Failed(error)) => card_description(
                            attrs: attributes! { class="break-words" },
                            (format!("Last error: {error}"))
                        ),
                        _ => "",
                    }
                )
                card_content(
                    attrs: attributes! { class="flex flex-col gap-4" },
                    match &status.held_removal {
                        Some(held_removal) => held_removal_alert(
                            source: source,
                            items: source_items(source, held_removal.count),
                        ),
                        None => "",
                    }
                    if status.kodi_metadata == Some(false) {
                        alert(
                            alert_title("Kodi metadata is off")
                            alert_description(
                                (format!("Turn on the Kodi (XBMC) metadata in {label}'s metadata settings, so that it writes the NFO files selfless reads."))
                            )
                        )
                    }
                    match status.last_webhook_at {
                        Some(_) => "",
                        None => alert(
                            alert_title("No webhook received")
                            alert_description(
                                (format!("Check the selfless webhook under Connect in {label}'s settings."))
                            )
                        ),
                    }
                    match status.reconcile_progress {
                        Some(ReconcileProgress { imported, total: Some(total) }) => {
                            <div class="flex flex-col gap-2 text-sm">
                                <p>(format!("Reconcile running: imported {imported} of {total}"))</p>
                                progress(
                                    value: imported as f32,
                                    max: total as f32,
                                    attrs: attributes! { aria-label="Reconcile progress" },
                                )
                            </div>
                        },
                        Some(ReconcileProgress { total: None, .. }) => {
                            <div class="flex flex-col gap-2 text-sm">
                                <p>(format!("Reconcile running: reading {label}'s list"))</p>
                                progress(attrs: attributes! { aria-label="Reconcile progress" })
                            </div>
                        },
                        None => "",
                    }
                    <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-3 text-sm">
                        <dt class="text-muted-foreground">"Webhooks"</dt>
                        <dd class="flex flex-col gap-1">
                            match status.last_webhook_at {
                                Some(at) => <p>"Last webhook " local_time(at: at, format: TimeFormat::Relative)</p>,
                                None => "",
                            }
                            <p>
                                match status.rejected_webhooks {
                                    0 => "No rejected webhooks",
                                    1 => "1 rejected webhook",
                                    rejected => (format!("{rejected} rejected webhooks")),
                                }
                            </p>
                        </dd>
                        <dt class="text-muted-foreground">"Last reconcile"</dt>
                        <dd class="flex flex-col gap-1">
                            match &status.last_reconcile {
                                Some(last_reconcile) => {
                                    match last_reconcile.result {
                                        ReconcileResult::Succeeded => badge(variant: BadgeVariant::Secondary, "Succeeded"),
                                        ReconcileResult::Failed => badge(variant: BadgeVariant::Destructive, "Failed"),
                                    }
                                    <p>"Started " local_time(at: last_reconcile.started_at, format: TimeFormat::Absolute)</p>
                                    <p>(format!("Took {}", duration(last_reconcile.duration)))</p>
                                },
                                None => <p>"Not run yet"</p>,
                            }
                        </dd>
                        <dt class="text-muted-foreground">"Library"</dt>
                        <dd>
                            match status.visible {
                                VisibleCounts::Movies(movies) => (count(movies, "movie", "movies")),
                                VisibleCounts::Shows { shows, episodes } => (format!(
                                    "{}, {}",
                                    count(shows, "show", "shows"),
                                    count(episodes, "episode", "episodes"),
                                )),
                            }
                        </dd>
                    </dl>
                )
            )
        </section>
    })
}

#[component]
async fn held_removal_alert(source: Source, items: String) -> topcoat::Result<impl View> {
    let label = source_label(source);
    let dialog_id = format!("confirm-removal-{}", source.name());
    let title_id = format!("{dialog_id}-title");
    Ok(view! {
        alert(
            variant: AlertVariant::Destructive,
            alert_title("Removal held")
            alert_description(
                attrs: attributes! { class="flex flex-col items-start gap-2" },
                <p>(format!("{label} no longer lists {items}. selfless keeps them until you confirm the removal."))</p>
                button(
                    variant: ButtonVariant::Destructive,
                    size: ButtonSize::Sm,
                    attrs: attributes! {
                        type="button"
                        commandfor=(&dialog_id)
                        command="show-modal"
                    },
                    "Confirm removal"
                )
            )
        )
        alert_dialog(
            open: false,
            attrs: attributes! { id=(&dialog_id) aria-labelledby=(&title_id) },
            dialog_content(
                dialog_header(
                    dialog_title(attrs: attributes! { id=(&title_id) }, (format!("Remove {items}?")))
                    dialog_description("Their watch history goes too. This cannot be undone.")
                )
                dialog_footer(
                    button(
                        variant: ButtonVariant::Outline,
                        attrs: attributes! {
                            type="button"
                            commandfor=(&dialog_id)
                            command="close"
                        },
                        "Cancel"
                    )
                    button(
                        variant: ButtonVariant::Destructive,
                        attrs: attributes! {
                            type="button"
                            hx-post=(href!(confirm_removal, SourceName(source.name())))
                            hx-target=(SYNC_STATUS_TARGET)
                            hx-swap="outerHTML"
                        },
                        (format!("Remove {items}"))
                    )
                )
            )
        )
    })
}

#[component]
async fn failed_deliveries(rows: &[FailedRow]) -> topcoat::Result<impl View> {
    Ok(view! {
        <section aria-label="Failed deliveries" class="flex flex-col gap-3">
            <h2 class="text-sm font-semibold">"Failed deliveries"</h2>
            if rows.is_empty() {
                <p class="text-sm text-muted-foreground">"No failed deliveries"</p>
            } else {
                card(
                    table(
                        table_header(
                            table_row(
                                table_head("Source")
                                table_head("Event")
                                table_head("Error")
                                table_head("Attempts")
                                table_head("Received")
                                table_head(<span class="sr-only">"Actions"</span>)
                            )
                        )
                        table_body(
                            #[key(row.id.as_u128())]
                            for row in rows {
                                table_row(
                                    attrs: attributes! { class="relative" },
                                    table_cell((source_label(row.source)))
                                    table_cell(
                                        <button
                                            type="button"
                                            commandfor=(failed_delivery_dialog_id(row))
                                            command="show-modal"
                                            class="cursor-pointer font-medium underline-offset-4 outline-none after:absolute after:inset-0 hover:underline focus-visible:underline"
                                        >
                                            (event_label(row))
                                        </button>
                                    )
                                    table_cell(
                                        attrs: attributes! { class="max-w-sm truncate" },
                                        (&row.error)
                                    )
                                    table_cell((row.attempts.to_string()))
                                    table_cell(local_time(at: row.received_at, format: TimeFormat::Absolute))
                                    table_cell(
                                        attrs: attributes! { class="text-right" },
                                        button(
                                            variant: ButtonVariant::Outline,
                                            size: ButtonSize::Sm,
                                            attrs: attributes! {
                                                type="button"
                                                class="relative z-10"
                                                hx-post=(href!(retry, DeliveryId(row.id)))
                                                hx-target=(SYNC_STATUS_TARGET)
                                                hx-swap="outerHTML"
                                            },
                                            "Retry"
                                        )
                                    )
                                )
                            }
                        )
                    )
                )
                #[key(row.id.as_u128())]
                for row in rows {
                    failed_delivery_dialog(row: row)
                }
            }
        </section>
    })
}

#[component]
async fn failed_delivery_dialog(row: &FailedRow) -> topcoat::Result<impl View> {
    let dialog_id = failed_delivery_dialog_id(row);
    let title_id = format!("{dialog_id}-title");
    let body = serde_json::from_slice::<serde_json::Value>(&row.body)
        .and_then(|x| serde_json::to_string_pretty(&x))
        .unwrap_or_else(|_| String::from_utf8_lossy(&row.body).into_owned());
    Ok(view! {
        dialog(
            open: false,
            attrs: attributes! { id=(&dialog_id) aria-labelledby=(&title_id) },
            dialog_content(
                attrs: attributes! { class="[&]:max-w-3xl" },
                dialog_header(
                    dialog_title(
                        attrs: attributes! { id=(&title_id) },
                        (format!("{} from {}", event_label(row), source_label(row.source)))
                    )
                    dialog_description(
                        "Received " local_time(at: row.received_at, format: TimeFormat::Absolute)
                    )
                )
                <div class="flex flex-col gap-2">
                    <h3 class="text-sm font-medium">"Last error"</h3>
                    <pre class="rounded-md bg-foreground/5 p-3 font-mono text-xs break-words whitespace-pre-wrap">(&row.error)</pre>
                </div>
                <div class="flex flex-col gap-2">
                    <h3 class="text-sm font-medium">"Attempts"</h3>
                    table(
                        table_header(
                            table_row(
                                table_head("Time")
                                table_head("Error")
                            )
                        )
                        table_body(
                            #[key(index)]
                            for (index, attempt) in row.attempt_log.iter().enumerate() {
                                table_row(
                                    table_cell(local_time(at: attempt.attempted_at, format: TimeFormat::Absolute))
                                    table_cell(
                                        attrs: attributes! { class="break-words [&]:whitespace-pre-wrap" },
                                        match &attempt.error {
                                            Some(error) => (error),
                                            None => "No error",
                                        }
                                    )
                                )
                            }
                        )
                    )
                </div>
                <div class="flex flex-col gap-2">
                    <h3 class="text-sm font-medium">"Body"</h3>
                    <pre class="max-h-96 overflow-auto rounded-md bg-foreground/5 p-3 font-mono text-xs">(body)</pre>
                </div>
                dialog_footer(
                    button(
                        variant: ButtonVariant::Outline,
                        attrs: attributes! {
                            type="button"
                            commandfor=(&dialog_id)
                            command="close"
                        },
                        "Close"
                    )
                )
            )
        )
    })
}

#[component]
async fn sync_problems(problems: &[Problem]) -> topcoat::Result<impl View> {
    Ok(view! {
        <section aria-label="Items with problems" class="flex flex-col gap-3">
            <h2 class="text-sm font-semibold">"Items with problems"</h2>
            if problems.is_empty() {
                <p class="text-sm text-muted-foreground">"No items with problems"</p>
            } else {
                card(
                    table(
                        table_header(
                            table_row(
                                table_head("Kind")
                                table_head("Item")
                                table_head("Path")
                            )
                        )
                        table_body(
                            #[key(index)]
                            for (index, problem) in problems.iter().enumerate() {
                                table_row(
                                    table_cell((problem_kind(problem.kind)))
                                    table_cell((problem_item(&problem.item)))
                                    table_cell(
                                        attrs: attributes! { class="font-mono text-xs" },
                                        (&problem.path)
                                    )
                                )
                            }
                        )
                    )
                )
            }
        </section>
    })
}

fn source_label(source: Source) -> &'static str {
    match source {
        Source::Radarr => "Radarr",
        Source::Sonarr => "Sonarr",
    }
}

fn failed_delivery_dialog_id(row: &FailedRow) -> String {
    format!("failed-delivery-{}", row.id)
}

fn event_label(row: &FailedRow) -> &str {
    row.event_type.as_deref().unwrap_or("Unknown")
}

fn source_items(source: Source, items: usize) -> String {
    let items = items as u64;
    match source {
        Source::Radarr => count(items, "movie", "movies"),
        Source::Sonarr => count(items, "show", "shows"),
    }
}

fn problem_kind(kind: SyncProblemKind) -> &'static str {
    match kind {
        SyncProblemKind::UnreadableNfo => "Unreadable NFO",
        SyncProblemKind::FileNotFound => "File not found",
        SyncProblemKind::FfprobeFailed => "Unreadable file",
    }
}

fn problem_item(item: &ProblemItem) -> String {
    match item {
        ProblemItem::Movie { title, .. } | ProblemItem::Show { title, .. } => title.clone(),
        ProblemItem::Episode {
            show,
            season: 0,
            number,
            title,
            ..
        } => format!("{show} · Specials E{number} · {title}"),
        ProblemItem::Episode {
            show,
            season,
            number,
            title,
            ..
        } => format!("{show} · S{season} E{number} · {title}"),
    }
}

fn count(count: u64, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

fn duration(duration: std::time::Duration) -> String {
    format!(
        "{:#}",
        SignedDuration::try_from(duration).unwrap_or(SignedDuration::MAX)
    )
}
