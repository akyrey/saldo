//! `GET /events/stream`: the household's events as Server-Sent Events, so an
//! open browser tab learns that another member changed something.
//!
//! A *stream* is an async iterator: the response body pulls the next item
//! whenever it can send more. It plays the role of a Go goroutine writing to
//! an `http.Flusher` in a loop, or a PHP generator feeding a streamed
//! response, but the HTTP library drives it and stops it when the client
//! disconnects.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::Stream;
use saldo_app::App;
use saldo_app::ctx::HouseholdCtx;
use saldo_app::events::EventNotice;
use serde::Deserialize;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::Auth;
use crate::error::{ApiError, ErrorEnvelope};
use crate::extract::ApiQuery;
use crate::state::AppState;

/// Keep-alive comments keep proxies from closing an idle stream.
const KEEP_ALIVE: Duration = Duration::from_secs(15);
/// Fallback poll for events written outside this process (the importer).
const POLL: Duration = Duration::from_secs(15);
/// Events read from the database at a time.
const BATCH: i64 = 100;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(stream_events))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StreamQuery {
    /// Start after this `seq`. The `Last-Event-ID` header wins; without
    /// either, the stream starts with the next new event.
    pub after_seq: Option<i64>,
}

/// Each message is `id: <seq>` and `data: {"seq", "type", "entity_type",
/// "entity_id"}`. Reconnecting with `Last-Event-ID` resumes without gaps.
#[utoipa::path(get, path = "/events/stream", tag = "events", params(StreamQuery),
    responses(
        (status = 200, description = "A `text/event-stream` of event notices", content_type = "text/event-stream", body = EventNotice),
        (status = 401, body = ErrorEnvelope)),
    security(("bearer" = []), ("cookie" = [])))]
pub async fn stream_events(
    State(state): State<AppState>,
    Auth(auth): Auth,
    headers: HeaderMap,
    ApiQuery(query): ApiQuery<StreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let start = match last_event_id(&headers).or(query.after_seq) {
        Some(seq) => seq,
        None => state.app.events.latest_seq(&auth.ctx).await?,
    };
    let cursor = Cursor {
        app: state.app.clone(),
        ctx: auth.ctx,
        after: start,
        pending: VecDeque::new(),
        wake: state.events.subscribe(),
        shutdown: state.shutdown.clone(),
    };
    // `unfold` builds a stream from a state value and a function that turns
    // it into (next item, next state), or `None` to end the stream.
    let stream = futures_util::stream::unfold(cursor, |mut cursor| async move {
        let notice = cursor.next().await?;
        Some((Ok(to_event(&notice)), cursor))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(KEEP_ALIVE)))
}

fn last_event_id(headers: &HeaderMap) -> Option<i64> {
    headers
        .get("last-event-id")?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn to_event(notice: &EventNotice) -> Event {
    let event = Event::default().id(notice.seq.to_string());
    // Serialising four plain fields cannot fail; fall back to an empty object.
    event
        .json_data(notice)
        .unwrap_or_else(|_| Event::default().data("{}"))
}

/// Where one stream is in the household's log.
struct Cursor {
    app: App,
    ctx: HouseholdCtx,
    after: i64,
    pending: VecDeque<EventNotice>,
    wake: watch::Receiver<u64>,
    shutdown: CancellationToken,
}

impl Cursor {
    /// The next notice, waiting for one if needed. `None` ends the stream:
    /// on shutdown, or on a database error (the client reconnects with
    /// `Last-Event-ID` and loses nothing).
    async fn next(&mut self) -> Option<EventNotice> {
        loop {
            if let Some(notice) = self.pending.pop_front() {
                self.after = notice.seq;
                return Some(notice);
            }
            match self
                .app
                .events
                .notices_after(&self.ctx, self.after, BATCH)
                .await
            {
                Ok(notices) if !notices.is_empty() => {
                    self.pending.extend(notices);
                    continue;
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::error!(%error, "event stream stopped");
                    return None;
                }
            }
            tokio::select! {
                () = self.shutdown.cancelled() => return None,
                _ = self.wake.changed() => {}
                () = tokio::time::sleep(POLL) => {}
            }
        }
    }
}
