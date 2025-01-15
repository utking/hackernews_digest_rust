use crate::{DigestItem, SmtpConfig};
use lettre::message::{MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{SmtpTransport, Transport};

pub struct Sender {
    config: SmtpConfig,
}

impl Sender {
    pub fn new(config: &SmtpConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    pub async fn send_email(
        &self,
        send_to: &str,
        subject: &str,
        text_body: &str,
        html_body: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let email = lettre::Message::builder()
            .from(self.config.from.parse()?)
            .to(send_to.parse()?)
            .subject(subject)
            .multipart(
                MultiPart::mixed().multipart(
                    MultiPart::alternative()
                        .singlepart(SinglePart::plain(String::from(text_body)))
                        .multipart(
                            MultiPart::related()
                                .singlepart(SinglePart::html(String::from(html_body))),
                        ),
                ),
            )?;

        let creds = Credentials::new(self.config.username.clone(), self.config.password.clone());
        let mailer = SmtpTransport::relay(&self.config.host)?
            .credentials(creds)
            .build();

        match mailer.send(&email) {
            Ok(_) => return Ok(()),
            Err(e) => eprintln!("Could not send email: {:?}", e),
        }

        Ok(())
    }
}

/// Convert a digest to an HTML string
pub fn digest_to_html(digest: &Vec<DigestItem>) -> String {
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
pub fn digest_to_text(digest: &Vec<DigestItem>) -> String {
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
