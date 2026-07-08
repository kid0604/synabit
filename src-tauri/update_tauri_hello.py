import re

with open('src/p2p/transport.rs', 'r') as f:
    content = f.read()

send_auth_new = """    async fn send_auth(&self) -> AppResult<()> {
        let mut session = self.session.lock().await;
        let s = session
            .as_mut()
            .ok_or_else(|| AppError::General("no active session".to_string()))?;

        // 1. Send Hello for version negotiation
        let hello = MailboxRequest::Hello { version: 1 };
        write_message(&mut s.send, &hello)
            .await
            .map_err(|e| AppError::General(format!("hello send failed: {}", e)))?;

        let hello_resp: MailboxResponse = read_message(&mut s.recv)
            .await
            .map_err(|e| AppError::General(format!("hello recv failed: {}", e)))?
            .ok_or_else(|| AppError::General("server closed connection during hello".to_string()))?;

        match hello_resp {
            MailboxResponse::HelloOk { server_version, max_bytes } => {
                info!("Server protocol version {}, max_bytes: {}", server_version, max_bytes);
            }
            MailboxResponse::Error { message } => {
                error!("Server hello failed: {}", message);
                // We could fallback or fail depending on message, but for now we continue
                // since the server might be an older version that expects Auth first.
                // Wait, if it expects Auth first, it might have dropped the connection or returned Error.
                // If it returned Error, we probably should fail.
                drop(session);
                self.close().await;
                return Err(AppError::General(format!("protocol negotiation failed: {}", message)));
            }
            _ => {
                drop(session);
                self.close().await;
                return Err(AppError::General("unexpected response to Hello".to_string()));
            }
        }

        // 2. Send Auth
        let auth = MailboxRequest::Auth {
            vault_hash: self.vault_hash,
            mailbox_token: self.mailbox_token,
            device_id: self.device_id.clone(),
        };

        write_message(&mut s.send, &auth)
            .await
            .map_err(|e| AppError::General(format!("auth send failed: {}", e)))?;

        let response: MailboxResponse = read_message(&mut s.recv)
            .await
            .map_err(|e| AppError::General(format!("auth recv failed: {}", e)))?
            .ok_or_else(|| {
                AppError::General("server closed connection during auth".to_string())
            })?;

        match response {"""

content = re.sub(r'    async fn send_auth\(&self\) -> AppResult<\(\)> \{.*?match response \{', send_auth_new, content, flags=re.MULTILINE|re.DOTALL)

with open('src/p2p/transport.rs', 'w') as f:
    f.write(content)
