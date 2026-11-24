use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CommandContext, ErrorCode, LogicalTime, ResourceError, Result};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueMessage {
    pub id: String,
    pub body: Value,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
    pub published_at: LogicalTime,
    pub available_at: LogicalTime,
    pub receive_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_handle: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "action"
)]
pub enum QueueCommand {
    Publish {
        body: Value,
        #[serde(default)]
        attributes: BTreeMap<String, String>,
        #[serde(default)]
        delay_ms: u64,
    },
    Receive {
        #[serde(default = "default_receive_count")]
        max_messages: usize,
        #[serde(default = "default_visibility_timeout")]
        visibility_timeout_ms: u64,
    },
    Ack {
        receipt_handle: String,
    },
    Nack {
        receipt_handle: String,
        #[serde(default)]
        delay_ms: u64,
    },
    ExtendVisibility {
        receipt_handle: String,
        visibility_timeout_ms: u64,
    },
    Size,
}

const fn default_receive_count() -> usize {
    1
}

const fn default_visibility_timeout() -> u64 {
    30_000
}

impl QueueCommand {
    #[must_use]
    pub const fn action(&self) -> &'static str {
        match self {
            Self::Publish { .. } => "publish",
            Self::Receive { .. } => "receive",
            Self::Ack { .. } => "ack",
            Self::Nack { .. } => "nack",
            Self::ExtendVisibility { .. } => "extend_visibility",
            Self::Size => "size",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "result", content = "value")]
