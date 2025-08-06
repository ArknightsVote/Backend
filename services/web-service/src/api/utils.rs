use rand::{Rng as _, distr::Alphanumeric};

use crate::error::AppError;

pub async fn publish_and_ack(
    jetstream: &async_nats::jetstream::Context,
    subject: &'static str,
    data: Vec<u8>,
) -> Result<(), AppError> {
    let publish_ack = jetstream.publish(subject, data.into()).await?;
    publish_ack.await?;
    Ok(())
}

pub fn generate_random_string(length: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}
