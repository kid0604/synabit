# Privacy Policy — Synabit

**Last updated:** May 10, 2026  
**Effective date:** May 10, 2026

This Privacy Policy describes how Synabit ("we", "us", "our") handles your information when you use the Synabit desktop and mobile application ("the App").

---

## 1. Our Commitment

Synabit is a **local-first** application. Your data is stored on your device, not on our servers. We are committed to protecting your privacy and being transparent about how the App works.

## 2. Information We Collect

### 2.1 Information We Do NOT Collect

- We do **not** collect, transmit, or store your notes, tasks, files, or any content you create in the App.
- We do **not** track your usage behavior, browsing activity, or feature usage.
- We do **not** use analytics, telemetry, or tracking cookies.
- We do **not** have access to your data at any time.

### 2.2 Information Stored Locally on Your Device

The App stores the following data **exclusively on your device**:

| Data | Storage Location | Purpose |
|---|---|---|
| Notes, tasks, events, files | Your chosen vault directory (local filesystem) | Core app functionality |
| App settings & preferences | OS app data directory | Configuration |
| Search index (FTS5) | SQLite database in app data directory | Full-text search |
| OAuth tokens | OS Keychain (macOS) / Credential Manager (Windows) / Android Keystore | Google Drive authentication |

### 2.3 Google Drive Integration (Optional)

If you choose to connect Google Drive:

- **What we access**: Files within your Google Drive account, limited to the scopes you authorize (`drive.file` for vault sync, `drive.readonly` for file browsing).
- **Authentication**: We use Google OAuth 2.0 with PKCE (Proof Key for Code Exchange) for secure authentication. Your Google credentials are never stored by the App — only OAuth tokens are stored in your operating system's secure keychain.
- **Data transfer**: When you use Drive Sync, your vault files are uploaded to and downloaded from **your own Google Drive account**. Data travels directly between your device and Google's servers. We do not proxy, intercept, or store this data.
- **Disconnecting**: You can disconnect Google Drive at any time from the App settings. This deletes all stored tokens from your device.

### 2.4 Subscription & Payment

If you purchase a subscription:

- Payments are processed by third-party payment providers (e.g., Stripe, Apple App Store, Google Play).
- We receive confirmation of your subscription status but do **not** store your payment card details.
- We may store your email address for subscription management and support purposes.

## 3. How We Use Information

Since we do not collect user data, there is minimal information usage:

| Purpose | Data Used |
|---|---|
| Subscription management | Email address (if provided during purchase) |
| Customer support | Information you voluntarily share when contacting us |

## 4. Data Sharing

We do **not** sell, rent, or share your personal data with any third party.

The only third-party services involved are:
- **Google APIs** — only when you voluntarily connect Google Drive
- **Payment processors** — only when you purchase a subscription

## 5. Data Security

- All vault data remains on your local filesystem under your OS user permissions.
- OAuth tokens are stored in your operating system's native secure storage (macOS Keychain, Windows Credential Manager, Android Keystore).
- The App uses PKCE for OAuth flows to prevent authorization code interception.
- Path traversal protections prevent unauthorized filesystem access.

## 6. Data Retention & Deletion

- **Your content**: Stored locally. Delete your vault folder to remove all content.
- **App data**: Uninstall the App to remove all cached data, search indices, and settings.
- **Google Drive tokens**: Disconnect from the App settings, or revoke access from your [Google Account security page](https://myaccount.google.com/permissions).
- **Subscription data**: Contact us at privacy@synabit.app to request deletion of your account information.

## 7. Children's Privacy

Synabit is not directed at children under 13. We do not knowingly collect information from children under 13. If you believe a child has provided us with personal information, please contact us at privacy@synabit.app.

## 8. International Users

The App processes all data locally on your device. If you use Google Drive sync, your data is subject to Google's own privacy practices and data processing locations.

## 9. Changes to This Policy

We may update this Privacy Policy from time to time. We will notify users of material changes through the App or on our website. Your continued use of the App after changes constitutes acceptance of the updated policy.

## 10. Contact Us

If you have questions about this Privacy Policy:

- **Email**: privacy@synabit.app
- **GitHub**: https://github.com/synabit/synabit/issues

---

*This Privacy Policy is provided in compliance with Google API Services User Data Policy and applicable data protection regulations.*
