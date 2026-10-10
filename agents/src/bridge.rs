//! The line protocol to ErisReader's main process: one JSON object per line on stdin and stdout.
//!
//! The host sends commands (`{"id": n, "command": ...}`), answered by `{"reply": n, "ok": ...}`,
//! and answers this side's questions (`{"ask": n, ...}`) with `{"answer": n, ...}`. Events
//! (`{"event": ...}`) go to the host unasked.

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

#[derive(Clone, Default)]
pub struct Bridge(Arc<Inner>);

#[derive(Default)]
struct Inner {
    out: Mutex<()>,
    next: AtomicU64,
    asks: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
}

impl Bridge {
    fn write(&self, value: &Value) {
        let _lock = self.0.out.lock().unwrap();
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{value}");
        let _ = out.flush();
    }

    pub fn event(&self, event: &str, mut fields: Value) {
        fields["event"] = json!(event);
        self.write(&fields);
    }

    pub fn reply(&self, id: u64, result: Result<Value>) {
        self.write(&match result {
            Ok(value) => json!({"reply": id, "ok": true, "value": value}),
            Err(error) => json!({"reply": id, "ok": false, "error": format!("{error:#}")}),
        });
    }

    /// Asks the host and waits for its answer.
    pub async fn ask(&self, question: Value) -> Result<Value> {
        let id = self.0.next.fetch_add(1, Ordering::Relaxed);
        let (send, receive) = oneshot::channel();
        self.0.asks.lock().unwrap().insert(id, send);
        let mut line = question;
        line["ask"] = json!(id);
        self.write(&line);
        let answer = receive
            .await
            .map_err(|_| anyhow!("ErisReader did not answer"))?;
        match answer.get("error").and_then(Value::as_str) {
            Some(error) => Err(anyhow!("{error}")),
            None => Ok(answer),
        }
    }

    /// Hands the host's answer to whoever asked.
    pub fn answer(&self, answer: Value) {
        let Some(id) = answer["answer"].as_u64() else {
            return;
        };
        if let Some(waiting) = self.0.asks.lock().unwrap().remove(&id) {
            let _ = waiting.send(answer);
        }
    }
}