pub enum QueueResult {
    Published(QueueMessage),
    Messages(Vec<QueueMessage>),
    Acknowledged(bool),
    Released(bool),
    VisibilityExtended(bool),
    Size(usize),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct StoredMessage {
    id: String,
    body: Value,
    attributes: BTreeMap<String, String>,
    published_at: LogicalTime,
    available_at: LogicalTime,
    receive_count: u32,
    receipt_handle: Option<String>,
}

impl StoredMessage {
    fn public(&self) -> QueueMessage {
        QueueMessage {
            id: self.id.clone(),
            body: self.body.clone(),
            attributes: self.attributes.clone(),
            published_at: self.published_at,
            available_at: self.available_at,
            receive_count: self.receive_count,
            receipt_handle: self.receipt_handle.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct QueueEngine {
    pub(crate) resources: BTreeMap<String, VecDeque<StoredMessage>>,
    pub(crate) next_message: u64,
    pub(crate) next_receipt: u64,
}

impl QueueEngine {
    pub(crate) fn lease_is_current(
        &self,
        resource: &str,
        message_id: &str,
        receipt_handle: &str,
        now: LogicalTime,
    ) -> bool {
        self.resources.get(resource).is_some_and(|messages| {
            messages.iter().any(|message| {
                message.id == message_id
                    && message.receipt_handle.as_deref() == Some(receipt_handle)
                    && message.available_at > now
            })
        })
    }

    pub(crate) fn execute(
        &mut self,
        resource: &str,
        context: &CommandContext,
        command: QueueCommand,
    ) -> Result<QueueResult> {
        match command {
            QueueCommand::Publish {
                body,
                attributes,
                delay_ms,
            } => {
                self.next_message = self.next_message.saturating_add(1);
                let message = StoredMessage {
                    id: format!("msg_{:016x}", self.next_message),
                    body,
                    attributes,
                    published_at: context.at,
                    available_at: add_time(context.at, delay_ms)?,
                    receive_count: 0,
                    receipt_handle: None,
                };
                self.resources
                    .entry(resource.to_owned())
                    .or_default()
                    .push_back(message.clone());
                Ok(QueueResult::Published(message.public()))
            }
            QueueCommand::Receive {
                max_messages,
                visibility_timeout_ms,
            } => {
                let available = self.resources.get(resource).map_or(0, |messages| {
                    messages
                        .iter()
                        .filter(|message| message.available_at <= context.at)
                        .take(max_messages)
                        .count()
                });
                let mut receipts = Vec::with_capacity(available);
                for _ in 0..available {
                    self.next_receipt = self.next_receipt.saturating_add(1);
                    receipts.push(format!("rct_{:016x}", self.next_receipt));
                }
                let mut receipts = receipts.into_iter();
                let messages = self.resources.entry(resource.to_owned()).or_default();
                let mut received = Vec::with_capacity(available);
                for message in messages
                    .iter_mut()
                    .filter(|message| message.available_at <= context.at)
                    .take(max_messages)
                {
                    let Some(receipt) = receipts.next() else {
                        break;
                    };
                    message.receive_count = message.receive_count.saturating_add(1);
                    message.receipt_handle = Some(receipt);
                    message.available_at = add_time(context.at, visibility_timeout_ms)?;
                    received.push(message.public());
                }
                Ok(QueueResult::Messages(received))
            }
            QueueCommand::Ack { receipt_handle } => {
                let messages = self.resources.entry(resource.to_owned()).or_default();
                let position = messages.iter().position(|message| {
                    message.receipt_handle.as_deref() == Some(&receipt_handle)
                        && message.available_at > context.at
                });
                if let Some(position) = position {
                    messages.remove(position);
                }
                Ok(QueueResult::Acknowledged(position.is_some()))
            }
            QueueCommand::Nack {
                receipt_handle,
                delay_ms,
            } => {
                let released = update_receipt(
                    self.resources.entry(resource.to_owned()).or_default(),
                    &receipt_handle,
                    context.at,
                    |message| {
                        message.available_at = add_time(context.at, delay_ms)?;
                        message.receipt_handle = None;
                        Ok(())
                    },
                )?;
                Ok(QueueResult::Released(released))
            }
            QueueCommand::ExtendVisibility {
                receipt_handle,
                visibility_timeout_ms,
            } => {
                let extended = update_receipt(
                    self.resources.entry(resource.to_owned()).or_default(),
                    &receipt_handle,
                    context.at,
                    |message| {
                        message.available_at = add_time(context.at, visibility_timeout_ms)?;
                        Ok(())
                    },
                )?;
                Ok(QueueResult::VisibilityExtended(extended))
            }
            QueueCommand::Size => Ok(QueueResult::Size(
                self.resources.get(resource).map_or(0, VecDeque::len),
            )),
        }
    }
}

fn add_time(time: LogicalTime, millis: u64) -> Result<LogicalTime> {
    time.checked_add(millis)
        .map_err(|error| ResourceError::new(ErrorCode::InvalidValue, error.to_string()))
}

fn update_receipt(
    messages: &mut VecDeque<StoredMessage>,
    receipt_handle: &str,
    now: LogicalTime,
    update: impl FnOnce(&mut StoredMessage) -> Result<()>,
) -> Result<bool> {
    let Some(message) = messages.iter_mut().find(|message| {
        message.receipt_handle.as_deref() == Some(receipt_handle) && message.available_at > now
    }) else {
        return Ok(false);
    };
    update(message)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{QueueCommand, QueueEngine, QueueResult};
    use crate::{CommandContext, CommandId, CommandMetadata, LogicalTime};

    fn context(at: i64) -> CommandContext {
        CommandContext {
            command_id: CommandId(1),
            at: LogicalTime::from_unix_millis(at),
            metadata: CommandMetadata::default(),
        }
    }

    #[test]
    fn unacknowledged_messages_become_visible_again() {
        let mut engine = QueueEngine::default();
        let published = engine.execute(
            "orders",
            &context(0),
            QueueCommand::Publish {
                body: json!({"order": "one"}),
                attributes: BTreeMap::new(),
                delay_ms: 0,
            },
        );
        assert!(matches!(published, Ok(QueueResult::Published(_))));
        let first = engine.execute(
            "orders",
            &context(0),
            QueueCommand::Receive {
                max_messages: 1,
                visibility_timeout_ms: 10,
            },
        );
        assert!(matches!(first, Ok(QueueResult::Messages(messages)) if messages.len() == 1));
        assert_eq!(
            engine.execute(
                "orders",
                &context(5),
                QueueCommand::Receive {
                    max_messages: 1,
                    visibility_timeout_ms: 10,
                }
            ),
            Ok(QueueResult::Messages(Vec::new()))
        );
        let second = engine.execute(
            "orders",
            &context(10),
            QueueCommand::Receive {
                max_messages: 1,
                visibility_timeout_ms: 10,
            },
        );
        assert!(
            matches!(second, Ok(QueueResult::Messages(messages)) if messages[0].receive_count == 2)
        );
    }
}
