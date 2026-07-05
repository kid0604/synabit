use reqwest::Client;
use serde_json::json;
use std::env;

pub async fn send_license_email(to_email: &str, license_key: &str) -> Result<(), String> {
    let api_key = match env::var("RESEND_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            tracing::warn!("RESEND_API_KEY is not set. Email will not be sent.");
            return Ok(());
        }
    };

    let client = Client::new();
    let url = "https://api.resend.com/emails";

    let from_email = "noreply@synabit.net";

    let html_content = format!(
        r#"
        <div style="font-family: sans-serif; max-width: 600px; margin: 0 auto; padding: 20px; border: 1px solid #eee; border-radius: 8px;">
            <h2 style="color: #333;">Thank you for purchasing Synabit Pro!</h2>
            <p>Your order has been successfully confirmed.</p>
            <p>Below is your <strong>License Key</strong> to activate the software:</p>
            <div style="background-color: #f5f5f5; padding: 15px; border-radius: 6px; text-align: center; margin: 20px 0;">
                <code style="font-size: 24px; color: #007bff; font-weight: bold;">{}</code>
            </div>
            <p><strong>Activation Instructions:</strong></p>
            <ol>
                <li>Open the Synabit Desktop application.</li>
                <li>Go to <strong>Settings > License</strong>.</li>
                <li>Paste the key above and click <strong>Activate</strong>.</li>
            </ol>
            <p>Best regards,<br/>The Synabit Team</p>
        </div>
        "#,
        license_key
    );

    let payload = json!({
        "from": format!("Synabit <{}>", from_email),
        "to": [to_email],
        "subject": "Your Synabit Pro License Key",
        "html": html_content
    });

    let res = client
        .post(url)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to send email request: {}", e))?;

    if res.status().is_success() {
        tracing::info!("License email sent successfully to {}", to_email);
        Ok(())
    } else {
        let error_msg = res.text().await.unwrap_or_default();
        tracing::error!("Failed to send email to {}: {}", to_email, error_msg);
        Err(error_msg)
    }
}
