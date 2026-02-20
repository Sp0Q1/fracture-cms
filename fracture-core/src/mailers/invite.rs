use include_dir::{include_dir, Dir};
use loco_rs::mailer::{Args, Mailer, MailerOpts};
use loco_rs::prelude::*;
use serde_json::json;

static INVITE_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/src/mailers/invite/invite");

pub struct InviteMailer;

impl Mailer for InviteMailer {
    fn opts() -> MailerOpts {
        MailerOpts {
            from: "Fracture CMS <noreply@fracture-cms.local>".to_string(),
            ..Default::default()
        }
    }
}

impl InviteMailer {
    /// Send an invitation email.
    ///
    /// # Errors
    ///
    /// Returns an error if the mailer fails to enqueue the email.
    pub async fn send_invite(
        ctx: &AppContext,
        to_email: &str,
        org_name: &str,
        inviter_name: &str,
        role: &str,
        accept_url: &str,
    ) -> Result<()> {
        Self::mail_template(
            ctx,
            &INVITE_DIR,
            Args {
                to: to_email.to_string(),
                locals: json!({
                    "org_name": org_name,
                    "inviter_name": inviter_name,
                    "role": role,
                    "accept_url": accept_url,
                }),
                ..Default::default()
            },
        )
        .await?;
        Ok(())
    }
}
