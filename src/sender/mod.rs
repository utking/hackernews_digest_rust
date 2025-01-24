use crate::config::{SmtpConfig, TelegramConfig};
use crate::DigestItem;
use lettre::message::{MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{SmtpTransport, Transport};
use teloxide::utils::markdown;

pub enum Sender {
    Dummy(DummySender),
    Smtp(SmtpSender),
    Telegram(TelegramSender),
}

impl Sender {
    pub async fn send_digest(
        &self,
        subj: &str,
        digest: &[DigestItem],
    ) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Sender::Dummy(sender) => sender.send_digest(subj, digest).await,
            Sender::Smtp(sender) => sender.send_digest(subj, digest).await,
            Sender::Telegram(sender) => sender.send_digest(subj, digest).await,
        }
    }
}

pub trait DigestSender {
    async fn send_digest(
        &self,
        subj: &str,
        digest: &[DigestItem],
    ) -> Result<(), Box<dyn std::error::Error>>;
}

pub struct DummySender {}

pub struct SmtpSender {
    config: SmtpConfig,
}

pub struct TelegramSender {
    config: TelegramConfig,
}

impl SmtpSender {
    pub fn new(config: &SmtpConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }
}

impl TelegramSender {
    #[must_use]
    pub fn new(config: &TelegramConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }
}

impl DigestSender for SmtpSender {
    async fn send_digest(
        &self,
        subj: &str,
        digest: &[DigestItem],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text_body = digest_to_text(digest);
        let html_body = digest_to_html(digest);
        let email = lettre::Message::builder()
            .from(self.config.from.parse()?)
            .to(self.config.to.parse()?)
            .subject(format!("{subj} {}", self.config.subject))
            .multipart(
                MultiPart::mixed().multipart(
                    MultiPart::alternative()
                        .singlepart(SinglePart::plain(text_body))
                        .multipart(MultiPart::related().singlepart(SinglePart::html(html_body))),
                ),
            )?;

        let creds = Credentials::new(self.config.username.clone(), self.config.password.clone());
        let mailer = SmtpTransport::relay(&self.config.host)?
            .credentials(creds)
            .build();

        match mailer.send(&email) {
            Ok(_) => return Ok(()),
            Err(e) => eprintln!("Could not send email: {e:?}"),
        }

        Ok(())
    }
}

impl DigestSender for DummySender {
    async fn send_digest(
        &self,
        _subj: &str,
        digest: &[DigestItem],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut body = String::from("Hi!\n\n");

        for item in digest {
            body.push_str(&format!(
                "* {title} - {url}\n",
                url = item.news_url,
                title = item.news_title
            ));
        }
        body.push_str(format!("\nGenerated: {}", formatted_now()).as_str());

        println!("{body}");

        Ok(())
    }
}

impl DigestSender for TelegramSender {
    async fn send_digest(
        &self,
        _subj: &str,
        digest: &[DigestItem],
    ) -> Result<(), Box<dyn std::error::Error>> {
        use teloxide::prelude::*;

        let bot = Bot::new(&self.config.token);

        for item in digest {
            let body = format!(
                "*[{}]({})*",
                markdown::escape(&item.news_title),
                markdown::escape_link_url(&item.news_url),
            );

            match bot
                .send_message(self.config.chat_id.clone(), body)
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .send()
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Could not send message: {e:?}");
                    return Err(Box::new(e));
                }
            }
        }

        Ok(())
    }
}

/// Convert a digest to an HTML string
pub fn digest_to_html(digest: &[DigestItem]) -> String {
    let mut body = String::from("<html><head>HackerNews Digest</head><body><p>Hi!</p><div><ul>");
    for item in digest {
        body.push_str(&format!(
            "<li><a href=\"{url}\">{title}</a></li>",
            url = item.news_url,
            title = item.news_title
        ));
    }
    body.push_str(
        format!(
            "</ul></div><p>Generated: {}</p></body></html>",
            formatted_now(),
        )
        .as_str(),
    );
    body
}

/// Convert a digest to a plain text string
pub fn digest_to_text(digest: &[DigestItem]) -> String {
    let mut body = String::from("Hi!\n\n");
    for item in digest {
        body.push_str(&format!(
            "* {title} - {url}\n",
            url = item.news_url,
            title = item.news_title
        ));
    }
    body.push_str(format!("\nGenerated: {}", formatted_now()).as_str());
    body
}

fn formatted_now() -> String {
    chrono::Local::now().to_rfc2822()
}
