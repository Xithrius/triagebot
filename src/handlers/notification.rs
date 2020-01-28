//! Purpose: Allow any user to ping a pre-selected group of people on GitHub via comments.
//!
//! The set of "teams" which can be pinged is intentionally restricted via configuration.
//!
//! Parsing is done in the `parser::command::ping` module.

use crate::db::notifications;
use crate::{
    github::{self, Event},
    handlers::Context,
};
use anyhow::Context as _;
use regex::Regex;
use std::convert::TryFrom;

lazy_static::lazy_static! {
    static ref PING_RE: Regex = Regex::new(r#"@([-\w\d]+)"#,).unwrap();
}

pub async fn handle(ctx: &Context, event: &Event) -> anyhow::Result<()> {
    if let Event::Issue(e) = event {
        if e.action != github::IssuesAction::Opened {
            // skip events other than opening the issue to avoid retriggering commands in the
            // issue body
            return Ok(());
        }
    }

    if let Event::IssueComment(e) = event {
        if e.action != github::IssueCommentAction::Created {
            // skip events other than creating a comment to avoid
            // renotifying
            //
            // FIXME: implement smart tracking to allow rerunning only if
            // the notification is "new" (i.e. edit adds a ping)
            return Ok(());
        }
    }

    let body = match event.comment_body() {
        Some(v) => v,
        // Skip events that don't have comment bodies associated
        None => return Ok(()),
    };

    let caps = PING_RE
        .captures_iter(body)
        .map(|c| c.get(1).unwrap().as_str().to_owned())
        .collect::<Vec<_>>();
    for login in caps {
        let user = github::User { login };
        let id = user
            .get_id(&ctx.github)
            .await
            .with_context(|| format!("failed to get user {} ID", user.login))?;
        let id = match id {
            Some(id) => id,
            // If the user was not in the team(s) then just don't record it.
            None => return Ok(()),
        };
        notifications::record_ping(
            &ctx.db,
            &notifications::Notification {
                user_id: i64::try_from(id)
                    .with_context(|| format!("user id {} out of bounds", id))?,
                username: user.login,
                origin_url: event.html_url().unwrap().to_owned(),
                origin_html: body.to_owned(),
                time: event.time(),
            },
        )
        .await
        .context("failed to record ping")?;
    }

    Ok(())
}
