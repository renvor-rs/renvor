//! The mail port with its recording substitute.
//!
//! ```sh
//! cargo run -p renvor --example mail --features capability-mail
//! ```
//!
//! The SMTP adapter is `renvor::mail::smtp` behind `renvor-mail/smtp`; it implements the same
//! `Mailer` trait. Sending is not idempotent: a retry is a durable job with an idempotency key.

use std::sync::Arc;

use renvor::kernel::observe::OsEntropy;
use renvor::mail::{Address, Mailbox};
use renvor::{Mailer as _, Message, RecordingMailbox};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mailbox = RecordingMailbox::new(Arc::new(OsEntropy::new()));
    let sender =
        Mailbox::new(Address::new("no-reply@example.test")?).with_display_name("Example")?;
    let message = Message::builder(sender)
        .to(Mailbox::new(Address::new("ada@example.test")?))
        .subject("Welcome")
        .text("Hello, Ada.\n".to_owned())
        .html("<p>Hello, Ada.</p>".to_owned())
        .build()?;
    // `Debug` prints counts and lengths, never a recipient, subject, or body.
    println!("sending {message:?}");
    let receipt = mailbox.send(message).await?;
    println!("receipt: {}", receipt.id().as_str());
    println!("recorded: {}", mailbox.delivered());

    // Header injection is unrepresentable: an address or subject with a line break is refused
    // at construction, before any transport could see it.
    println!(
        "injected address refused: {}",
        Address::new("ada@example.test\r\nBcc: eve@example.test").is_err()
    );
    Ok(())
}
