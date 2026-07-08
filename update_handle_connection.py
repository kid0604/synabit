import re

with open('sync-server/src/mailbox.rs', 'r') as f:
    content = f.read()

handle_connection_new = """        // Create an mpsc channel for this connection to receive push notifications
        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<u64>(100);

        // Register the channel in active_subscriptions
        {
            let mut subs = self.active_subscriptions.write().await;
            subs.entry(vault_hash_hex.clone()).or_default().push(notify_tx);
        }

        // Spawn a task to read requests, because read_message is not cancellation-safe.
        let (req_tx, mut req_rx) = tokio::sync::mpsc::channel::<Result<Option<MailboxRequest>, anyhow::Error>>(10);
        let mut recv_task = tokio::spawn(async move {
            loop {
                let req = protocol::read_message(&mut recv).await;
                if req_tx.send(req).await.is_err() {
                    break;
                }
            }
        });

        loop {
            tokio::select! {
                req_opt = req_rx.recv() => {
                    let request = match req_opt {
                        Some(Ok(Some(msg))) => msg,
                        Some(Ok(None)) | None => {
                            debug!(vault = vault_hash_hex, device = device_id, "client closed stream");
                            break;
                        }
                        Some(Err(e)) => {
                            warn!(vault = vault_hash_hex, device = device_id, error = %e, "error reading from stream");
                            break;
                        }
                    };

                    // Handle request
                    let response_res = self.handle_request(
                        &vault_hash_hex,
                        &device_id,
                        request,
                    ).await;

                    match response_res {
                        Ok(resp) => {
                            if let Err(e) = protocol::write_message(&mut send, &resp).await {
                                warn!(vault = vault_hash_hex, device = device_id, error = %e, "error writing response");
                                break;
                            }
                        }
                        Err(e) => {
                            warn!(vault = vault_hash_hex, device = device_id, error = %e, "error handling request");
                            let err_resp = MailboxResponse::Error { message: e.to_string() };
                            let _ = protocol::write_message(&mut send, &err_resp).await;
                            break;
                        }
                    }
                }
                Some(trigger_seq) = notify_rx.recv() => {
                    debug!(vault = vault_hash_hex, device = device_id, "pushing NotifyNewData");
                    let response = MailboxResponse::NotifyNewData { trigger_seq };
                    if let Err(e) = protocol::write_message(&mut send, &response).await {
                        warn!(vault = vault_hash_hex, device = device_id, error = %e, "error writing NotifyNewData");
                        break;
                    }
                }
            }
        }

        recv_task.abort();

        // Cleanup subscription
        {
            let mut subs = self.active_subscriptions.write().await;
            if let Some(list) = subs.get_mut(&vault_hash_hex) {
                // Since Sender doesn't have an ID, we just remove all closed channels
                list.retain(|tx| !tx.is_closed());
                if list.is_empty() {
                    subs.remove(&vault_hash_hex);
                }
            }
        }"""

content = re.sub(r'        loop \{\n\s+let request: MailboxRequest = match protocol::read_message.*?                \}\n            \}\n        \}', handle_connection_new, content, flags=re.MULTILINE|re.DOTALL)

with open('sync-server/src/mailbox.rs', 'w') as f:
    f.write(content)

